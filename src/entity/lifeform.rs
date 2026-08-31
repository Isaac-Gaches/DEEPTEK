mod spawning;

pub use spawning::{
    GLOWGNAT_MIN_MACHINERY_ATTENTION, LifeformSimulation, LifeformSimulationConfig,
    LifeformSimulationUpdate, LifeformSpawnView,
};

use super::components::move_towards;
use super::{Collider, Health, Sprite, Transform};
use crate::BiomeId;
use easy_gpu::assets::Material;
use easy_gpu::assets_manager::Handle;
use hecs::{Entity, World};
use std::error::Error;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct LifeformId(u16);

impl LifeformId {
    pub const WALKER: Self = Self(1);
    pub const GLOWGNAT: Self = Self(2);

    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LifeformLocomotion {
    Grounded,
    Flying {
        drift_speed: f32,
        drift_frequency: f32,
    },
}

impl LifeformLocomotion {
    pub const fn is_flying(self) -> bool {
        matches!(self, Self::Flying { .. })
    }
}

#[derive(Clone, Copy)]
pub struct LifeformMaterials {
    pub walker: Handle<Material>,
    pub glowgnat: Handle<Material>,
}

impl LifeformMaterials {
    pub const fn new(walker: Handle<Material>, glowgnat: Handle<Material>) -> Self {
        Self { walker, glowgnat }
    }

    const fn for_lifeform(self, id: LifeformId) -> Handle<Material> {
        if id.raw() == LifeformId::GLOWGNAT.raw() {
            self.glowgnat
        } else {
            self.walker
        }
    }
}

impl From<Handle<Material>> for LifeformMaterials {
    fn from(material: Handle<Material>) -> Self {
        Self::new(material, material)
    }
}

/// Data shared by every lifeform of one species. Registering another definition
/// adds a species without changing the movement system or ECS component layout.
#[derive(Clone, Debug, PartialEq)]
pub struct LifeformDefinition {
    pub id: LifeformId,
    pub name: String,
    pub maximum_health: u16,
    pub collider_size: [f32; 2],
    pub sprite_scale: [f32; 2],
    pub tint: [f32; 4],
    pub locomotion: LifeformLocomotion,
    pub acceleration: f32,
    pub max_speed: f32,
    pub air_control: f32,
    pub jump_speed: f32,
    pub stop_distance: f32,
    pub stuck_duration: f32,
    pub jump_cooldown: f32,
    pub minimum_progress_speed: f32,
    pub attack_damage: u16,
    pub attack_range: f32,
    pub attack_interval: f32,
    /// Deliberately much weaker than machinery attacks. This is used only when
    /// a noisy machine has attracted the lifeform and a block obstructs it.
    pub block_attack_damage: u16,
    pub block_attack_interval: f32,
    pub spawn_biomes: Vec<BiomeId>,
    pub minimum_spawn_attention: u32,
    pub spawn_weight: u16,
}

impl LifeformDefinition {
    pub fn walker(id: LifeformId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            maximum_health: 40,
            collider_size: [1.1, 1.7],
            sprite_scale: [1.65, 2.1],
            tint: [0.62, 1.0, 0.58, 1.0],
            locomotion: LifeformLocomotion::Grounded,
            acceleration: 32.0,
            max_speed: 7.0,
            air_control: 0.3,
            jump_speed: 20.0,
            stop_distance: 0.8,
            stuck_duration: 0.2,
            jump_cooldown: 0.55,
            minimum_progress_speed: 0.35,
            attack_damage: 8,
            attack_range: 0.65,
            attack_interval: 0.75,
            block_attack_damage: 4,
            block_attack_interval: 5.0 / 3.0,
            spawn_biomes: vec![BiomeId::NORMAL, BiomeId::GLOWING_CRYSTAL],
            minimum_spawn_attention: 0,
            spawn_weight: 100,
        }
    }

