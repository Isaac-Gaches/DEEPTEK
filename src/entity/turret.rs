use super::{Collider, DynamicLight, Health, Lifeform, Particle, ParticleKind, Sprite, Transform};
use crate::{
    CHUNK_SIZE, ChunkPos, FurnitureObject, Layer, ObjectId, PowerSystem, TargetPriority, TileId,
    World as TerrainWorld,
};
use easy_gpu::assets::Material;
use easy_gpu::assets_manager::Handle;
use hecs::{Entity, World};
use std::collections::{HashMap, HashSet};

const PROJECTILE_COLLISION_STEP: f32 = 0.2;
const TARGET_CELL_SIZE: f32 = 16.0;
const MUZZLE_PARTICLE_COUNT: usize = 4;
const MUZZLE_PARTICLE_COLOUR: [f32; 4] = [0.0, 1.0, 1.0, 1.0];
const TURRET_PROJECTILE_LIGHT: [f32; 3] = [0.0, 0.72, 0.92];
const TURRET_MUZZLE_LIGHT: [f32; 3] = [0.0, 0.52, 0.68];

/// All balance values for the built-in turret live in one data object. Games
/// can construct `TurretSystem::new` with a modified value without changing
/// targeting, persistence, UI, or projectile integration code.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurretStats {
    pub range: f32,
    pub fire_interval: f32,
    pub projectile_speed: f32,
    pub projectile_gravity: f32,
    pub projectile_lifetime: f32,
    pub damage: u16,
}

impl Default for TurretStats {
    fn default() -> Self {
        Self {
            range: 30.0,
            fire_interval: 0.75,
            projectile_speed: 42.0,
            projectile_gravity: 18.0,
            projectile_lifetime: 4.0,
            damage: 12,
        }
    }
}

impl TurretStats {
    fn sanitized(self) -> Self {
        Self {
            range: positive_or(self.range, 30.0),
            fire_interval: positive_or(self.fire_interval, 0.75),
            projectile_speed: positive_or(self.projectile_speed, 42.0),
            projectile_gravity: non_negative_or(self.projectile_gravity, 18.0),
            projectile_lifetime: positive_or(self.projectile_lifetime, 4.0),
            damage: if self.damage == 0 { 12 } else { self.damage },
        }
    }
}

fn positive_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

fn non_negative_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        fallback
    }
}

/// A constant-acceleration round owned exclusively by `TurretSystem`.
/// Keeping it out of the rigid-body integrator guarantees exact SUVAT motion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurretProjectile {
    pub velocity: [f32; 2],
    pub acceleration: [f32; 2],
    pub damage: u16,
    source: Option<ObjectId>,
    remaining_lifetime: f32,
}

impl TurretProjectile {
    pub const fn remaining_lifetime(self) -> f32 {
        self.remaining_lifetime
    }
}

#[derive(Clone, Copy)]
struct TargetSnapshot {
    entity: Entity,
    position: [f32; 2],
    half_extents: [f32; 2],
    offset: [f32; 2],
    health: u16,
}

/// Retains all per-frame scratch data and per-object firing cooldowns.
pub struct TurretSystem {
    stats: TurretStats,
    cooldowns: HashMap<ObjectId, f64>,
    simulation_time: f64,
    seen_turrets: HashSet<ObjectId>,
    candidate_turrets: Vec<ObjectId>,
    targets: Vec<TargetSnapshot>,
    target_buckets: HashMap<[i32; 2], Vec<usize>>,
    maximum_target_radius: f32,
    projectiles_to_remove: Vec<Entity>,
    hits: Vec<(Entity, Entity, u16, Option<ObjectId>)>,
}

impl Default for TurretSystem {
    fn default() -> Self {
        Self::new(TurretStats::default())
    }
}

impl TurretSystem {
    pub fn new(stats: TurretStats) -> Self {
        Self {
            stats: stats.sanitized(),
            cooldowns: HashMap::new(),
            simulation_time: 0.0,
            seen_turrets: HashSet::new(),
            candidate_turrets: Vec::new(),
            targets: Vec::new(),
            target_buckets: HashMap::new(),
            maximum_target_radius: 0.0,
            projectiles_to_remove: Vec::new(),
            hits: Vec::new(),
        }
    }

    pub const fn stats(&self) -> TurretStats {
        self.stats
    }

    pub fn set_stats(&mut self, stats: TurretStats) {
        self.stats = stats.sanitized();
    }

