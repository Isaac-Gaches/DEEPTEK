mod housing;

pub use housing::{
    HouseRequirements, MAX_HOUSE_INTERIOR_CELLS, MIN_HOUSE_INTERIOR_CELLS, RoomAssessment,
    assess_bed, assess_room,
};

use crate::{
    BiomeId, CHUNK_SIZE, ChunkPos, Collider, ContractCompany, FurnitureObject, ObjectId, Sprite,
    TilePos, Transform, World, furniture_definition,
};
use easy_gpu::assets::Material;
use easy_gpu::assets_manager::Handle;
use hecs::{Entity, World as EntityWorld};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

const RESIDENT_DISTANCE_TILES: f32 = 192.0;
const HAPPINESS_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct SpecialistId(u16);

impl SpecialistId {
    pub const ENGINEER: Self = Self(1);
    pub const GEOLOGIST: Self = Self(2);
    pub const QUARTERMASTER: Self = Self(3);
    pub const DRILL_ENGINEER: Self = Self::ENGINEER;
    pub const LOGISTICS_ENGINEER: Self = Self::GEOLOGIST;
    pub const SECURITY_OFFICER: Self = Self::QUARTERMASTER;

    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HappinessRule {
    NearbyNoise {
        radius_tiles: u16,
        penalty_per_noise: u8,
    },
    BiomePreference {
        preferred: &'static [BiomeId],
        mismatch_penalty: u8,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpecialistBonus {
    DrillSpeedPercent(u16),
    DrillDepthTiles(u32),
    TurretDamagePercent(u16),
    TurretFireRatePercent(u16),
    AdvancedTurretTargeting,
    ConveyorSpeedPercent(u16),
    LiftSpeedPercent(u16),
    ProcessingSpeedPercent(u16),
}

impl SpecialistBonus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::DrillSpeedPercent(_) => "DRILL SPEED",
            Self::DrillDepthTiles(_) => "DRILL DEPTH",
            Self::TurretDamagePercent(_) => "TURRET DAMAGE",
            Self::TurretFireRatePercent(_) => "TURRET FIRE RATE",
            Self::AdvancedTurretTargeting => "TARGETING MODES",
            Self::ConveyorSpeedPercent(_) => "CONVEYOR SPEED",
            Self::LiftSpeedPercent(_) => "CARGO LIFT SPEED",
            Self::ProcessingSpeedPercent(_) => "PROCESSING THROUGHPUT",
        }
    }