    pub fn glowgnat(id: LifeformId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            maximum_health: 18,
            collider_size: [0.85, 0.60],
            sprite_scale: [1.35, 1.05],
            tint: [0.88, 1.0, 0.78, 1.0],
            locomotion: LifeformLocomotion::Flying {
                drift_speed: 1.25,
                drift_frequency: 0.85,
            },
            acceleration: 6.5,
            max_speed: 3.2,
            air_control: 1.0,
            jump_speed: 0.0,
            stop_distance: 0.45,
            stuck_duration: 0.6,
            jump_cooldown: 0.0,
            minimum_progress_speed: 0.15,
            attack_damage: 5,
            attack_range: 0.35,
            attack_interval: 1.1,
            block_attack_damage: 4,
            block_attack_interval: 5.0 / 3.0,
            spawn_biomes: vec![BiomeId::GLOWING_CRYSTAL],
            minimum_spawn_attention: GLOWGNAT_MIN_MACHINERY_ATTENTION,
            spawn_weight: 100,
        }
    }

    fn is_valid(&self) -> bool {
        self.maximum_health > 0
            && self.collider_size.into_iter().all(|value| value > 0.0)
            && self.sprite_scale.into_iter().all(|value| value > 0.0)
            && match self.locomotion {
                LifeformLocomotion::Grounded => true,
                LifeformLocomotion::Flying {
                    drift_speed,
                    drift_frequency,
                } => drift_speed >= 0.0 && drift_frequency >= 0.0,
            }
            && self.acceleration >= 0.0
            && self.max_speed >= 0.0
            && (0.0..=1.0).contains(&self.air_control)
            && self.jump_speed >= 0.0
            && self.stop_distance >= 0.0
            && self.stuck_duration >= 0.0
            && self.jump_cooldown >= 0.0
            && self.minimum_progress_speed >= 0.0
            && self.attack_damage > 0
            && self.attack_range >= 0.0
            && self.attack_interval > 0.0
            && self.block_attack_damage > 0
            && self.block_attack_interval > 0.0
            && !self.spawn_biomes.is_empty()
            && self.spawn_biomes.iter().all(|biome| biome.is_known())
            && self.spawn_weight > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lifeform {
    pub id: LifeformId,
    last_position: [f32; 2],
    stuck_for: f32,
    jump_cooldown_remaining: f32,
    attack_cooldown_remaining: f32,
    block_attack_cooldown_remaining: f32,
    drift_phase: f32,
    roam_origin: [f32; 2],
    roam_target: [f32; 2],
    roam_seconds_remaining: f32,
    random_state: u32,
    retreat_seconds_remaining: f32,
    return_seconds_remaining: f32,
    recovery_target: [f32; 2],
    retreat_direction: [f32; 2],
}

impl Lifeform {
    fn new(id: LifeformId, position: [f32; 2]) -> Self {
        Self {
            id,
            last_position: position,
            stuck_for: 0.0,
            jump_cooldown_remaining: 0.0,
            attack_cooldown_remaining: 0.0,
            block_attack_cooldown_remaining: 0.0,
            drift_phase: (position[0] * 0.173 + position[1] * 0.317).fract(),
            roam_origin: position,
            roam_target: position,
            roam_seconds_remaining: 0.0,
            random_state: position[0]
                .to_bits()
                .rotate_left(11)
                .wrapping_add(position[1].to_bits().rotate_left(23))
                .wrapping_add(u32::from(id.raw()).wrapping_mul(0x9E37_79B9)),
            retreat_seconds_remaining: 0.0,
            return_seconds_remaining: 0.0,
            recovery_target: position,
            retreat_direction: [0.0, 0.0],
        }
    }

    fn next_random(&mut self) -> f32 {
        self.random_state = self
            .random_state
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.random_state >> 8) as f32 / 16_777_215.0
    }

    fn roaming_target(&mut self, position: [f32; 2], flying: bool, dt: f32) -> [f32; 2] {
        self.roam_seconds_remaining = (self.roam_seconds_remaining - dt).max(0.0);
        let distance_from_origin =
            (position[0] - self.roam_origin[0]).hypot(position[1] - self.roam_origin[1]);
        if self.roam_seconds_remaining <= 0.0 {
            let range = if distance_from_origin > 12.0 {
                2.0
            } else {
                8.0
            };
            let centre = if distance_from_origin > 12.0 {
                self.roam_origin
            } else {
                position
            };
            self.roam_target = [
                centre[0] + (self.next_random() * 2.0 - 1.0) * range,
                if flying {
                    centre[1] + (self.next_random() * 2.0 - 1.0) * range * 0.45
                } else {
                    position[1]
                },
            ];
            self.roam_seconds_remaining = 2.5 + self.next_random() * 3.0;
        }
        self.roam_target
    }

    fn recovery_destination(
        &mut self,
        normal_target: [f32; 2],
        position: [f32; 2],
        dt: f32,
    ) -> [f32; 2] {
        if self.retreat_seconds_remaining > 0.0 {
            self.retreat_seconds_remaining = (self.retreat_seconds_remaining - dt).max(0.0);
            return [
                position[0] + self.retreat_direction[0] * 6.0,
                position[1] + self.retreat_direction[1] * 4.0,
            ];
        }
        if self.return_seconds_remaining > 0.0 {
            self.return_seconds_remaining = (self.return_seconds_remaining - dt).max(0.0);
            return self.recovery_target;
        }
        normal_target
    }

    fn begin_recovery(&mut self, position: [f32; 2], target: [f32; 2], flying: bool) {
        let delta = [target[0] - position[0], target[1] - position[1]];
        let distance = delta[0].hypot(delta[1]);
        let fallback = if self.next_random() < 0.5 { -1.0 } else { 1.0 };
        self.retreat_direction = if distance > 0.1 {
            [-delta[0] / distance, -delta[1] / distance]
        } else {
            [fallback, if flying { fallback * 0.4 } else { 0.0 }]
        };
        if !flying {
            self.retreat_direction[1] = 0.0;
        }
        self.recovery_target = target;
        self.retreat_seconds_remaining = 0.9;
        self.return_seconds_remaining = 1.6;
        self.stuck_for = 0.0;
    }

    fn is_recovering(&self) -> bool {
        self.retreat_seconds_remaining > 0.0 || self.return_seconds_remaining > 0.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifeformSystemError {
    DuplicateId(LifeformId),
    InvalidDefinition(LifeformId),
    UnknownId(LifeformId),
}

impl fmt::Display for LifeformSystemError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateId(id) => {
                write!(formatter, "lifeform ID {} is already registered", id.raw())
            }
            Self::InvalidDefinition(id) => {
                write!(
                    formatter,
                    "lifeform ID {} has an invalid definition",
                    id.raw()
                )
            }
            Self::UnknownId(id) => write!(formatter, "lifeform ID {} is not registered", id.raw()),
        }
    }
}