    pub fn update(
        &mut self,
        entities: &mut World,
        terrain: &mut TerrainWorld,
        power: &PowerSystem,
        projectile_material: Handle<Material>,
        particle_material: Handle<Material>,
        elapsed: f32,
    ) {
        let elapsed = elapsed.clamp(0.0, 0.1);
        if elapsed <= 0.0 {
            return;
        }
        self.simulation_time += f64::from(elapsed);
        self.collect_targets(entities);
        self.update_projectiles(entities, terrain, elapsed);
        self.targets.retain_mut(|target| {
            let Some(health) = entities
                .get::<&Health>(target.entity)
                .ok()
                .map(|health| *health)
            else {
                return false;
            };
            target.health = health.current();
            target.health > 0
        });
        rebuild_target_buckets(&self.targets, &mut self.target_buckets);
        self.fire_turrets(
            entities,
            terrain,
            power,
            projectile_material,
            particle_material,
        );
    }

    fn collect_targets(&mut self, entities: &World) {
        self.targets.clear();
        self.target_buckets.clear();
        self.maximum_target_radius = 0.0;
        self.targets.extend(
            entities
                .query::<(&Lifeform, &Transform, &Collider, &Health)>()
                .iter()
                .filter(|(_, (_, transform, collider, health))| {
                    collider.enabled
                        && health.current() > 0
                        && transform.position.into_iter().all(f32::is_finite)
                        && collider.half_extents.into_iter().all(f32::is_finite)
                        && collider.offset.into_iter().all(f32::is_finite)
                })
                .map(
                    |(entity, (_, transform, collider, health))| TargetSnapshot {
                        entity,
                        position: transform.position,
                        half_extents: collider.half_extents,
                        offset: collider.offset,
                        health: health.current(),
                    },
                ),
        );
        self.maximum_target_radius = self
            .targets
            .iter()
            .flat_map(|target| {
                [
                    target.offset[0].abs() + target.half_extents[0],
                    target.offset[1].abs() + target.half_extents[1],
                ]
            })
            .fold(0.0_f32, f32::max);
        rebuild_target_buckets(&self.targets, &mut self.target_buckets);
    }

    fn update_projectiles(
        &mut self,
        entities: &mut World,
        terrain: &mut TerrainWorld,
        elapsed: f32,
    ) {
        self.projectiles_to_remove.clear();
        self.hits.clear();
        for (entity, (projectile, transform)) in entities
            .query::<(&mut TurretProjectile, &mut Transform)>()
            .iter()
        {
            let start = transform.position;
            // Exact constant-acceleration SUVAT integration for this time step.
            let end = [
                start[0]
                    + projectile.velocity[0] * elapsed
                    + 0.5 * projectile.acceleration[0] * elapsed * elapsed,
                start[1]
                    + projectile.velocity[1] * elapsed
                    + 0.5 * projectile.acceleration[1] * elapsed * elapsed,
            ];
            projectile.velocity[0] += projectile.acceleration[0] * elapsed;
            projectile.velocity[1] += projectile.acceleration[1] * elapsed;
            projectile.remaining_lifetime -= elapsed;
            transform.position = end;
            transform.rotation = projectile.velocity[1].atan2(projectile.velocity[0]);

            if projectile.remaining_lifetime <= 0.0 {
                self.projectiles_to_remove.push(entity);
                continue;
            }
            let terrain_hit = segment_terrain_hit_fraction(start, end, terrain);
            let target_hit = first_target_hit(
                start,
                end,
                &self.targets,
                &self.target_buckets,
                self.maximum_target_radius,
            );
            if let Some((target, target_fraction)) = target_hit
                && terrain_hit.is_none_or(|terrain_fraction| target_fraction <= terrain_fraction)
            {
                self.hits
                    .push((entity, target, projectile.damage, projectile.source));
                self.projectiles_to_remove.push(entity);
            } else if terrain_hit.is_some() {
                self.projectiles_to_remove.push(entity);
            }
        }

        for &(_, target, damage, source) in &self.hits {
            let killed = entities
                .get::<&mut Health>(target)
                .ok()
                .is_some_and(|mut health| {
                    health.damage(damage);
                    health.current() == 0
                });
            if killed {
                let _ = entities.despawn(target);
                if let Some(source) = source {
                    terrain.increment_turret_kill_count(source);
                }
            }
        }
        self.projectiles_to_remove.sort_unstable();
        self.projectiles_to_remove.dedup();
        for entity in self.projectiles_to_remove.drain(..) {
            let _ = entities.despawn(entity);
        }
    }

