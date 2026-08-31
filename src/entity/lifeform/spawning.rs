use super::{Lifeform, LifeformId, LifeformMaterials, LifeformSystem};
use crate::entity::physics::update_collider;
use crate::{
    BUILT_IN_FURNITURE, BrokenTile, CHUNK_SIZE, ChunkActivity, ChunkPos, Collider, Layer, ObjectId,
    PhysicsConfig, TileId, TilePos, Transform, World,
};
use hecs::{Entity, World as EntityWorld};
use std::collections::HashMap;
use std::time::Duration;

const MAX_LOW_RATE_PHYSICS_STEPS: usize = 4;
const SPAWN_CLEARANCE_EPSILON: f32 = 0.001;
const MACHINE_TARGET_SEARCH_RADIUS_CHUNKS: u32 = 2;
pub const GLOWGNAT_MIN_MACHINERY_ATTENTION: u32 = 48;
pub const DEFAULT_HOSTILE_MACHINERY_ATTENTION: u32 = 32;
const FLYING_SPAWN_CANDIDATES: u64 = 16;
const BLOCK_ATTACK_CONTACT_EPSILON: f32 = 0.01;
const SEPARATION_RADIUS: f32 = 2.5;
const SEPARATION_CELL_SIZE: f32 = 3.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LifeformSimulationConfig {
    pub player_chunk_radius: [u32; 2],
    pub machinery_chunk_radius: [u32; 2],
    pub player_spawn_interval: Duration,
    pub machinery_refresh_interval: Duration,
    pub machinery_spawn_interval: Duration,
    pub max_player_chunks_per_spawn: usize,
    pub max_machinery_chunks_per_spawn: usize,
    pub spawn_attempts_per_chunk: usize,
    pub maximum_lifeforms: usize,
    pub maximum_lifeforms_per_chunk: usize,
    pub visibility_margin_tiles: u32,
    /// Baseline pressure around the player when no machinery is nearby.
    pub ambient_spawn_attention: u32,
    /// Attention required to sustain one lifeform in an active area.
    pub attention_per_lifeform: u32,
    /// Attention at which each scheduled chunk check is guaranteed to try spawning.
    pub attention_for_guaranteed_spawn: u32,
    /// Local machinery attention required before roaming lifeforms become hostile.
    pub minimum_hostile_attention: u32,
    /// Global per-update cap for sparse terrain mutations by lifeforms.
    pub max_block_attacks_per_update: usize,
}