impl Error for LifeformSystemError {}

/// Owns species definitions and updates all `Lifeform + Transform + Collider` entities.
#[derive(Clone, Debug, Default)]
pub struct LifeformSystem {
    definitions: Vec<Option<LifeformDefinition>>,
}

impl LifeformSystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_built_ins() -> Self {
        let mut system = Self::new();
        for definition in built_in_lifeform_definitions() {
            system
                .register(definition)
                .expect("built-in lifeform definitions are valid");
        }
        system
    }

    pub fn register(&mut self, definition: LifeformDefinition) -> Result<(), LifeformSystemError> {
        if !definition.is_valid() {
            return Err(LifeformSystemError::InvalidDefinition(definition.id));
        }
        let index = usize::from(definition.id.raw());
        if self.definitions.len() <= index {
            self.definitions.resize_with(index + 1, || None);
        }
        if self.definitions[index].is_some() {
            return Err(LifeformSystemError::DuplicateId(definition.id));
        }
        self.definitions[index] = Some(definition);
        Ok(())
    }

    pub fn definition(&self, id: LifeformId) -> Option<&LifeformDefinition> {
        self.definitions
            .get(usize::from(id.raw()))
            .and_then(Option::as_ref)
    }

    pub fn definitions(&self) -> impl Iterator<Item = &LifeformDefinition> {
        self.definitions.iter().filter_map(Option::as_ref)
    }

    pub(super) fn select_spawn(
        &self,
        biome: BiomeId,
        attention: u32,
        hash: u64,
    ) -> Option<LifeformId> {
        let total_weight: u64 = self
            .definitions()
            .filter(|definition| {
                attention >= definition.minimum_spawn_attention
                    && definition.spawn_biomes.contains(&biome)
            })
            .map(|definition| u64::from(definition.spawn_weight))
            .sum();
        if total_weight == 0 {
            return None;
        }
        let mut ticket = hash.rotate_left(11) % total_weight;
        for definition in self.definitions().filter(|definition| {
            attention >= definition.minimum_spawn_attention
                && definition.spawn_biomes.contains(&biome)
        }) {
            let weight = u64::from(definition.spawn_weight);
            if ticket < weight {
                return Some(definition.id);
            }
            ticket -= weight;
        }
        None
    }

    pub fn spawn(
        &self,
        entities: &mut World,
        id: LifeformId,
        materials: impl Into<LifeformMaterials>,
        position: [f32; 2],
    ) -> Result<Entity, LifeformSystemError> {
        let definition = self
            .definition(id)
            .ok_or(LifeformSystemError::UnknownId(id))?;
        let materials = materials.into();
        let collider = Collider::new(definition.collider_size[0], definition.collider_size[1])
            .with_material(0.0, 0.2)
            .with_gravity_scale(if definition.locomotion.is_flying() {
                0.0
            } else {
                1.0
            });
        let sprite = Sprite::new(materials.for_lifeform(id))
            .with_tint(definition.tint)
            .with_emissive(if id == LifeformId::GLOWGNAT {
                0.85
            } else {
                0.0
            });
        Ok(entities.spawn((
            Lifeform::new(id, position),
            Transform::new(position).with_scale(definition.sprite_scale),
            collider,
            Health::new(definition.maximum_health),
            sprite,
        )))
    }

    pub fn update(&self, entities: &mut World, target: Entity, elapsed: f32) {
        let Some(target_position) = entities
            .get::<&Transform>(target)
            .ok()
            .map(|transform| transform.position)
        else {
            return;
        };
        let dt = elapsed.clamp(0.0, 0.1);
        if dt == 0.0 {
            return;
        }

        for (_, (lifeform, transform, collider)) in entities
            .query::<(&mut Lifeform, &mut Transform, &mut Collider)>()
            .iter()
        {
            self.update_one(
                lifeform,
                transform,
                collider,
                target_position,
                [0.0, 0.0],
                true,
                dt,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn update_one(
        &self,
        lifeform: &mut Lifeform,
        transform: &mut Transform,
        collider: &mut Collider,
        target_position: [f32; 2],
        separation: [f32; 2],
        engaged: bool,
        dt: f32,
    ) {
        let Some(definition) = self.definition(lifeform.id) else {
            collider.velocity[0] = 0.0;
            return;
        };
        lifeform.jump_cooldown_remaining = (lifeform.jump_cooldown_remaining - dt).max(0.0);
        lifeform.attack_cooldown_remaining = (lifeform.attack_cooldown_remaining - dt).max(0.0);
        lifeform.block_attack_cooldown_remaining =
            (lifeform.block_attack_cooldown_remaining - dt).max(0.0);

        let flying = definition.locomotion.is_flying();
        let normal_target = if engaged {
            target_position
        } else {
            lifeform.roaming_target(transform.position, flying, dt)
        };
        let target_position = lifeform.recovery_destination(normal_target, transform.position, dt);

        match definition.locomotion {
            LifeformLocomotion::Grounded => update_grounded_lifeform(
                lifeform,
                transform,
                collider,
                target_position,
                separation,
                engaged,
                definition,
                dt,
            ),
            LifeformLocomotion::Flying {
                drift_speed,
                drift_frequency,
            } => update_flying_lifeform(
                lifeform,
                transform,
                collider,
                target_position,
                separation,
                definition,
                drift_speed,
                drift_frequency,
                dt,
            ),
        }
    }

    pub(super) fn is_flying(&self, id: LifeformId) -> bool {
        self.definition(id)
            .is_some_and(|definition| definition.locomotion.is_flying())
    }

    pub(super) fn attack_if_ready(
        &self,
        lifeform: &mut Lifeform,
        attacker_position: [f32; 2],
        attacker_half_extents: [f32; 2],
        target_position: [f32; 2],
        target_half_extents: [f32; 2],
    ) -> Option<u16> {
        let definition = self.definition(lifeform.id)?;
        if lifeform.attack_cooldown_remaining > 0.0 {
            return None;
        }
        let gap_x = (attacker_position[0] - target_position[0]).abs()
            - attacker_half_extents[0]
            - target_half_extents[0];
        let gap_y = (attacker_position[1] - target_position[1]).abs()
            - attacker_half_extents[1]
            - target_half_extents[1];
        if gap_x.max(0.0).hypot(gap_y.max(0.0)) > definition.attack_range {
            return None;
        }
        lifeform.attack_cooldown_remaining = definition.attack_interval;
        Some(definition.attack_damage)
    }

    pub(super) fn block_attack_if_ready(&self, lifeform: &mut Lifeform) -> Option<u16> {
        let definition = self.definition(lifeform.id)?;
        if lifeform.block_attack_cooldown_remaining > 0.0 {
            return None;
        }
        lifeform.block_attack_cooldown_remaining = definition.block_attack_interval;
        Some(definition.block_attack_damage)
    }
}

#[allow(clippy::too_many_arguments)]
fn update_grounded_lifeform(
    lifeform: &mut Lifeform,
    transform: &mut Transform,
    collider: &mut Collider,
    target_position: [f32; 2],
    separation: [f32; 2],
    engaged: bool,
    definition: &LifeformDefinition,
    dt: f32,
) {
    let delta_x = target_position[0] - transform.position[0];
    let wants_to_move = delta_x.abs() > definition.stop_distance;
    let direction = if wants_to_move { delta_x.signum() } else { 0.0 };
    let control = if collider.on_ground {
        1.0
    } else {
        definition.air_control
    };
    let desired_velocity = (direction * definition.max_speed
        + separation[0] * definition.max_speed * 0.65)
        .clamp(-definition.max_speed, definition.max_speed);
    collider.velocity[0] = move_towards(
        collider.velocity[0],
        desired_velocity,
        definition.acceleration * control * dt,
    );

    if direction != 0.0 {
        transform.scale[0] = definition.sprite_scale[0].copysign(direction);
    }

    let progress = (transform.position[0] - lifeform.last_position[0]).abs();
    let insufficient_progress = progress < definition.minimum_progress_speed * dt;
    if wants_to_move && collider.on_ground && (collider.hit_wall || insufficient_progress) {
        lifeform.stuck_for += dt;
    } else {
        lifeform.stuck_for = 0.0;
    }
    lifeform.last_position = transform.position;

    if collider.on_ground
        && lifeform.stuck_for >= definition.stuck_duration
        && lifeform.jump_cooldown_remaining <= 0.0
        && !lifeform.is_recovering()
    {
        if !engaged {
            collider.velocity[1] = -definition.jump_speed;
            collider.on_ground = false;
        }
        collider.hit_wall = false;
        lifeform.begin_recovery(transform.position, target_position, false);
        lifeform.jump_cooldown_remaining = definition.jump_cooldown;
    }
}

#[allow(clippy::too_many_arguments)]
fn update_flying_lifeform(
    lifeform: &mut Lifeform,
    transform: &mut Transform,
    collider: &mut Collider,
    target_position: [f32; 2],
    separation: [f32; 2],
    definition: &LifeformDefinition,
    drift_speed: f32,
    drift_frequency: f32,
    dt: f32,
) {
    lifeform.drift_phase = (lifeform.drift_phase + drift_frequency * dt).fract();
    let drift_wave = 1.0 - 4.0 * (lifeform.drift_phase - 0.5).abs();
    let delta = [
        target_position[0] - transform.position[0],
        target_position[1] - transform.position[1],
    ];
    let distance = delta[0].hypot(delta[1]);
    let direction = if distance > definition.stop_distance.max(f32::EPSILON) {
        [delta[0] / distance, delta[1] / distance]
    } else {
        [0.0, 0.0]
    };
    let perpendicular = if direction == [0.0, 0.0] {
        [1.0, -0.5]
    } else {
        [-direction[1], direction[0]]
    };
    let desired = [
        direction[0] * definition.max_speed
            + perpendicular[0] * drift_speed * drift_wave
            + separation[0] * definition.max_speed * 0.7,
        direction[1] * definition.max_speed
            + perpendicular[1] * drift_speed * drift_wave
            + separation[1] * definition.max_speed * 0.7,
    ];
    let maximum_delta = definition.acceleration * dt;
    collider.velocity[0] = move_towards(collider.velocity[0], desired[0], maximum_delta);
    collider.velocity[1] = move_towards(collider.velocity[1], desired[1], maximum_delta);
    collider.on_ground = false;
    let progress = (transform.position[0] - lifeform.last_position[0])
        .hypot(transform.position[1] - lifeform.last_position[1]);
    let insufficient_progress = progress < definition.minimum_progress_speed * dt;
    if distance > definition.stop_distance && (collider.hit_wall || insufficient_progress) {
        lifeform.stuck_for += dt;
    } else {
        lifeform.stuck_for = 0.0;
    }
    lifeform.last_position = transform.position;
    if lifeform.stuck_for >= definition.stuck_duration && !lifeform.is_recovering() {
        lifeform.begin_recovery(transform.position, target_position, true);
        collider.hit_wall = false;
    }
    if collider.velocity[0].abs() > 0.05 {
        transform.scale[0] = definition.sprite_scale[0].copysign(collider.velocity[0]);
    }
}

/// Definitions installed by `LifeformSystem::with_built_ins`. New enemy
/// species can use the same registration path without changing ECS systems.
pub fn built_in_lifeform_definitions() -> [LifeformDefinition; 2] {
    [
        LifeformDefinition::walker(LifeformId::WALKER, "Surface Walker"),
        LifeformDefinition::glowgnat(LifeformId::GLOWGNAT, "Glowgnat"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definitions_are_data_driven_and_reject_duplicates() {
        let mut system = LifeformSystem::new();
        let definition = LifeformDefinition::walker(LifeformId::new(9), "Test Walker");
        system.register(definition.clone()).unwrap();
        assert_eq!(system.definition(definition.id), Some(&definition));
        assert_eq!(
            system.register(definition),
            Err(LifeformSystemError::DuplicateId(LifeformId::new(9)))
        );
    }

    #[test]
    fn walker_accelerates_towards_target() {
        let system = LifeformSystem::with_built_ins();
        let mut entities = World::new();
        let target = entities.spawn((Transform::new([10.0, 0.0]),));
        let walker = entities.spawn((
            Lifeform::new(LifeformId::WALKER, [0.0, 0.0]),
            Transform::new([0.0, 0.0]),
            Collider::new(1.0, 1.0),
        ));

        system.update(&mut entities, target, 0.1);

        assert!(entities.get::<&Collider>(walker).unwrap().velocity[0] > 0.0);
    }

    #[test]
    fn grounded_walker_retreats_then_returns_after_being_stuck() {
        let system = LifeformSystem::with_built_ins();
        let mut entities = World::new();
        let target = entities.spawn((Transform::new([10.0, 0.0]),));
        let walker = entities.spawn((
            Lifeform::new(LifeformId::WALKER, [0.0, 0.0]),
            Transform::new([0.0, 0.0]),
            Collider {
                on_ground: true,
                hit_wall: true,
                ..Collider::new(1.0, 1.0)
            },
        ));

        for _ in 0..5 {
            system.update(&mut entities, target, 0.1);
        }

        let collider = entities.get::<&Collider>(walker).unwrap();
        assert!(collider.velocity[0] < 0.0);
        drop(collider);

        for _ in 0..14 {
            system.update(&mut entities, target, 0.1);
        }
        assert!(entities.get::<&Collider>(walker).unwrap().velocity[0] > 0.0);
    }

    #[test]
    fn glowgnat_steers_in_two_dimensions_without_gravity() {
        let system = LifeformSystem::with_built_ins();
        let mut entities = World::new();
        let target = entities.spawn((Transform::new([8.0, 6.0]),));
        let glowgnat = entities.spawn((
            Lifeform::new(LifeformId::GLOWGNAT, [0.0, 0.0]),
            Transform::new([0.0, 0.0]),
            Collider::new(1.0, 1.0).with_gravity_scale(0.0),
        ));

        system.update(&mut entities, target, 0.1);

        let collider = entities.get::<&Collider>(glowgnat).unwrap();
        assert!(collider.velocity[0] > 0.0);
        assert!(collider.velocity[1] > 0.0);
        assert_eq!(collider.gravity_scale, 0.0);
    }

    #[test]
    fn peaceful_roaming_changes_destination_without_leaving_its_local_area() {
        let mut lifeform = Lifeform::new(LifeformId::WALKER, [20.0, 10.0]);
        let first = lifeform.roaming_target([20.0, 10.0], false, 0.1);
        lifeform.roam_seconds_remaining = 0.0;
        let second = lifeform.roaming_target([20.0, 10.0], false, 0.1);
        assert_ne!(first, second);
        assert!((first[0] - 20.0).abs() <= 8.0);
        assert!((second[0] - 20.0).abs() <= 8.0);
    }

    #[test]
    fn built_in_lifeforms_break_dirt_in_about_five_seconds() {
        let system = LifeformSystem::with_built_ins();
        for id in [LifeformId::WALKER, LifeformId::GLOWGNAT] {
            let mut lifeform = Lifeform::new(id, [0.0, 0.0]);
            let mut transform = Transform::new([0.0, 0.0]);
            let mut collider = Collider::new(1.0, 1.0);
            let mut damage = system.block_attack_if_ready(&mut lifeform).unwrap();
            let mut elapsed = 0.0;
            while damage < 16 {
                system.update_one(
                    &mut lifeform,
                    &mut transform,
                    &mut collider,
                    [0.0, 0.0],
                    [0.0, 0.0],
                    false,
                    0.1,
                );
                elapsed += 0.1;
                if let Some(applied) = system.block_attack_if_ready(&mut lifeform) {
                    damage += applied;
                }
            }
            assert!((4.9..=5.2).contains(&elapsed), "elapsed was {elapsed}");
        }
    }
}