    fn fire_turrets(
        &mut self,
        entities: &mut World,
        terrain: &TerrainWorld,
        power: &PowerSystem,
        projectile_material: Handle<Material>,
        particle_material: Handle<Material>,
    ) {
        self.collect_candidate_turrets(terrain, power);
        for id in self.candidate_turrets.iter().copied() {
            let Some(object) = terrain.object(id) else {
                continue;
            };
            if self
                .cooldowns
                .get(&id)
                .is_some_and(|&ready_at| ready_at > self.simulation_time)
            {
                continue;
            }

            let origin = turret_muzzle(object.anchor(), object.size());
            let priority = terrain
                .furniture_target_priority(object.id())
                .unwrap_or_default();
            let Some(target) = select_visible_target(
                origin,
                self.stats.range,
                priority,
                &self.targets,
                &self.target_buckets,
                terrain,
            ) else {
                continue;
            };
            let Some(velocity) = ballistic_velocity(
                origin,
                target.position,
                self.stats.projectile_speed,
                self.stats.projectile_gravity,
                self.stats.projectile_lifetime,
            ) else {
                continue;
            };
            spawn_turret_projectile(
                entities,
                projectile_material,
                origin,
                velocity,
                self.stats,
                Some(id),
            );
            spawn_muzzle_particles(entities, particle_material, origin, velocity);
            self.cooldowns.insert(
                id,
                self.simulation_time + f64::from(self.stats.fire_interval),
            );
        }
        self.cooldowns.retain(|&id, _| {
            terrain.object(id).is_some_and(|object| {
                object.object_type() == FurnitureObject::TURRET && object.is_active()
            })
        });
    }

    fn collect_candidate_turrets(&mut self, terrain: &TerrainWorld, power: &PowerSystem) {
        self.seen_turrets.clear();
        self.candidate_turrets.clear();
        let chunk_size = CHUNK_SIZE as f32;
        let maximum_chunk = [
            terrain.width().saturating_sub(1) / CHUNK_SIZE as u32,
            terrain.height().saturating_sub(1) / CHUNK_SIZE as u32,
        ];
        for target in &self.targets {
            let minimum = [
                ((target.position[0] - self.stats.range).max(0.0) / chunk_size).floor() as u32,
                ((target.position[1] - self.stats.range).max(0.0) / chunk_size).floor() as u32,
            ];
            let maximum = [
                ((target.position[0] + self.stats.range).max(0.0) / chunk_size).floor() as u32,
                ((target.position[1] + self.stats.range).max(0.0) / chunk_size).floor() as u32,
            ];
            for chunk_y in minimum[1]..=maximum[1].min(maximum_chunk[1]) {
                for chunk_x in minimum[0]..=maximum[0].min(maximum_chunk[0]) {
                    for object in terrain.objects_in_chunk(ChunkPos {
                        x: chunk_x,
                        y: chunk_y,
                    }) {
                        if object.object_type() == FurnitureObject::TURRET
                            && object.is_active()
                            && power.is_powered(object.id())
                            && self.seen_turrets.insert(object.id())
                        {
                            self.candidate_turrets.push(object.id());
                        }
                    }
                }
            }
        }
        self.candidate_turrets.sort_unstable();
    }
}

fn rebuild_target_buckets(targets: &[TargetSnapshot], buckets: &mut HashMap<[i32; 2], Vec<usize>>) {
    buckets.clear();
    for (index, target) in targets.iter().enumerate() {
        buckets
            .entry(target_cell(target.position))
            .or_default()
            .push(index);
    }
}

fn target_cell(position: [f32; 2]) -> [i32; 2] {
    [
        (position[0] / TARGET_CELL_SIZE).floor() as i32,
        (position[1] / TARGET_CELL_SIZE).floor() as i32,
    ]
}

fn turret_muzzle(anchor: crate::TilePos, size: [u16; 2]) -> [f32; 2] {
    [
        anchor.x as f32 + (f32::from(size[0]) - 1.0) * 0.5,
        anchor.y as f32 + f32::from(size[1]) * 0.28,
    ]
}

fn spawn_turret_projectile(
    entities: &mut World,
    material: Handle<Material>,
    origin: [f32; 2],
    velocity: [f32; 2],
    stats: TurretStats,
    source: Option<ObjectId>,
) -> Entity {
    entities.spawn((
        TurretProjectile {
            velocity,
            acceleration: [0.0, stats.projectile_gravity],
            damage: stats.damage,
            source,
            remaining_lifetime: stats.projectile_lifetime,
        },
        DynamicLight::new(TURRET_PROJECTILE_LIGHT),
        Transform::new(origin)
            .with_scale([0.38, 0.22])
            .with_rotation(velocity[1].atan2(velocity[0])),
        Sprite::new(material)
            .with_frame(3)
            .with_tint([0.0, 1.0, 1.0, 1.0])
            .with_emissive(1.0)
            .with_depth(0.08),
    ))
}