    pub fn description(self) -> String {
        match self {
            Self::DrillSpeedPercent(percent) => format!("+{percent}% damage per drill cycle"),
            Self::DrillDepthTiles(tiles) => format!("+{tiles} tiles maximum drilling range"),
            Self::TurretDamagePercent(percent) => format!("+{percent}% projectile damage"),
            Self::TurretFireRatePercent(percent) => format!("+{percent}% firing speed"),
            Self::AdvancedTurretTargeting => {
                "Unlocks weakest, strongest, and furthest priorities".to_owned()
            }
            Self::ConveyorSpeedPercent(percent) => format!("+{percent}% item transfer speed"),
            Self::LiftSpeedPercent(percent) => format!("+{percent}% powered lift speed"),
            Self::ProcessingSpeedPercent(percent) => format!("+{percent}% machine throughput"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpecialistBonuses {
    drill_speed_percent: u16,
    drill_depth_tiles: u32,
    turret_damage_percent: u16,
    turret_fire_rate_percent: u16,
    advanced_turret_targeting: bool,
    conveyor_speed_percent: u16,
    lift_speed_percent: u16,
    processing_speed_percent: u16,
}

impl Default for SpecialistBonuses {
    fn default() -> Self {
        Self {
            drill_speed_percent: 100,
            drill_depth_tiles: 0,
            turret_damage_percent: 100,
            turret_fire_rate_percent: 100,
            advanced_turret_targeting: false,
            conveyor_speed_percent: 100,
            lift_speed_percent: 100,
            processing_speed_percent: 100,
        }
    }
}

impl SpecialistBonuses {
    pub fn from_ids(ids: impl IntoIterator<Item = SpecialistId>) -> Self {
        let mut bonuses = Self::default();
        for id in ids {
            let Some(definition) = specialist_definition(id) else {
                continue;
            };
            for &bonus in definition.bonuses {
                bonuses.apply(bonus);
            }
        }
        bonuses
    }

    pub const fn drill_speed_percent(self) -> u16 {
        self.drill_speed_percent
    }

    pub const fn drill_depth_tiles(self) -> u32 {
        self.drill_depth_tiles
    }

    pub const fn turret_damage_percent(self) -> u16 {
        self.turret_damage_percent
    }

    pub const fn turret_fire_rate_percent(self) -> u16 {
        self.turret_fire_rate_percent
    }

    pub const fn advanced_turret_targeting(self) -> bool {
        self.advanced_turret_targeting
    }

    pub const fn conveyor_speed_percent(self) -> u16 {
        self.conveyor_speed_percent
    }

    pub const fn lift_speed_percent(self) -> u16 {
        self.lift_speed_percent
    }

    pub const fn processing_speed_percent(self) -> u16 {
        self.processing_speed_percent
    }

    fn apply(&mut self, bonus: SpecialistBonus) {
        match bonus {
            SpecialistBonus::DrillSpeedPercent(percent) => {
                self.drill_speed_percent = self.drill_speed_percent.saturating_add(percent);
            }
            SpecialistBonus::DrillDepthTiles(tiles) => {
                self.drill_depth_tiles = self.drill_depth_tiles.saturating_add(tiles);
            }
            SpecialistBonus::TurretDamagePercent(percent) => {
                self.turret_damage_percent = self.turret_damage_percent.saturating_add(percent);
            }
            SpecialistBonus::TurretFireRatePercent(percent) => {
                self.turret_fire_rate_percent =
                    self.turret_fire_rate_percent.saturating_add(percent);
            }
            SpecialistBonus::AdvancedTurretTargeting => {
                self.advanced_turret_targeting = true;
            }
            SpecialistBonus::ConveyorSpeedPercent(percent) => {
                self.conveyor_speed_percent = self.conveyor_speed_percent.saturating_add(percent);
            }
            SpecialistBonus::LiftSpeedPercent(percent) => {
                self.lift_speed_percent = self.lift_speed_percent.saturating_add(percent);
            }
            SpecialistBonus::ProcessingSpeedPercent(percent) => {
                self.processing_speed_percent =
                    self.processing_speed_percent.saturating_add(percent);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpecialistDefinition {
    pub id: SpecialistId,
    pub name: &'static str,
    pub role: &'static str,
    pub company: ContractCompany,
    pub description: &'static str,
    pub greeting: &'static str,
    pub preferred_biomes: &'static [BiomeId],
    pub bonuses: &'static [SpecialistBonus],
    pub tint: [f32; 4],
    pub move_speed: f32,
    pub happiness_rules: &'static [HappinessRule],
}

const NORMAL_BIOMES: &[BiomeId] = &[BiomeId::NORMAL];
const CRYSTAL_BIOMES: &[BiomeId] = &[BiomeId::GLOWING_CRYSTAL];
const ENGINEER_RULES: &[HappinessRule] = &[
    HappinessRule::NearbyNoise {
        radius_tiles: 48,
        penalty_per_noise: 1,
    },
    HappinessRule::BiomePreference {
        preferred: CRYSTAL_BIOMES,
        mismatch_penalty: 20,
    },
];
const GEOLOGIST_RULES: &[HappinessRule] = &[
    HappinessRule::NearbyNoise {
        radius_tiles: 48,
        penalty_per_noise: 2,
    },
    HappinessRule::BiomePreference {
        preferred: NORMAL_BIOMES,
        mismatch_penalty: 20,
    },
];
const QUARTERMASTER_RULES: &[HappinessRule] = &[
    HappinessRule::NearbyNoise {
        radius_tiles: 40,
        penalty_per_noise: 2,
    },
    HappinessRule::BiomePreference {
        preferred: NORMAL_BIOMES,
        mismatch_penalty: 20,
    },
];

const DRILL_ENGINEER_BONUSES: &[SpecialistBonus] = &[
    SpecialistBonus::DrillSpeedPercent(50),
    SpecialistBonus::DrillDepthTiles(80),
];
const LOGISTICS_ENGINEER_BONUSES: &[SpecialistBonus] = &[
    SpecialistBonus::ConveyorSpeedPercent(50),
    SpecialistBonus::LiftSpeedPercent(25),
    SpecialistBonus::ProcessingSpeedPercent(25),
];
const SECURITY_OFFICER_BONUSES: &[SpecialistBonus] = &[
    SpecialistBonus::AdvancedTurretTargeting,
    SpecialistBonus::TurretDamagePercent(25),
    SpecialistBonus::TurretFireRatePercent(25),
];

pub const BUILT_IN_SPECIALISTS: &[SpecialistDefinition] = &[
    SpecialistDefinition {
        id: SpecialistId::ENGINEER,
        name: "Mara Venn",
        role: "Drill Engineer",
        company: ContractCompany::DeepTekIndustries,
        description: "A drilling specialist who improves excavation speed and the reach of every bore in the world.",
        greeting: "Every stratum has a weak point. Give the drills enough power and I will help them find it faster.",
        preferred_biomes: CRYSTAL_BIOMES,
        bonuses: DRILL_ENGINEER_BONUSES,
        tint: [0.25, 0.90, 1.0, 1.0],
        move_speed: 2.2,
        happiness_rules: ENGINEER_RULES,
    },
    SpecialistDefinition {
        id: SpecialistId::GEOLOGIST,
        name: "Dr. Ivo Senn",
        role: "Logistics Engineer",
        company: ContractCompany::AstraSurveyCorp,
        description: "An industrial logistics engineer who accelerates conveyors, cargo lifts, and processing machinery.",
        greeting: "Throughput is a question of flow. Let me tune the routes and we will keep every machine supplied.",
        preferred_biomes: NORMAL_BIOMES,
        bonuses: LOGISTICS_ENGINEER_BONUSES,
        tint: [0.72, 0.56, 1.0, 1.0],
        move_speed: 1.9,
        happiness_rules: GEOLOGIST_RULES,
    },
    SpecialistDefinition {
        id: SpecialistId::QUARTERMASTER,
        name: "Rook Hale",
        role: "Security Officer",
        company: ContractCompany::VanguardDefence,
        description: "A defence specialist who unlocks advanced turret priorities and improves every weapon platform.",
        greeting: "Closest is not always the greatest threat. I can sharpen the targeting logic and the response.",
        preferred_biomes: NORMAL_BIOMES,
        bonuses: SECURITY_OFFICER_BONUSES,
        tint: [1.0, 0.38, 0.25, 1.0],
        move_speed: 2.0,
        happiness_rules: QUARTERMASTER_RULES,
    },
];

pub fn specialist_definition(id: SpecialistId) -> Option<&'static SpecialistDefinition> {
    BUILT_IN_SPECIALISTS
        .iter()
        .find(|definition| definition.id == id)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecialistRecord {
    id: SpecialistId,
    home_terminal: ObjectId,
    position_bits: [u32; 2],
    happiness: u8,
}

impl SpecialistRecord {
    pub(crate) fn new(id: SpecialistId, home_terminal: ObjectId, position: [f32; 2]) -> Self {
        Self {
            id,
            home_terminal,
            position_bits: position.map(f32::to_bits),
            happiness: 100,
        }
    }

    pub(crate) fn from_saved(
        id: SpecialistId,
        home_terminal: ObjectId,
        position: [f32; 2],
        happiness: u8,
    ) -> Self {
        Self {
            id,
            home_terminal,
            position_bits: position.map(f32::to_bits),
            happiness,
        }
    }

    pub const fn id(&self) -> SpecialistId {
        self.id
    }

    pub const fn home_terminal(&self) -> ObjectId {
        self.home_terminal
    }

    pub fn position(&self) -> [f32; 2] {
        self.position_bits.map(f32::from_bits)
    }

    pub const fn happiness(&self) -> u8 {
        self.happiness
    }

    fn set_position(&mut self, position: [f32; 2]) {
        self.position_bits = position.map(f32::to_bits);
    }

    fn set_happiness(&mut self, happiness: u8) {
        self.happiness = happiness.min(100);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HappinessFactor {
    pub label: &'static str,
    pub delta: i16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HappinessReport {
    pub score: u8,
    pub factors: Vec<HappinessFactor>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecruitSpecialistError {
    UnknownSpecialist,
    AlreadyRecruited,
    InvalidTerminal,
    UnsuitableHouse,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpecialistOfferState {
    pub definition: &'static SpecialistDefinition,
    pub recruited: bool,
    pub happiness: Option<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpecialistTerminalView {
    pub requirements: HouseRequirements,
    pub interior_cells: usize,
    pub specialists: Vec<SpecialistOfferState>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Specialist {
    pub id: SpecialistId,
    pub home_terminal: ObjectId,
    target_index: usize,
    idle_seconds: f32,
    random_state: u32,
}

impl Specialist {
    fn new(id: SpecialistId, home_terminal: ObjectId) -> Self {
        Self {
            id,
            home_terminal,
            target_index: 0,
            idle_seconds: 0.5,
            random_state: u32::from(id.raw())
                .wrapping_mul(0x9E37_79B9)
                .wrapping_add(home_terminal.raw() as u32),
        }
    }

    fn next_index(&mut self, count: usize) -> usize {
        self.random_state ^= self.random_state << 13;
        self.random_state ^= self.random_state >> 17;
        self.random_state ^= self.random_state << 5;
        self.random_state as usize % count.max(1)
    }
}

#[derive(Clone, Debug)]
struct CachedRoom {
    revision: [u64; 2],
    assessment: Arc<RoomAssessment>,
}

#[derive(Debug, Default)]
pub struct SpecialistSystem {
    residents: HashMap<SpecialistId, Entity>,
    rooms: HashMap<ObjectId, CachedRoom>,
    happiness_elapsed: Duration,
}

impl SpecialistSystem {
    pub fn reset(&mut self) {
        self.residents.clear();
        self.rooms.clear();
        self.happiness_elapsed = Duration::ZERO;
    }

    pub fn resident_count(&self) -> usize {
        self.residents.len()
    }

    pub fn terminal_view(
        &mut self,
        world: &World,
        terminal: Option<ObjectId>,
    ) -> SpecialistTerminalView {
        let room = terminal.and_then(|terminal| self.room(world, terminal));
        let requirements = room
            .as_ref()
            .map_or_else(HouseRequirements::default, |room| room.requirements());
        SpecialistTerminalView {
            requirements,
            interior_cells: room.as_ref().map_or(0, |room| room.interior_cells().len()),
            specialists: BUILT_IN_SPECIALISTS
                .iter()
                .map(|definition| {
                    let record = world.specialist(definition.id);
                    SpecialistOfferState {
                        definition,
                        recruited: record.is_some(),
                        happiness: record.map(SpecialistRecord::happiness),
                    }
                })
                .collect(),
        }
    }

    pub fn recruit(
        &mut self,
        world: &mut World,
        terminal: ObjectId,
        id: SpecialistId,
    ) -> Result<(), RecruitSpecialistError> {
        specialist_definition(id).ok_or(RecruitSpecialistError::UnknownSpecialist)?;
        if world.specialist(id).is_some() {
            return Err(RecruitSpecialistError::AlreadyRecruited);
        }
        let room = self
            .room(world, terminal)
            .ok_or(RecruitSpecialistError::InvalidTerminal)?;
        if !room.is_valid() {
            return Err(RecruitSpecialistError::UnsuitableHouse);
        }
        let position = nearest_spot(&room, room.centre()).unwrap_or(room.centre());
        world
            .specialists
            .push(SpecialistRecord::new(id, terminal, position));
        Ok(())
    }

    pub fn sync_to_world(&self, entities: &EntityWorld, world: &mut World) {
        for (&id, &entity) in &self.residents {
            let Some(position) = entities
                .get::<&Transform>(entity)
                .ok()
                .map(|transform| transform.position)
            else {
                continue;
            };
            if let Some(record) = world.specialist_mut(id) {
                record.set_position(position);
            }
        }
    }

    pub fn update(
        &mut self,
        world: &mut World,
        entities: &mut EntityWorld,
        material: Handle<Material>,
        player_position: [f32; 2],
        elapsed: Duration,
    ) {
        world.prune_orphaned_specialists();
        self.residents
            .retain(|id, entity| world.specialist(*id).is_some() && entities.contains(*entity));

        self.happiness_elapsed = self.happiness_elapsed.saturating_add(elapsed);
        let refresh_happiness = self.happiness_elapsed >= HAPPINESS_INTERVAL;
        if refresh_happiness {
            self.happiness_elapsed = Duration::ZERO;
        }

        let records: Vec<_> = world.specialists.clone();
        for record in records {
            let Some(room) = self.room(world, record.home_terminal()) else {
                self.unload(record.id(), entities, world);
                continue;
            };
            if refresh_happiness {
                let score = happiness_report(world, &record, &room).score;
                if let Some(saved) = world.specialist_mut(record.id()) {
                    saved.set_happiness(score);
                }
            }
            let distance = distance(record.position(), player_position);
            if !room.is_valid() || distance > RESIDENT_DISTANCE_TILES {
                self.unload(record.id(), entities, world);
                continue;
            }
            let entity = match self.residents.get(&record.id()).copied() {
                Some(entity) => entity,
                None => {
                    let Some(definition) = specialist_definition(record.id()) else {
                        continue;
                    };
                    let position =
                        nearest_spot(&room, record.position()).unwrap_or(record.position());
                    let entity = entities.spawn((
                        Specialist::new(record.id(), record.home_terminal()),
                        Transform::new(position).with_scale([1.25, 1.75]),
                        Collider::new(0.85, 1.7).with_material(0.0, 0.25),
                        Sprite::new(material).with_tint(definition.tint),
                    ));
                    self.residents.insert(record.id(), entity);
                    entity
                }
            };
            update_resident(entities, entity, &room, elapsed.as_secs_f32());
            if let Some(position) = entities
                .get::<&Transform>(entity)
                .ok()
                .map(|transform| transform.position)
                && let Some(saved) = world.specialist_mut(record.id())
            {
                saved.set_position(position);
            }
        }
    }

    fn room(&mut self, world: &World, terminal: ObjectId) -> Option<Arc<RoomAssessment>> {
        let revision = world.housing_revision();
        let current = self
            .rooms
            .get(&terminal)
            .is_some_and(|cached| cached.revision == revision);
        if !current {
            let assessment = Arc::new(assess_room(world, terminal)?);
            self.rooms.insert(
                terminal,
                CachedRoom {
                    revision,
                    assessment,
                },
            );
        }
        self.rooms
            .get(&terminal)
            .map(|cached| Arc::clone(&cached.assessment))
    }

    fn unload(&mut self, id: SpecialistId, entities: &mut EntityWorld, world: &mut World) {
        let Some(entity) = self.residents.remove(&id) else {
            return;
        };
        if let Some(position) = entities
            .get::<&Transform>(entity)
            .ok()
            .map(|transform| transform.position)
            && let Some(record) = world.specialist_mut(id)
        {
            record.set_position(position);
        }
        let _ = entities.despawn(entity);
    }
}

impl World {
    pub fn specialists(&self) -> &[SpecialistRecord] {
        &self.specialists
    }

    pub fn specialist(&self, id: SpecialistId) -> Option<&SpecialistRecord> {
        self.specialists.iter().find(|record| record.id == id)
    }

    pub fn specialist_bonuses(&self) -> SpecialistBonuses {
        SpecialistBonuses::from_ids(self.specialists.iter().map(SpecialistRecord::id))
    }

    pub(crate) fn specialist_mut(&mut self, id: SpecialistId) -> Option<&mut SpecialistRecord> {
        self.specialists.iter_mut().find(|record| record.id == id)
    }

    pub fn prune_orphaned_specialists(&mut self) -> usize {
        let existing_terminals: HashSet<_> = self
            .objects_of_type(FurnitureObject::PROCUREMENT_TERMINAL)
            .map(|object| object.id())
            .collect();
        let previous = self.specialists.len();
        self.specialists
            .retain(|record| existing_terminals.contains(&record.home_terminal()));
        previous - self.specialists.len()
    }
}

pub fn happiness_report(
    world: &World,
    specialist: &SpecialistRecord,
    room: &RoomAssessment,
) -> HappinessReport {
    if !room.is_valid() {
        return HappinessReport {
            score: 0,
            factors: vec![HappinessFactor {
                label: "Unsuitable house",
                delta: -100,
            }],
        };
    }
    let Some(definition) = specialist_definition(specialist.id()) else {
        return HappinessReport {
            score: 0,
            factors: Vec::new(),
        };
    };
    let mut score = 100_i16;
    let mut factors = Vec::new();
    for rule in definition.happiness_rules {
        match *rule {
            HappinessRule::NearbyNoise {
                radius_tiles,
                penalty_per_noise,
            } => {
                let noise = nearby_noise(world, room.centre(), radius_tiles);
                let penalty = i16::try_from(noise.saturating_mul(u32::from(penalty_per_noise)))
                    .unwrap_or(i16::MAX)
                    .min(100);
                if penalty > 0 {
                    score -= penalty;
                    factors.push(HappinessFactor {
                        label: "Nearby machinery noise",
                        delta: -penalty,
                    });
                }
            }
            HappinessRule::BiomePreference {
                preferred,
                mismatch_penalty,
            } => {
                let centre = room.centre();
                let position = TilePos::new(
                    centre[0].floor().max(0.0) as u32,
                    centre[1].floor().max(0.0) as u32,
                );
                let matches = world
                    .biome_at(position)
                    .is_some_and(|biome| preferred.contains(&biome));
                if !matches {
                    let penalty = i16::from(mismatch_penalty);
                    score -= penalty;
                    factors.push(HappinessFactor {
                        label: "Unpreferred biome",
                        delta: -penalty,
                    });
                }
            }
        }
    }
    HappinessReport {
        score: score.clamp(0, 100) as u8,
        factors,
    }
}

fn nearby_noise(world: &World, centre: [f32; 2], radius_tiles: u16) -> u32 {
    let radius = f32::from(radius_tiles);
    let min_x = (centre[0] - radius).floor().max(0.0) as u32;
    let min_y = (centre[1] - radius).floor().max(0.0) as u32;
    let max_x = (centre[0] + radius)
        .ceil()
        .min(world.width().saturating_sub(1) as f32) as u32;
    let max_y = (centre[1] + radius)
        .ceil()
        .min(world.height().saturating_sub(1) as f32) as u32;
    let mut seen = HashSet::new();
    let mut total = 0_u32;
    for chunk_y in min_y / CHUNK_SIZE as u32..=max_y / CHUNK_SIZE as u32 {
        for chunk_x in min_x / CHUNK_SIZE as u32..=max_x / CHUNK_SIZE as u32 {
            for object in world.objects_in_chunk(ChunkPos {
                x: chunk_x,
                y: chunk_y,
            }) {
                if !seen.insert(object.id()) || !object.is_active() {
                    continue;
                }
                let Some(noise) = furniture_definition(object.object_type())
                    .map(|definition| definition.noise_emission())
                    .filter(|&noise| noise > 0)
                else {
                    continue;
                };
                let [width, height] = object.size();
                let object_centre = [
                    object.anchor().x as f32 + (f32::from(width) - 1.0) * 0.5,
                    object.anchor().y as f32 + (f32::from(height) - 1.0) * 0.5,
                ];
                let distance = distance(centre, object_centre);
                if distance < radius {
                    let falloff = 1.0 - distance / radius;
                    total = total.saturating_add((f32::from(noise) * falloff).ceil() as u32);
                }
            }
        }
    }
    total
}

fn nearest_spot(room: &RoomAssessment, position: [f32; 2]) -> Option<[f32; 2]> {
    room.standing_spots()
        .iter()
        .copied()
        .min_by(|left, right| distance(*left, position).total_cmp(&distance(*right, position)))
}

fn update_resident(
    entities: &mut EntityWorld,
    entity: Entity,
    room: &RoomAssessment,
    elapsed: f32,
) {
    let Some(definition) = entities
        .get::<&Specialist>(entity)
        .ok()
        .and_then(|specialist| specialist_definition(specialist.id))
    else {
        return;
    };
    let Ok(mut query) =
        entities.query_one::<(&mut Specialist, &mut Transform, &mut Collider)>(entity)
    else {
        return;
    };
    let Some((specialist, transform, collider)) = query.get() else {
        return;
    };
    if room.standing_spots().is_empty() {
        collider.velocity[0] = 0.0;
        return;
    }
    let dt = elapsed.clamp(0.0, 0.1);
    specialist.idle_seconds = (specialist.idle_seconds - dt).max(0.0);
    specialist.target_index = specialist.target_index.min(room.standing_spots().len() - 1);
    let target = room.standing_spots()[specialist.target_index];
    if (target[0] - transform.position[0]).abs() < 0.3 {
        collider.velocity[0] *= 0.7;
        if specialist.idle_seconds <= 0.0 {
            specialist.target_index = specialist.next_index(room.standing_spots().len());
            specialist.idle_seconds = 1.0 + (specialist.random_state % 250) as f32 / 100.0;
        }
    } else if specialist.idle_seconds <= 0.0 {
        let direction = (target[0] - transform.position[0]).signum();
        collider.velocity[0] = (collider.velocity[0] + direction * 9.0 * dt)
            .clamp(-definition.move_speed, definition.move_speed);
        transform.scale[0] = 1.25_f32.copysign(direction);
    }
}

fn distance(left: [f32; 2], right: [f32; 2]) -> f32 {
    (left[0] - right[0]).hypot(left[1] - right[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BackgroundTile, ForegroundTile, Layer, TileId, TilePos};

    fn house() -> (World, ObjectId) {
        let mut world = World::empty(20, 15, 4).unwrap();
        for y in 2..=11 {
            for x in 2..=12 {
                let boundary = x == 2 || x == 12 || y == 2 || y == 11;
                world
                    .set_tile(
                        x,
                        y,
                        Layer::Foreground,
                        if boundary {
                            ForegroundTile::STONE
                        } else {
                            TileId::EMPTY
                        },
                    )
                    .unwrap();
                world
                    .set_tile(x, y, Layer::Background, BackgroundTile::STONE_WALL)
                    .unwrap();
            }
        }
        for y in 8..=10 {
            world
                .set_tile(2, y, Layer::Foreground, TileId::EMPTY)
                .unwrap();
        }
        world
            .set_tile(8, 2, Layer::Foreground, TileId::new(4))
            .unwrap();
        world
            .place_furniture(FurnitureObject::DOOR, TilePos::new(2, 8))
            .unwrap();
        let terminal = world
            .place_furniture(FurnitureObject::PROCUREMENT_TERMINAL, TilePos::new(6, 9))
            .unwrap();
        world
            .place_furniture(FurnitureObject::BED, TilePos::new(9, 10))
            .unwrap();
        (world, terminal)
    }

    #[test]
    fn a_specialist_can_only_be_recruited_once() {
        let (mut world, terminal) = house();
        let mut system = SpecialistSystem::default();
        assert_eq!(
            system.recruit(&mut world, terminal, SpecialistId::ENGINEER),
            Ok(())
        );
        assert_eq!(
            system.recruit(&mut world, terminal, SpecialistId::ENGINEER),
            Err(RecruitSpecialistError::AlreadyRecruited)
        );
        assert_eq!(world.specialists().len(), 1);
    }

    #[test]
    fn recruited_specialists_combine_their_world_bonuses() {
        let (mut world, terminal) = house();
        let mut system = SpecialistSystem::default();
        for id in [
            SpecialistId::DRILL_ENGINEER,
            SpecialistId::LOGISTICS_ENGINEER,
            SpecialistId::SECURITY_OFFICER,
        ] {
            system.recruit(&mut world, terminal, id).unwrap();
        }

        let bonuses = world.specialist_bonuses();
        assert_eq!(bonuses.drill_speed_percent(), 150);
        assert_eq!(bonuses.drill_depth_tiles(), 80);
        assert_eq!(bonuses.turret_damage_percent(), 125);
        assert_eq!(bonuses.turret_fire_rate_percent(), 125);
        assert!(bonuses.advanced_turret_targeting());
        assert_eq!(bonuses.conveyor_speed_percent(), 150);
        assert_eq!(bonuses.lift_speed_percent(), 125);
        assert_eq!(bonuses.processing_speed_percent(), 125);
    }

    #[test]
    fn active_drills_reduce_happiness() {
        let (mut world, terminal) = house();
        let room = assess_room(&world, terminal).unwrap();
        let specialist = SpecialistRecord::new(SpecialistId::GEOLOGIST, terminal, room.centre());
        let quiet = happiness_report(&world, &specialist, &room).score;
        for x in 14..=17 {
            world
                .set_tile(x, 12, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        let drill = world
            .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(14, 9))
            .unwrap();
        assert!(world.set_furniture_active(drill, true));
        let noisy = happiness_report(&world, &specialist, &room).score;
        assert!(noisy < quiet);
    }

    #[test]
    fn specialists_prefer_definition_selected_biomes() {
        let (mut world, terminal) = house();
        let room = assess_room(&world, terminal).unwrap();
        let specialist =
            SpecialistRecord::new(SpecialistId::DRILL_ENGINEER, terminal, room.centre());
        let normal = happiness_report(&world, &specialist, &room);
        assert!(
            normal
                .factors
                .iter()
                .any(|factor| { factor.label == "Unpreferred biome" && factor.delta == -20 })
        );

        assert!(world.set_biome_in_chunk(
            TilePos::new(room.centre()[0] as u32, room.centre()[1] as u32).chunk(),
            BiomeId::GLOWING_CRYSTAL,
        ));
        let preferred = happiness_report(&world, &specialist, &room);
        assert_eq!(preferred.score, normal.score + 20);
        assert!(
            preferred
                .factors
                .iter()
                .all(|factor| factor.label != "Unpreferred biome")
        );
    }

    #[test]
    fn specialists_are_entities_only_while_the_player_is_nearby() {
        let (mut world, terminal) = house();
        let mut system = SpecialistSystem::default();
        system
            .recruit(&mut world, terminal, SpecialistId::ENGINEER)
            .unwrap();
        let material = Handle {
            index: 0,
            generation: 0,
            _marker: std::marker::PhantomData,
        };
        let mut entities = EntityWorld::new();

        system.update(
            &mut world,
            &mut entities,
            material,
            [7.0, 7.0],
            Duration::from_millis(16),
        );
        assert_eq!(system.resident_count(), 1);

        system.update(
            &mut world,
            &mut entities,
            material,
            [1_000.0, 1_000.0],
            Duration::from_millis(16),
        );
        assert_eq!(system.resident_count(), 0);
        assert!(world.specialist(SpecialistId::ENGINEER).is_some());
    }

    #[test]
    fn cached_housing_is_invalidated_by_background_edits() {
        let (mut world, terminal) = house();
        let mut system = SpecialistSystem::default();
        assert!(
            system
                .terminal_view(&world, Some(terminal))
                .requirements
                .is_valid()
        );

        world
            .set_tile(5, 5, Layer::Background, TileId::EMPTY)
            .unwrap();
        assert!(
            !system
                .terminal_view(&world, Some(terminal))
                .requirements
                .background_walls
        );
    }
}