impl Default for LifeformSimulationConfig {
    fn default() -> Self {
        Self {
            player_chunk_radius: [2, 1],
            machinery_chunk_radius: [1, 1],
            player_spawn_interval: Duration::from_secs(1),
            machinery_refresh_interval: Duration::from_millis(250),
            machinery_spawn_interval: Duration::from_secs(4),
            max_player_chunks_per_spawn: 8,
            max_machinery_chunks_per_spawn: 4,
            spawn_attempts_per_chunk: 3,
            maximum_lifeforms: 128,
            maximum_lifeforms_per_chunk: 4,
            visibility_margin_tiles: 4,
            ambient_spawn_attention: 1,
            attention_per_lifeform: 24,
            attention_for_guaranteed_spawn: 64,
            minimum_hostile_attention: DEFAULT_HOSTILE_MACHINERY_ATTENTION,
            max_block_attacks_per_update: 32,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LifeformSpawnView {
    pub player_position: [f32; 2],
    pub visible_minimum: [f32; 2],
    pub visible_maximum: [f32; 2],
}

impl LifeformSpawnView {
    pub fn new(player_position: [f32; 2], first_corner: [f32; 2], second_corner: [f32; 2]) -> Self {
        Self {
            player_position,
            visible_minimum: [
                first_corner[0].min(second_corner[0]),
                first_corner[1].min(second_corner[1]),
            ],
            visible_maximum: [
                first_corner[0].max(second_corner[0]),
                first_corner[1].max(second_corner[1]),
            ],
        }
    }

    fn intersects_visible_area(
        self,
        position: [f32; 2],
        half_extents: [f32; 2],
        margin: f32,
    ) -> bool {
        let minimum = [position[0] - half_extents[0], position[1] - half_extents[1]];
        let maximum = [position[0] + half_extents[0], position[1] + half_extents[1]];
        maximum[0] >= self.visible_minimum[0] - margin
            && minimum[0] <= self.visible_maximum[0] + margin
            && maximum[1] >= self.visible_minimum[1] - margin
            && minimum[1] <= self.visible_maximum[1] + margin
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LifeformSimulationUpdate {
    pub active_machinery_chunks: usize,
    pub player_chunks_checked: usize,
    pub machinery_chunks_checked: usize,
    pub spawn_attempts: usize,
    pub spawned: usize,
    pub lifeforms_updated_near_player: usize,
    pub lifeforms_updated_near_machinery: usize,
    pub population_capped: bool,
    pub machine_attacks: usize,
    pub machine_damage: u32,
    pub machines_disabled: usize,
    pub player_machine_attention: u32,
    pub distant_machine_attention: u32,
    pub block_attacks: usize,
    pub block_damage: u32,
    pub blocks_broken: Vec<BrokenTile>,
}

#[derive(Clone, Copy, Debug)]
struct Occupant {
    position: [f32; 2],
    half_extents: [f32; 2],
}

#[derive(Clone, Copy, Debug)]
struct MachineTarget {
    object: ObjectId,
    position: [f32; 2],
    half_extents: [f32; 2],
    noise_emission: u16,
}

#[derive(Clone, Copy, Debug)]
struct ChunkRect {
    minimum: ChunkPos,
    maximum: ChunkPos,
}

impl ChunkRect {
    fn around(world: &World, position: [f32; 2], radius: [u32; 2]) -> Self {
        let centre = position_chunk(world, position).unwrap_or(ChunkPos { x: 0, y: 0 });
        Self {
            minimum: ChunkPos {
                x: centre.x.saturating_sub(radius[0]),
                y: centre.y.saturating_sub(radius[1]),
            },
            maximum: ChunkPos {
                x: centre
                    .x
                    .saturating_add(radius[0])
                    .min(world.chunks_wide() - 1),
                y: centre
                    .y
                    .saturating_add(radius[1])
                    .min(world.chunks_high() - 1),
            },
        }
    }

    fn contains(self, chunk: ChunkPos) -> bool {
        (self.minimum.x..=self.maximum.x).contains(&chunk.x)
            && (self.minimum.y..=self.maximum.y).contains(&chunk.y)
    }

    fn len(self) -> usize {
        (self.maximum.x - self.minimum.x + 1) as usize
            * (self.maximum.y - self.minimum.y + 1) as usize
    }

    fn at(self, index: usize) -> ChunkPos {
        let width = (self.maximum.x - self.minimum.x + 1) as usize;
        ChunkPos {
            x: self.minimum.x + (index % width) as u32,
            y: self.minimum.y + (index / width) as u32,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LifeformSimulation {
    config: LifeformSimulationConfig,
    player_spawn_accumulator: Duration,
    machinery_refresh_accumulator: Duration,
    machinery_spawn_accumulator: Duration,
    player_chunk_cursor: usize,
    machinery_chunk_cursor: usize,
    spawn_sequence: u64,
    machinery_chunks: Vec<ChunkPos>,
    machinery_spawn_chunks: Vec<ChunkPos>,
    machine_attention_by_chunk: HashMap<ChunkPos, u32>,
    distant_attention_by_chunk: HashMap<ChunkPos, u32>,
    player_machine_attention: u32,
    distant_machine_attention: u32,
    machine_targets: HashMap<ChunkPos, Vec<MachineTarget>>,
}

impl LifeformSimulation {
    pub fn new(config: LifeformSimulationConfig) -> Self {
        Self {
            config,
            player_spawn_accumulator: Duration::ZERO,
            machinery_refresh_accumulator: Duration::ZERO,
            machinery_spawn_accumulator: Duration::ZERO,
            player_chunk_cursor: 0,
            machinery_chunk_cursor: 0,
            spawn_sequence: 0,
            machinery_chunks: Vec::new(),
            machinery_spawn_chunks: Vec::new(),
            machine_attention_by_chunk: HashMap::new(),
            distant_attention_by_chunk: HashMap::new(),
            player_machine_attention: 0,
            distant_machine_attention: 0,
            machine_targets: HashMap::new(),
        }
    }

    pub fn config(&self) -> LifeformSimulationConfig {
        self.config
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        system: &LifeformSystem,
        entities: &mut EntityWorld,
        target: Entity,
        terrain: &mut World,
        materials: impl Into<LifeformMaterials>,
        view: LifeformSpawnView,
        elapsed: Duration,
        physics: PhysicsConfig,
    ) -> LifeformSimulationUpdate {
        let materials = materials.into();
        let player_chunks = ChunkRect::around(
            terrain,
            view.player_position,
            self.config.player_chunk_radius,
        );
        let player_spawn_due = interval_elapsed(
            &mut self.player_spawn_accumulator,
            elapsed,
            self.config.player_spawn_interval,
        );
        let machinery_refresh_due = interval_elapsed(
            &mut self.machinery_refresh_accumulator,
            elapsed,
            self.config.machinery_refresh_interval,
        );
        let machinery_spawn_due = interval_elapsed(
            &mut self.machinery_spawn_accumulator,
            elapsed,
            self.config.machinery_spawn_interval,
        );
        if machinery_refresh_due.is_some() {
            self.refresh_machinery_chunks(terrain, player_chunks);
        }

        let mut update = LifeformSimulationUpdate {
            active_machinery_chunks: self.machinery_chunks.len(),
            player_machine_attention: self.player_machine_attention,
            distant_machine_attention: self.distant_machine_attention,
            ..LifeformSimulationUpdate::default()
        };
        self.update_active_lifeforms(
            system,
            entities,
            target,
            terrain,
            player_chunks,
            elapsed.as_secs_f32().min(physics.max_frame_time.max(0.0)),
            physics,
            &mut update,
        );

        if player_spawn_due.is_none() && machinery_spawn_due.is_none() {
            return update;
        }
        let (mut population, mut population_by_chunk, mut occupants) =
            collect_population(entities, terrain, target);
        if population >= self.config.maximum_lifeforms {
            update.population_capped = true;
            return update;
        }

        if player_spawn_due.is_some() {
            let player_attention = self
                .config
                .ambient_spawn_attention
                .saturating_add(self.player_machine_attention);
            let player_population_limit = attention_population_limit(
                player_attention,
                self.config.attention_per_lifeform,
                self.config.maximum_lifeforms,
            );
            let mut player_population = population_by_chunk
                .iter()
                .filter(|(chunk, _)| player_chunks.contains(**chunk))
                .map(|(_, count)| *count)
                .sum::<usize>();
            let checks = self
                .config
                .max_player_chunks_per_spawn
                .min(player_chunks.len());
            for offset in 0..checks {
                if player_population >= player_population_limit {
                    update.population_capped = true;
                    break;
                }
                let index = (self.player_chunk_cursor + offset) % player_chunks.len();
                let chunk = player_chunks.at(index);
                let chunk_attention = self.config.ambient_spawn_attention.saturating_add(
                    self.machine_attention_by_chunk
                        .get(&chunk)
                        .copied()
                        .unwrap_or(0),
                );
                let spawned = self.try_spawn_in_chunk(
                    system,
                    entities,
                    terrain,
                    materials,
                    view,
                    chunk,
                    chunk_attention,
                    player_population_limit - player_population,
                    &mut population,
                    &mut population_by_chunk,
                    &mut occupants,
                    &mut update,
                );
                player_population += spawned;
                update.player_chunks_checked += 1;
                if population >= self.config.maximum_lifeforms {
                    update.population_capped = true;
                    break;
                }
            }
            self.player_chunk_cursor =
                (self.player_chunk_cursor + checks) % player_chunks.len().max(1);
        }

        if machinery_spawn_due.is_some() && population < self.config.maximum_lifeforms {
            let machinery_population_limit = attention_population_limit(
                self.distant_machine_attention,
                self.config.attention_per_lifeform,
                self.config.maximum_lifeforms,
            );
            let mut machinery_population = population_by_chunk
                .iter()
                .filter(|(chunk, _)| self.distant_attention_by_chunk.contains_key(*chunk))
                .map(|(_, count)| *count)
                .sum::<usize>();
            let checks = self
                .config
                .max_machinery_chunks_per_spawn
                .min(self.machinery_spawn_chunks.len());
            for offset in 0..checks {
                if machinery_population >= machinery_population_limit {
                    update.population_capped = true;
                    break;
                }
                let index =
                    (self.machinery_chunk_cursor + offset) % self.machinery_spawn_chunks.len();
                let chunk = self.machinery_spawn_chunks[index];
                let attention = self
                    .distant_attention_by_chunk
                    .get(&chunk)
                    .copied()
                    .unwrap_or(0);
                let spawned = self.try_spawn_in_chunk(
                    system,
                    entities,
                    terrain,
                    materials,
                    view,
                    chunk,
                    attention,
                    machinery_population_limit - machinery_population,
                    &mut population,
                    &mut population_by_chunk,
                    &mut occupants,
                    &mut update,
                );
                machinery_population += spawned;
                update.machinery_chunks_checked += 1;
                if population >= self.config.maximum_lifeforms {
                    update.population_capped = true;
                    break;
                }
            }
            if !self.machinery_spawn_chunks.is_empty() {
                self.machinery_chunk_cursor =
                    (self.machinery_chunk_cursor + checks) % self.machinery_spawn_chunks.len();
            }
        }
        update
    }

    fn refresh_machinery_chunks(&mut self, terrain: &World, player_chunks: ChunkRect) {
        self.machinery_chunks.clear();
        self.machinery_spawn_chunks.clear();
        self.machine_attention_by_chunk.clear();
        self.distant_attention_by_chunk.clear();
        self.player_machine_attention = 0;
        self.distant_machine_attention = 0;
        self.machine_targets.clear();
        for definition in BUILT_IN_FURNITURE.iter().copied() {
            let radius = match definition.chunk_activity() {
                ChunkActivity::None => continue,
                ChunkActivity::Local => [0, 0],
                ChunkActivity::Nearby => self.config.machinery_chunk_radius,
            };
            for object in terrain.objects_of_type(definition.object_type()) {
                if definition.chunk_activity() == ChunkActivity::Nearby && !object.is_active() {
                    continue;
                }
                let anchor = object.anchor();
                let size = object.size();
                if definition.maximum_health().is_some()
                    && object.is_active()
                    && object.health() > 0
                {
                    let position = [
                        anchor.x as f32 + (f32::from(size[0]) - 1.0) * 0.5,
                        anchor.y as f32 + (f32::from(size[1]) - 1.0) * 0.5,
                    ];
                    self.machine_targets
                        .entry(position_chunk(terrain, position).unwrap_or_else(|| anchor.chunk()))
                        .or_default()
                        .push(MachineTarget {
                            object: object.id(),
                            position,
                            half_extents: [f32::from(size[0]) * 0.5, f32::from(size[1]) * 0.5],
                            noise_emission: definition.noise_emission(),
                        });
                }
                let last = crate::TilePos::new(
                    anchor.x + u32::from(size[0]) - 1,
                    anchor.y + u32::from(size[1]) - 1,
                );
                let first_chunk = anchor.chunk();
                let last_chunk = last.chunk();
                let minimum = ChunkPos {
                    x: first_chunk.x.saturating_sub(radius[0]),
                    y: first_chunk.y.saturating_sub(radius[1]),
                };
                let maximum = ChunkPos {
                    x: last_chunk
                        .x
                        .saturating_add(radius[0])
                        .min(terrain.chunks_wide() - 1),
                    y: last_chunk
                        .y
                        .saturating_add(radius[1])
                        .min(terrain.chunks_high() - 1),
                };
                let attracts_lifeforms = definition.lifeform_attention() > 0;
                let attracts_player_area =
                    attracts_lifeforms && chunk_rects_intersect(minimum, maximum, player_chunks);
                if attracts_player_area {
                    self.player_machine_attention = self
                        .player_machine_attention
                        .saturating_add(definition.lifeform_attention());
                } else if attracts_lifeforms {
                    self.distant_machine_attention = self
                        .distant_machine_attention
                        .saturating_add(definition.lifeform_attention());
                }
                for y in minimum.y..=maximum.y {
                    for x in minimum.x..=maximum.x {
                        let chunk = ChunkPos { x, y };
                        if !player_chunks.contains(chunk) {
                            self.machinery_chunks.push(chunk);
                        }
                        if !attracts_lifeforms {
                            continue;
                        }
                        let distance = chunk_distance_from_rect(chunk, first_chunk, last_chunk);
                        let divisor = distance.saturating_add(1);
                        let attention =
                            definition.lifeform_attention().saturating_add(divisor - 1) / divisor;
                        let chunk_attention =
                            self.machine_attention_by_chunk.entry(chunk).or_default();
                        *chunk_attention = chunk_attention.saturating_add(attention);
                        if !attracts_player_area && !player_chunks.contains(chunk) {
                            let chunk_attention =
                                self.distant_attention_by_chunk.entry(chunk).or_default();
                            *chunk_attention = chunk_attention.saturating_add(attention);
                            self.machinery_spawn_chunks.push(chunk);
                        }
                    }
                }
            }
        }
        self.machinery_chunks
            .sort_unstable_by_key(|chunk| (chunk.y, chunk.x));
        self.machinery_chunks.dedup();
        self.machinery_spawn_chunks
            .sort_unstable_by_key(|chunk| (chunk.y, chunk.x));
        self.machinery_spawn_chunks.dedup();
        if !self.machinery_spawn_chunks.is_empty() {
            self.machinery_chunk_cursor %= self.machinery_spawn_chunks.len();
        } else {
            self.machinery_chunk_cursor = 0;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn update_active_lifeforms(
        &self,
        system: &LifeformSystem,
        entities: &mut EntityWorld,
        target: Entity,
        terrain: &mut World,
        player_chunks: ChunkRect,
        player_step: f32,
        physics: PhysicsConfig,
        update: &mut LifeformSimulationUpdate,
    ) {
        let Some(target_position) = entities
            .get::<&Transform>(target)
            .ok()
            .map(|transform| transform.position)
        else {
            return;
        };
        let mut attacks = Vec::new();
        let mut block_attacks = Vec::new();
        let movement_grid = collect_movement_grid(entities);
        {
            let mut query = entities.query::<(&mut Lifeform, &mut Transform, &mut Collider)>();
            for (_, (lifeform, transform, collider)) in query.iter() {
                // Managed lifeforms are excluded from the global collider pass.
                collider.enabled = false;
                let Some(chunk) = position_chunk(terrain, transform.position) else {
                    continue;
                };
                let (step, near_player) = if player_chunks.contains(chunk) {
                    (player_step, true)
                } else if self
                    .machinery_chunks
                    .binary_search_by_key(&(chunk.y, chunk.x), |candidate| {
                        (candidate.y, candidate.x)
                    })
                    .is_ok()
                {
                    (player_step, false)
                } else {
                    continue;
                };
                if step <= 0.0 {
                    continue;
                }
                let machine_target =
                    self.nearest_machine_target(terrain, lifeform.id, transform.position);
                let separation = separation_steering(
                    transform.position,
                    system.is_flying(lifeform.id),
                    &movement_grid,
                );
                let destination = machine_target.map_or_else(
                    || {
                        if system.is_flying(lifeform.id) {
                            transform.position
                        } else {
                            target_position
                        }
                    },
                    |target| target.position,
                );
                advance_lifeform(
                    system,
                    lifeform,
                    transform,
                    collider,
                    destination,
                    separation,
                    machine_target.is_some(),
                    terrain,
                    step,
                    physics,
                );
                if let Some(target) = machine_target
                    && let Some(damage) = system.attack_if_ready(
                        lifeform,
                        transform.position,
                        collider.half_extents,
                        target.position,
                        target.half_extents,
                    )
                {
                    attacks.push((target.object, damage));
                }
                if let Some(target) = machine_target
                    && target.noise_emission > 0
                    && block_attacks.len() < self.config.max_block_attacks_per_update
                    && let Some(block) = blocking_block_towards(
                        terrain,
                        transform.position,
                        collider.half_extents,
                        target.position,
                    )
                    && let Some(damage) = system.block_attack_if_ready(lifeform)
                {
                    block_attacks.push((block, damage));
                }
                if near_player {
                    update.lifeforms_updated_near_player += 1;
                } else {
                    update.lifeforms_updated_near_machinery += 1;
                }
            }
        }
        for (machine, damage) in attacks {
            let result = terrain.damage_machine(machine, damage);
            if result.applied == 0 {
                continue;
            }
            update.machine_attacks += 1;
            update.machine_damage = update
                .machine_damage
                .saturating_add(u32::from(result.applied));
            update.machines_disabled += usize::from(result.disabled);
        }
        for (block, damage) in block_attacks {
            let Ok(result) = terrain.damage_block(block, Layer::Foreground, damage) else {
                continue;
            };
            if result.applied == 0 {
                continue;
            }
            update.block_attacks += 1;
            update.block_damage = update
                .block_damage
                .saturating_add(u32::from(result.applied));
            if let Some(broken) = result.broken {
                update.blocks_broken.push(broken);
            }
        }
    }

    fn nearest_machine_target(
        &self,
        terrain: &World,
        lifeform: LifeformId,
        position: [f32; 2],
    ) -> Option<MachineTarget> {
        let chunk = position_chunk(terrain, position)?;
        let minimum_x = chunk.x.saturating_sub(MACHINE_TARGET_SEARCH_RADIUS_CHUNKS);
        let maximum_x = chunk
            .x
            .saturating_add(MACHINE_TARGET_SEARCH_RADIUS_CHUNKS)
            .min(terrain.chunks_wide() - 1);
        let minimum_y = chunk.y.saturating_sub(MACHINE_TARGET_SEARCH_RADIUS_CHUNKS);
        let maximum_y = chunk
            .y
            .saturating_add(MACHINE_TARGET_SEARCH_RADIUS_CHUNKS)
            .min(terrain.chunks_high() - 1);
        let mut nearest = None;
        let mut nearest_distance = f32::INFINITY;
        for y in minimum_y..=maximum_y {
            for x in minimum_x..=maximum_x {
                let target_chunk = ChunkPos { x, y };
                let minimum_attention = if lifeform == LifeformId::GLOWGNAT {
                    self.config
                        .minimum_hostile_attention
                        .max(GLOWGNAT_MIN_MACHINERY_ATTENTION)
                } else {
                    self.config.minimum_hostile_attention
                };
                if self
                    .machine_attention_by_chunk
                    .get(&target_chunk)
                    .copied()
                    .unwrap_or(0)
                    < minimum_attention
                {
                    continue;
                }
                let Some(targets) = self.machine_targets.get(&target_chunk) else {
                    continue;
                };
                for &target in targets {
                    if terrain
                        .object(target.object)
                        .is_none_or(|object| !object.is_active() || object.health() == 0)
                    {
                        continue;
                    }
                    let delta_x = target.position[0] - position[0];
                    let delta_y = target.position[1] - position[1];
                    let distance = delta_x.mul_add(delta_x, delta_y * delta_y);
                    if distance < nearest_distance {
                        nearest = Some(target);
                        nearest_distance = distance;
                    }
                }
            }
        }
        nearest
    }

    #[allow(clippy::too_many_arguments)]
    fn try_spawn_in_chunk(
        &mut self,
        system: &LifeformSystem,
        entities: &mut EntityWorld,
        terrain: &World,
        materials: LifeformMaterials,
        view: LifeformSpawnView,
        chunk: ChunkPos,
        attention: u32,
        maximum_spawns: usize,
        population: &mut usize,
        population_by_chunk: &mut HashMap<ChunkPos, usize>,
        occupants: &mut Vec<Occupant>,
        update: &mut LifeformSimulationUpdate,
    ) -> usize {
        let local_limit = attention_population_limit(
            attention,
            self.config.attention_per_lifeform,
            self.config.maximum_lifeforms_per_chunk,
        );
        if maximum_spawns == 0
            || local_limit == 0
            || population_by_chunk.get(&chunk).copied().unwrap_or(0) >= local_limit
            || !self.spawn_pressure_succeeds(terrain.seed(), chunk, attention)
        {
            return 0;
        }
        let mut spawned = 0;
        for attempt in 0..self.config.spawn_attempts_per_chunk {
            if spawned >= maximum_spawns
                || *population >= self.config.maximum_lifeforms
                || population_by_chunk.get(&chunk).copied().unwrap_or(0) >= local_limit
            {
                break;
            }
            update.spawn_attempts += 1;
            let hash = spawn_hash(terrain.seed(), self.spawn_sequence, chunk, attempt as u64);
            self.spawn_sequence = self.spawn_sequence.wrapping_add(1);
            let biome = terrain
                .biome_in_chunk(chunk)
                .unwrap_or(crate::BiomeId::NORMAL);
            let Some(lifeform_id) = system.select_spawn(biome, attention, hash) else {
                continue;
            };
            let Some(definition) = system.definition(lifeform_id) else {
                continue;
            };
            let half_extents = [
                definition.collider_size[0] * 0.5,
                definition.collider_size[1] * 0.5,
            ];
            let position = if definition.locomotion.is_flying() {
                flying_spawn_position_in_chunk(terrain, chunk, half_extents, hash)
            } else {
                spawn_position_in_chunk(terrain, chunk, half_extents, hash)
            };
            let Some(position) = position else {
                continue;
            };
            if view.intersects_visible_area(
                position,
                half_extents,
                self.config.visibility_margin_tiles as f32,
            ) || occupants
                .iter()
                .any(|occupant| aabbs_overlap(position, half_extents, *occupant))
            {
                continue;
            }
            let Ok(entity) = system.spawn(entities, lifeform_id, materials, position) else {
                continue;
            };
            if let Ok(mut collider) = entities.get::<&mut Collider>(entity) {
                collider.enabled = false;
            }
            occupants.push(Occupant {
                position,
                half_extents,
            });
            *population += 1;
            *population_by_chunk.entry(chunk).or_default() += 1;
            update.spawned += 1;
            spawned += 1;
        }
        spawned
    }

    fn spawn_pressure_succeeds(&mut self, seed: u64, chunk: ChunkPos, attention: u32) -> bool {
        if attention == 0 {
            return false;
        }
        let threshold = self.config.attention_for_guaranteed_spawn.max(1);
        let hash = spawn_hash(seed, self.spawn_sequence, chunk, u64::MAX);
        self.spawn_sequence = self.spawn_sequence.wrapping_add(1);
        hash % u64::from(threshold) < u64::from(attention.min(threshold))
    }
}

/// Checks only the three cells immediately across the collider face aimed at
/// the target. Work is constant per attacking lifeform and never raycasts or
/// searches a chunk.
fn blocking_block_towards(
    terrain: &World,
    position: [f32; 2],
    half_extents: [f32; 2],
    target: [f32; 2],
) -> Option<TilePos> {
    let delta = [target[0] - position[0], target[1] - position[1]];
    if delta[0].abs() >= delta[1].abs() {
        let x = if delta[0] >= 0.0 {
            (position[0] + half_extents[0] + 0.5 + BLOCK_ATTACK_CONTACT_EPSILON).floor() as i64
        } else {
            (position[0] - half_extents[0] - 0.5 - BLOCK_ATTACK_CONTACT_EPSILON).ceil() as i64
        };
        let spread = half_extents[1] * 0.8;
        closest_solid_candidate(
            terrain,
            [
                [x, position[1].round() as i64],
                [x, (position[1] - spread).round() as i64],
                [x, (position[1] + spread).round() as i64],
            ],
            target,
        )
    } else {
        let y = if delta[1] >= 0.0 {
            (position[1] + half_extents[1] + 0.5 + BLOCK_ATTACK_CONTACT_EPSILON).floor() as i64
        } else {
            (position[1] - half_extents[1] - 0.5 - BLOCK_ATTACK_CONTACT_EPSILON).ceil() as i64
        };
        let spread = half_extents[0] * 0.8;
        closest_solid_candidate(
            terrain,
            [
                [position[0].round() as i64, y],
                [(position[0] - spread).round() as i64, y],
                [(position[0] + spread).round() as i64, y],
            ],
            target,
        )
    }
}

fn closest_solid_candidate(
    terrain: &World,
    candidates: [[i64; 2]; 3],
    target: [f32; 2],
) -> Option<TilePos> {
    candidates
        .into_iter()
        .filter_map(|[x, y]| {
            let x = u32::try_from(x).ok()?;
            let y = u32::try_from(y).ok()?;
            if x >= terrain.width()
                || y >= terrain.height()
                || terrain.tile(x, y, Layer::Foreground).ok()? == TileId::EMPTY
            {
                return None;
            }
            let distance = (x as f32 - target[0]).abs() + (y as f32 - target[1]).abs();
            Some((TilePos::new(x, y), distance))
        })
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(position, _)| position)
}

impl Default for LifeformSimulation {
    fn default() -> Self {
        Self::new(LifeformSimulationConfig::default())
    }
}

#[allow(clippy::too_many_arguments)]
fn advance_lifeform(
    system: &LifeformSystem,
    lifeform: &mut Lifeform,
    transform: &mut Transform,
    collider: &mut Collider,
    target_position: [f32; 2],
    separation: [f32; 2],
    engaged: bool,
    terrain: &World,
    elapsed: f32,
    physics: PhysicsConfig,
) {
    let maximum_step = physics.max_frame_time.clamp(0.001, 0.1);
    let mut remaining = elapsed.min(maximum_step * MAX_LOW_RATE_PHYSICS_STEPS as f32);
    while remaining > f32::EPSILON {
        let step = remaining.min(maximum_step);
        system.update_one(
            lifeform,
            transform,
            collider,
            target_position,
            separation,
            engaged,
            step,
        );
        update_collider(transform, collider, terrain, step, physics);
        remaining -= step;
    }
}

fn interval_elapsed(
    accumulator: &mut Duration,
    elapsed: Duration,
    interval: Duration,
) -> Option<Duration> {
    if interval.is_zero() {
        return Some(elapsed);
    }
    *accumulator = accumulator.saturating_add(elapsed);
    if *accumulator < interval {
        return None;
    }
    *accumulator = Duration::from_nanos(
        (accumulator.as_nanos() % interval.as_nanos()).min(u128::from(u64::MAX)) as u64,
    );
    Some(interval)
}

fn attention_population_limit(
    attention: u32,
    attention_per_lifeform: u32,
    hard_cap: usize,
) -> usize {
    if attention == 0 || hard_cap == 0 {
        return 0;
    }
    let unit = attention_per_lifeform.max(1);
    let limit = attention.saturating_add(unit - 1) / unit;
    usize::try_from(limit).unwrap_or(usize::MAX).min(hard_cap)
}

fn chunk_rects_intersect(minimum: ChunkPos, maximum: ChunkPos, other: ChunkRect) -> bool {
    minimum.x <= other.maximum.x
        && maximum.x >= other.minimum.x
        && minimum.y <= other.maximum.y
        && maximum.y >= other.minimum.y
}

fn chunk_distance_from_rect(chunk: ChunkPos, minimum: ChunkPos, maximum: ChunkPos) -> u32 {
    let x = minimum
        .x
        .saturating_sub(chunk.x)
        .max(chunk.x.saturating_sub(maximum.x));
    let y = minimum
        .y
        .saturating_sub(chunk.y)
        .max(chunk.y.saturating_sub(maximum.y));
    x.max(y)
}

fn movement_cell(position: [f32; 2]) -> (i32, i32) {
    (
        (position[0] / SEPARATION_CELL_SIZE).floor() as i32,
        (position[1] / SEPARATION_CELL_SIZE).floor() as i32,
    )
}

fn collect_movement_grid(entities: &EntityWorld) -> HashMap<(i32, i32), Vec<[f32; 2]>> {
    let mut grid = HashMap::new();
    for (_, (_, transform)) in entities.query::<(&Lifeform, &Transform)>().iter() {
        grid.entry(movement_cell(transform.position))
            .or_insert_with(Vec::new)
            .push(transform.position);
    }
    grid
}

fn separation_steering(
    position: [f32; 2],
    flying: bool,
    grid: &HashMap<(i32, i32), Vec<[f32; 2]>>,
) -> [f32; 2] {
    let cell = movement_cell(position);
    let mut steering = [0.0, 0.0];
    let y_range = if flying {
        cell.1 - 1..=cell.1 + 1
    } else {
        cell.1..=cell.1
    };
    for y in y_range {
        for x in cell.0 - 1..=cell.0 + 1 {
            let Some(neighbours) = grid.get(&(x, y)) else {
                continue;
            };
            for neighbour in neighbours {
                let delta = [position[0] - neighbour[0], position[1] - neighbour[1]];
                let distance = delta[0].hypot(delta[1]);
                if !(0.001..SEPARATION_RADIUS).contains(&distance) {
                    continue;
                }
                let strength = 1.0 - distance / SEPARATION_RADIUS;
                steering[0] += delta[0] / distance * strength;
                steering[1] += delta[1] / distance * strength;
            }
        }
    }
    let magnitude = steering[0].hypot(steering[1]);
    if magnitude > 1.0 {
        [steering[0] / magnitude, steering[1] / magnitude]
    } else {
        steering
    }
}

fn collect_population(
    entities: &EntityWorld,
    terrain: &World,
    target: Entity,
) -> (usize, HashMap<ChunkPos, usize>, Vec<Occupant>) {
    let mut population = 0;
    let mut by_chunk = HashMap::new();
    let mut occupants = Vec::new();
    for (_, (_, transform, collider)) in entities
        .query::<(&Lifeform, &Transform, &Collider)>()
        .iter()
    {
        population += 1;
        if let Some(chunk) = position_chunk(terrain, transform.position) {
            *by_chunk.entry(chunk).or_default() += 1;
        }
        occupants.push(Occupant {
            position: [
                transform.position[0] + collider.offset[0],
                transform.position[1] + collider.offset[1],
            ],
            half_extents: collider.half_extents,
        });
    }
    if let Ok(mut query) = entities.query_one::<(&Transform, &Collider)>(target)
        && let Some((transform, collider)) = query.get()
    {
        occupants.push(Occupant {
            position: [
                transform.position[0] + collider.offset[0],
                transform.position[1] + collider.offset[1],
            ],
            half_extents: collider.half_extents,
        });
    }
    (population, by_chunk, occupants)
}

fn spawn_position_in_chunk(
    terrain: &World,
    chunk: ChunkPos,
    half_extents: [f32; 2],
    hash: u64,
) -> Option<[f32; 2]> {
    let chunk_size = CHUNK_SIZE as u32;
    let minimum_x = chunk.x * chunk_size;
    let minimum_y = chunk.y * chunk_size;
    let width = chunk_size.min(terrain.width().saturating_sub(minimum_x));
    let height = chunk_size.min(terrain.height().saturating_sub(minimum_y));
    if width == 0 || height < 2 {
        return None;
    }
    let x = minimum_x + (hash % u64::from(width)) as u32;
    let first_y = (hash.rotate_left(23) % u64::from(height)) as u32;
    for offset in 0..height {
        let y = minimum_y + (first_y + offset) % height;
        if y == 0 || terrain.tile_in_bounds(x, y, Layer::Foreground) == TileId::EMPTY {
            continue;
        }
        let position = [
            x as f32,
            y as f32 - 0.5 - half_extents[1] - SPAWN_CLEARANCE_EPSILON,
        ];
        if terrain_space_is_clear(terrain, position, half_extents) {
            return Some(position);
        }
    }
    None
}

fn flying_spawn_position_in_chunk(
    terrain: &World,
    chunk: ChunkPos,
    half_extents: [f32; 2],
    hash: u64,
) -> Option<[f32; 2]> {
    let chunk_size = CHUNK_SIZE as u32;
    let minimum_x = chunk.x * chunk_size;
    let minimum_y = chunk.y * chunk_size;
    let width = chunk_size.min(terrain.width().saturating_sub(minimum_x));
    let height = chunk_size.min(terrain.height().saturating_sub(minimum_y));
    if width == 0 || height == 0 {
        return None;
    }
    for candidate in 0..FLYING_SPAWN_CANDIDATES {
        let value = spawn_hash(hash, candidate, chunk, candidate.rotate_left(17));
        let position = [
            (minimum_x + (value % u64::from(width)) as u32) as f32,
            (minimum_y + (value.rotate_left(29) % u64::from(height)) as u32) as f32,
        ];
        if terrain_space_is_clear(terrain, position, half_extents) {
            return Some(position);
        }
    }
    None
}

fn terrain_space_is_clear(terrain: &World, position: [f32; 2], half_extents: [f32; 2]) -> bool {
    let minimum = [position[0] - half_extents[0], position[1] - half_extents[1]];
    let maximum = [position[0] + half_extents[0], position[1] + half_extents[1]];
    if minimum[0] < -0.5
        || minimum[1] < -0.5
        || maximum[0] > terrain.width() as f32 - 0.5
        || maximum[1] > terrain.height() as f32 - 0.5
    {
        return false;
    }
    let minimum_tile = [
        (minimum[0] + 0.5).floor().max(0.0) as u32,
        (minimum[1] + 0.5).floor().max(0.0) as u32,
    ];
    let maximum_tile = [
        (maximum[0] - 0.5).ceil().max(0.0) as u32,
        (maximum[1] - 0.5).ceil().max(0.0) as u32,
    ];
    for y in minimum_tile[1]..=maximum_tile[1] {
        for x in minimum_tile[0]..=maximum_tile[0] {
            if x >= terrain.width()
                || y >= terrain.height()
                || terrain.is_collision_cell(crate::TilePos::new(x, y))
            {
                return false;
            }
        }
    }
    true
}

fn position_chunk(terrain: &World, position: [f32; 2]) -> Option<ChunkPos> {
    if !position.into_iter().all(f32::is_finite)
        || position[0] < 0.0
        || position[1] < 0.0
        || position[0] >= terrain.width() as f32
        || position[1] >= terrain.height() as f32
    {
        return None;
    }
    Some(ChunkPos {
        x: position[0] as u32 / CHUNK_SIZE as u32,
        y: position[1] as u32 / CHUNK_SIZE as u32,
    })
}

fn aabbs_overlap(position: [f32; 2], half_extents: [f32; 2], other: Occupant) -> bool {
    (position[0] - other.position[0]).abs() < half_extents[0] + other.half_extents[0]
        && (position[1] - other.position[1]).abs() < half_extents[1] + other.half_extents[1]
}

fn spawn_hash(seed: u64, sequence: u64, chunk: ChunkPos, attempt: u64) -> u64 {
    let mut value = seed
        ^ sequence.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ u64::from(chunk.x).wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ u64::from(chunk.y).wrapping_mul(0x94D0_49BB_1331_11EB)
        ^ attempt.wrapping_mul(0xD6E8_FEB8_6659_FD93);
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests;