fn spawn_muzzle_particles(
    entities: &mut World,
    material: Handle<Material>,
    origin: [f32; 2],
    velocity: [f32; 2],
) {
    let speed = velocity[0].hypot(velocity[1]).max(f32::EPSILON);
    let direction = [velocity[0] / speed, velocity[1] / speed];
    let perpendicular = [-direction[1], direction[0]];
    let muzzle = [
        origin[0] + direction[0] * 0.45,
        origin[1] + direction[1] * 0.45,
    ];
    for index in 0..MUZZLE_PARTICLE_COUNT {
        let spread = index as f32 - (MUZZLE_PARTICLE_COUNT - 1) as f32 * 0.5;
        let particle_velocity = [
            direction[0] * (3.0 + index as f32 * 0.55) + perpendicular[0] * spread * 1.2,
            direction[1] * (3.0 + index as f32 * 0.55) + perpendicular[1] * spread * 1.2,
        ];
        let start_scale = 0.34 + index as f32 * 0.035;
        entities.spawn((
            Particle::new(
                ParticleKind::LaserEnergy,
                0.16 + index as f32 * 0.025,
                particle_velocity,
                start_scale,
                0.05,
            ),
            DynamicLight::new(TURRET_MUZZLE_LIGHT),
            Transform::new(muzzle).with_scale([start_scale; 2]),
            Sprite::new(material)
                .with_frame(0)
                .with_tint(MUZZLE_PARTICLE_COLOUR)
                .with_emissive(1.0)
                .with_depth(0.07),
        ));
    }
}

fn select_visible_target<'a>(
    origin: [f32; 2],
    range: f32,
    priority: TargetPriority,
    targets: &'a [TargetSnapshot],
    buckets: &HashMap<[i32; 2], Vec<usize>>,
    terrain: &TerrainWorld,
) -> Option<&'a TargetSnapshot> {
    let range_squared = range * range;
    let mut best: Option<(&TargetSnapshot, f32)> = None;
    let minimum = target_cell([origin[0] - range, origin[1] - range]);
    let maximum = target_cell([origin[0] + range, origin[1] + range]);
    for cell_y in minimum[1]..=maximum[1] {
        for cell_x in minimum[0]..=maximum[0] {
            let Some(indices) = buckets.get(&[cell_x, cell_y]) else {
                continue;
            };
            for &index in indices {
                let target = &targets[index];
                let distance_squared = squared_distance(origin, target.position);
                if distance_squared > range_squared
                    || !has_line_of_sight(origin, target.position, terrain)
                {
                    continue;
                }
                let replace = best.is_none_or(|(current, current_distance)| {
                    target_is_better(
                        target,
                        distance_squared,
                        current,
                        current_distance,
                        priority,
                    )
                });
                if replace {
                    best = Some((target, distance_squared));
                }
            }
        }
    }
    best.map(|(target, _)| target)
}

fn target_is_better(
    candidate: &TargetSnapshot,
    candidate_distance: f32,
    current: &TargetSnapshot,
    current_distance: f32,
    priority: TargetPriority,
) -> bool {
    let primary = match priority {
        TargetPriority::Weakest => candidate.health.cmp(&current.health),
        TargetPriority::Strongest => current.health.cmp(&candidate.health),
        TargetPriority::Closest => candidate_distance.total_cmp(&current_distance),
        TargetPriority::Furthest => current_distance.total_cmp(&candidate_distance),
    };
    primary.is_lt()
        || (primary.is_eq()
            && (candidate_distance.total_cmp(&current_distance).is_lt()
                || (candidate_distance == current_distance
                    && candidate.entity.to_bits() < current.entity.to_bits())))
}

fn squared_distance(left: [f32; 2], right: [f32; 2]) -> f32 {
    let delta = [right[0] - left[0], right[1] - left[1]];
    delta[0] * delta[0] + delta[1] * delta[1]
}

/// Traverses only the tile cells crossed by the sight segment. Tile centres
/// are integral world coordinates, so their boundaries lie on half units.
fn has_line_of_sight(start: [f32; 2], end: [f32; 2], terrain: &TerrainWorld) -> bool {
    let mut cell = sight_cell(start);
    let end_cell = sight_cell(end);
    if cell == end_cell {
        return !tile_blocks_view(terrain, cell);
    }

    let delta = [end[0] - start[0], end[1] - start[1]];
    let step = [sight_step(delta[0]), sight_step(delta[1])];
    let mut maximum_t = [f32::INFINITY; 2];
    let mut delta_t = [f32::INFINITY; 2];
    for axis in 0..2 {
        if step[axis] == 0 {
            continue;
        }
        let boundary = cell[axis] as f32 + step[axis] as f32 * 0.5;
        maximum_t[axis] = (boundary - start[axis]) / delta[axis];
        delta_t[axis] = 1.0 / delta[axis].abs();
    }

    while cell != end_cell {
        match maximum_t[0].total_cmp(&maximum_t[1]) {
            std::cmp::Ordering::Less => {
                cell[0] += i64::from(step[0]);
                maximum_t[0] += delta_t[0];
            }
            std::cmp::Ordering::Greater => {
                cell[1] += i64::from(step[1]);
                maximum_t[1] += delta_t[1];
            }
            std::cmp::Ordering::Equal => {
                let horizontal = [cell[0] + i64::from(step[0]), cell[1]];
                let vertical = [cell[0], cell[1] + i64::from(step[1])];
                if tile_blocks_view(terrain, horizontal) || tile_blocks_view(terrain, vertical) {
                    return false;
                }
                cell[0] = horizontal[0];
                cell[1] = vertical[1];
                maximum_t[0] += delta_t[0];
                maximum_t[1] += delta_t[1];
            }
        }
        if tile_blocks_view(terrain, cell) {
            return false;
        }
    }
    true
}

fn sight_step(delta: f32) -> i32 {
    if delta > 0.0 {
        1
    } else if delta < 0.0 {
        -1
    } else {
        0
    }
}

fn sight_cell(position: [f32; 2]) -> [i64; 2] {
    [
        (position[0] + 0.5).floor() as i64,
        (position[1] + 0.5).floor() as i64,
    ]
}

fn tile_blocks_view(terrain: &TerrainWorld, cell: [i64; 2]) -> bool {
    let (Ok(x), Ok(y)) = (u32::try_from(cell[0]), u32::try_from(cell[1])) else {
        return true;
    };
    x >= terrain.width()
        || y >= terrain.height()
        || terrain.tile_in_bounds(x, y, Layer::Foreground) != TileId::EMPTY
}

/// Finds the earliest positive flight time whose required initial velocity
/// has the requested magnitude. The resulting velocity reaches the target
/// exactly under constant acceleration when a solution exists.
fn ballistic_velocity(
    origin: [f32; 2],
    target: [f32; 2],
    speed: f32,
    gravity: f32,
    maximum_time: f32,
) -> Option<[f32; 2]> {
    let delta = [target[0] - origin[0], target[1] - origin[1]];
    let distance = delta[0].hypot(delta[1]);
    if distance <= f32::EPSILON || speed <= 0.0 || maximum_time <= 0.0 {
        return None;
    }
    if gravity <= f32::EPSILON {
        return Some([delta[0] / distance * speed, delta[1] / distance * speed]);
    }

    let required_speed_squared = |time: f32| {
        let velocity = [
            delta[0] / time,
            (delta[1] - 0.5 * gravity * time * time) / time,
        ];
        velocity[0] * velocity[0] + velocity[1] * velocity[1] - speed * speed
    };
    let minimum_time = (distance / speed * 0.05).clamp(0.001, maximum_time);
    let mut previous_time = minimum_time;
    let mut previous = required_speed_squared(previous_time);
    const SAMPLES: u32 = 192;
    for sample in 1..=SAMPLES {
        let time = minimum_time + (maximum_time - minimum_time) * sample as f32 / SAMPLES as f32;
        let value = required_speed_squared(time);
        if previous > 0.0 && value <= 0.0 {
            let mut lower = previous_time;
            let mut upper = time;
            for _ in 0..32 {
                let middle = (lower + upper) * 0.5;
                if required_speed_squared(middle) > 0.0 {
                    lower = middle;
                } else {
                    upper = middle;
                }
            }
            let flight_time = (lower + upper) * 0.5;
            return Some([
                delta[0] / flight_time,
                (delta[1] - 0.5 * gravity * flight_time * flight_time) / flight_time,
            ]);
        }
        previous_time = time;
        previous = value;
    }
    None
}

fn segment_terrain_hit_fraction(
    start: [f32; 2],
    end: [f32; 2],
    terrain: &TerrainWorld,
) -> Option<f32> {
    let distance = (end[0] - start[0]).hypot(end[1] - start[1]);
    let steps = (distance / PROJECTILE_COLLISION_STEP).ceil().max(1.0) as u32;
    (1..=steps).find_map(|step| {
        let amount = step as f32 / steps as f32;
        let point = [
            start[0] + (end[0] - start[0]) * amount,
            start[1] + (end[1] - start[1]) * amount,
        ];
        point_hits_terrain(point, terrain).then_some(amount)
    })
}

fn point_hits_terrain(point: [f32; 2], terrain: &TerrainWorld) -> bool {
    if !point.into_iter().all(f32::is_finite) {
        return true;
    }
    let x = (point[0] + 0.5).floor() as i64;
    let y = (point[1] + 0.5).floor() as i64;
    let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
        return true;
    };
    x >= terrain.width()
        || y >= terrain.height()
        || terrain.tile_in_bounds(x, y, Layer::Foreground) != TileId::EMPTY
}

fn first_target_hit(
    start: [f32; 2],
    end: [f32; 2],
    targets: &[TargetSnapshot],
    buckets: &HashMap<[i32; 2], Vec<usize>>,
    padding: f32,
) -> Option<(Entity, f32)> {
    let minimum = target_cell([
        start[0].min(end[0]) - padding,
        start[1].min(end[1]) - padding,
    ]);
    let maximum = target_cell([
        start[0].max(end[0]) + padding,
        start[1].max(end[1]) + padding,
    ]);
    let mut first_hit: Option<(Entity, f32)> = None;
    for cell_y in minimum[1]..=maximum[1] {
        for cell_x in minimum[0]..=maximum[0] {
            let Some(indices) = buckets.get(&[cell_x, cell_y]) else {
                continue;
            };
            for &index in indices {
                let target = &targets[index];
                let centre = [
                    target.position[0] + target.offset[0],
                    target.position[1] + target.offset[1],
                ];
                let Some(fraction) = segment_aabb_fraction(start, end, centre, target.half_extents)
                else {
                    continue;
                };
                let replace = first_hit.is_none_or(|(entity, current)| {
                    fraction.total_cmp(&current).is_lt()
                        || (fraction == current && target.entity.to_bits() < entity.to_bits())
                });
                if replace {
                    first_hit = Some((target.entity, fraction));
                }
            }
        }
    }
    first_hit
}

fn segment_aabb_fraction(
    start: [f32; 2],
    end: [f32; 2],
    centre: [f32; 2],
    half_extents: [f32; 2],
) -> Option<f32> {
    let direction = [end[0] - start[0], end[1] - start[1]];
    let minimum = [centre[0] - half_extents[0], centre[1] - half_extents[1]];
    let maximum = [centre[0] + half_extents[0], centre[1] + half_extents[1]];
    let mut near = 0.0_f32;
    let mut far = 1.0_f32;
    for axis in 0..2 {
        if direction[axis].abs() <= f32::EPSILON {
            if start[axis] < minimum[axis] || start[axis] > maximum[axis] {
                return None;
            }
            continue;
        }
        let inverse = 1.0 / direction[axis];
        let mut first = (minimum[axis] - start[axis]) * inverse;
        let mut second = (maximum[axis] - start[axis]) * inverse;
        if first > second {
            std::mem::swap(&mut first, &mut second);
        }
        near = near.max(first);
        far = far.min(second);
        if near > far {
            return None;
        }
    }
    (0.0..=1.0).contains(&near).then_some(near)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ForegroundTile, LifeformId, LifeformSystem, TilePos};
    use std::marker::PhantomData;

    fn material() -> Handle<Material> {
        Handle {
            index: 0,
            generation: 0,
            _marker: PhantomData,
        }
    }

    fn snapshot(entity: Entity, x: f32, health: u16) -> TargetSnapshot {
        TargetSnapshot {
            entity,
            position: [x, 0.0],
            half_extents: [0.5; 2],
            offset: [0.0; 2],
            health,
        }
    }

    fn place_power_network(terrain: &mut TerrainWorld) -> PowerSystem {
        for x in [7, 9, 10] {
            terrain
                .set_tile(x, 12, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        terrain
            .place_furniture(FurnitureObject::PYLON, TilePos::new(7, 10))
            .unwrap();
        terrain
            .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(9, 9))
            .unwrap();
        let mut power = PowerSystem::new();
        power.distribute(terrain, 0.5, std::time::Duration::from_secs(1));
        power
    }

    #[test]
    fn target_modes_choose_the_requested_lifeform() {
        let mut entities = World::new();
        let weak_near = entities.spawn(());
        let strong_middle = entities.spawn(());
        let weak_far = entities.spawn(());
        let targets = [
            snapshot(weak_near, 3.0, 10),
            snapshot(strong_middle, 6.0, 80),
            snapshot(weak_far, 9.0, 10),
        ];
        let origin = [0.0, 0.0];
        let terrain = TerrainWorld::empty(20, 12, 0).unwrap();
        let mut buckets = HashMap::new();
        rebuild_target_buckets(&targets, &mut buckets);

        assert_eq!(
            select_visible_target(
                origin,
                10.0,
                TargetPriority::Weakest,
                &targets,
                &buckets,
                &terrain,
            )
            .unwrap()
            .entity,
            weak_near
        );
        assert_eq!(
            select_visible_target(
                origin,
                10.0,
                TargetPriority::Strongest,
                &targets,
                &buckets,
                &terrain,
            )
            .unwrap()
            .entity,
            strong_middle
        );
        assert_eq!(
            select_visible_target(
                origin,
                10.0,
                TargetPriority::Closest,
                &targets,
                &buckets,
                &terrain,
            )
            .unwrap()
            .entity,
            weak_near
        );
        assert_eq!(
            select_visible_target(
                origin,
                10.0,
                TargetPriority::Furthest,
                &targets,
                &buckets,
                &terrain,
            )
            .unwrap()
            .entity,
            weak_far
        );
        assert!(
            select_visible_target(
                origin,
                2.0,
                TargetPriority::Closest,
                &targets,
                &buckets,
                &terrain,
            )
            .is_none()
        );
    }

    #[test]
    fn ballistic_solution_reaches_target_under_suvat() {
        let origin = [2.0, 8.0];
        let target = [20.0, 5.0];
        let speed = 30.0;
        let gravity = 18.0;
        let velocity = ballistic_velocity(origin, target, speed, gravity, 4.0).unwrap();
        assert!((velocity[0].hypot(velocity[1]) - speed).abs() < 0.001);

        let flight_time = (target[0] - origin[0]) / velocity[0];
        let reached = [
            origin[0] + velocity[0] * flight_time,
            origin[1] + velocity[1] * flight_time + 0.5 * gravity * flight_time * flight_time,
        ];
        assert!((reached[0] - target[0]).abs() < 0.001);
        assert!((reached[1] - target[1]).abs() < 0.001);
    }

    #[test]
    fn segment_collision_catches_fast_rounds() {
        assert_eq!(
            segment_aabb_fraction([0.0, 0.0], [20.0, 0.0], [10.0, 0.0], [0.5, 1.0]),
            Some(0.475)
        );
    }

    #[test]
    fn invalid_stats_are_sanitized_at_the_system_boundary() {
        let system = TurretSystem::new(TurretStats {
            range: f32::NAN,
            fire_interval: 0.0,
            projectile_speed: -1.0,
            projectile_gravity: -1.0,
            projectile_lifetime: f32::INFINITY,
            damage: 0,
        });
        assert_eq!(system.stats(), TurretStats::default());
    }

    #[test]
    fn turret_must_be_active_and_powered_to_fire() {
        let mut terrain = TerrainWorld::empty(40, 24, 0).unwrap();
        for x in 4..=5 {
            terrain
                .set_tile(x, 12, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        let turret = terrain
            .place_furniture(FurnitureObject::TURRET, TilePos::new(4, 10))
            .unwrap();
        let mut entities = World::new();
        LifeformSystem::with_built_ins()
            .spawn(&mut entities, LifeformId::WALKER, material(), [18.0, 10.5])
            .unwrap();
        let mut system = TurretSystem::default();

        let mut power = PowerSystem::new();
        power.update(&terrain);
        system.update(
            &mut entities,
            &mut terrain,
            &power,
            material(),
            material(),
            1.0 / 60.0,
        );
        assert_eq!(entities.query::<&TurretProjectile>().iter().count(), 0);

        assert!(terrain.set_furniture_active(turret, true));
        power.update(&terrain);
        system.update(
            &mut entities,
            &mut terrain,
            &power,
            material(),
            material(),
            1.0 / 60.0,
        );
        assert_eq!(entities.query::<&TurretProjectile>().iter().count(), 0);

        power = place_power_network(&mut terrain);
        system.update(
            &mut entities,
            &mut terrain,
            &power,
            material(),
            material(),
            1.0 / 60.0,
        );
        assert_eq!(entities.query::<&TurretProjectile>().iter().count(), 1);
        assert_eq!(
            entities.query::<&Particle>().iter().count(),
            MUZZLE_PARTICLE_COUNT
        );
        assert_eq!(
            entities.query::<&DynamicLight>().iter().count(),
            MUZZLE_PARTICLE_COUNT + 1
        );
    }

    #[test]
    fn swept_round_damages_a_lifeform_before_the_wall_behind_it() {
        let mut terrain = TerrainWorld::empty(20, 12, 0).unwrap();
        terrain
            .set_tile(8, 5, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        let mut entities = World::new();
        let target = LifeformSystem::with_built_ins()
            .spawn(&mut entities, LifeformId::WALKER, material(), [5.0, 5.0])
            .unwrap();
        spawn_turret_projectile(
            &mut entities,
            material(),
            [0.0, 5.0],
            [100.0, 0.0],
            TurretStats {
                projectile_gravity: 0.0,
                ..TurretStats::default()
            },
            None,
        );

        TurretSystem::default().update(
            &mut entities,
            &mut terrain,
            &PowerSystem::new(),
            material(),
            material(),
            0.1,
        );

        assert_eq!(entities.get::<&Health>(target).unwrap().current(), 28);
        assert_eq!(entities.query::<&TurretProjectile>().iter().count(), 0);
    }

    #[test]
    fn lethal_projectile_is_credited_to_its_source_turret() {
        let mut terrain = TerrainWorld::empty(20, 12, 0).unwrap();
        for x in 1..=2 {
            terrain
                .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        let turret = terrain
            .place_furniture(FurnitureObject::TURRET, TilePos::new(1, 6))
            .unwrap();
        let mut entities = World::new();
        let target = LifeformSystem::with_built_ins()
            .spawn(&mut entities, LifeformId::WALKER, material(), [5.0, 5.0])
            .unwrap();
        entities
            .get::<&mut Health>(target)
            .unwrap()
            .damage(u16::MAX);
        // Restore a live one-hit target without relying on a lifeform's balance data.
        *entities.get::<&mut Health>(target).unwrap() = Health::new(1);
        spawn_turret_projectile(
            &mut entities,
            material(),
            [0.0, 5.0],
            [100.0, 0.0],
            TurretStats {
                projectile_gravity: 0.0,
                ..TurretStats::default()
            },
            Some(turret),
        );

        TurretSystem::default().update(
            &mut entities,
            &mut terrain,
            &PowerSystem::new(),
            material(),
            material(),
            0.1,
        );

        assert!(entities.get::<&Health>(target).is_err());
        assert_eq!(terrain.turret_kill_count(turret), Some(1));
    }

    #[test]
    fn tiles_block_sight_and_hidden_priority_targets_are_skipped() {
        let mut terrain = TerrainWorld::empty(20, 12, 0).unwrap();
        terrain
            .set_tile(4, 2, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        assert!(!has_line_of_sight([0.0, 2.0], [8.0, 2.0], &terrain));
        assert!(has_line_of_sight([0.0, 2.0], [8.0, 5.0], &terrain));

        let mut entities = World::new();
        let hidden_weak = entities.spawn(());
        let visible_strong = entities.spawn(());
        let targets = [
            TargetSnapshot {
                position: [8.0, 2.0],
                ..snapshot(hidden_weak, 8.0, 5)
            },
            TargetSnapshot {
                position: [8.0, 5.0],
                ..snapshot(visible_strong, 8.0, 40)
            },
        ];
        let mut buckets = HashMap::new();
        rebuild_target_buckets(&targets, &mut buckets);

        assert_eq!(
            select_visible_target(
                [0.0, 2.0],
                12.0,
                TargetPriority::Weakest,
                &targets,
                &buckets,
                &terrain,
            )
            .unwrap()
            .entity,
            visible_strong
        );
    }

    #[test]
    fn only_turret_chunks_near_lifeforms_enter_targeting_work() {
        let mut terrain = TerrainWorld::empty(1_000, 24, 0).unwrap();
        for x in [4, 5, 900, 901] {
            terrain
                .set_tile(x, 12, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        let near = terrain
            .place_furniture(FurnitureObject::TURRET, TilePos::new(4, 10))
            .unwrap();
        let far = terrain
            .place_furniture(FurnitureObject::TURRET, TilePos::new(900, 10))
            .unwrap();
        assert!(terrain.set_furniture_active(near, true));
        assert!(terrain.set_furniture_active(far, true));
        let mut entities = World::new();
        LifeformSystem::with_built_ins()
            .spawn(&mut entities, LifeformId::WALKER, material(), [18.0, 10.5])
            .unwrap();
        let mut system = TurretSystem::default();
        let power = place_power_network(&mut terrain);

        system.update(
            &mut entities,
            &mut terrain,
            &power,
            material(),
            material(),
            1.0 / 60.0,
        );

        assert_eq!(system.candidate_turrets, vec![near]);
        assert_eq!(entities.query::<&TurretProjectile>().iter().count(), 1);
    }
}
