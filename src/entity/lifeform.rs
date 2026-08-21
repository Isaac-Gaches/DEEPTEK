use super::components::move_towards;
use super::{Collider, Health, Sprite, Transform};
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

    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u16 {
        self.0
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
    pub acceleration: f32,
    pub max_speed: f32,
    pub air_control: f32,
    pub jump_speed: f32,
    pub stop_distance: f32,
    pub stuck_duration: f32,
    pub jump_cooldown: f32,
    pub minimum_progress_speed: f32,
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
            acceleration: 32.0,
            max_speed: 7.0,
            air_control: 0.3,
            jump_speed: 20.0,
            stop_distance: 0.8,
            stuck_duration: 0.2,
            jump_cooldown: 0.55,
            minimum_progress_speed: 0.35,
        }
    }

    fn is_valid(&self) -> bool {
        self.maximum_health > 0
            && self.collider_size.into_iter().all(|value| value > 0.0)
            && self.sprite_scale.into_iter().all(|value| value > 0.0)
            && self.acceleration >= 0.0
            && self.max_speed >= 0.0
            && (0.0..=1.0).contains(&self.air_control)
            && self.jump_speed >= 0.0
            && self.stop_distance >= 0.0
            && self.stuck_duration >= 0.0
            && self.jump_cooldown >= 0.0
            && self.minimum_progress_speed >= 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lifeform {
    pub id: LifeformId,
    last_x: f32,
    stuck_for: f32,
    jump_cooldown_remaining: f32,
}

impl Lifeform {
    fn new(id: LifeformId, position: [f32; 2]) -> Self {
        Self {
            id,
            last_x: position[0],
            stuck_for: 0.0,
            jump_cooldown_remaining: 0.0,
        }
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

    pub fn spawn(
        &self,
        entities: &mut World,
        id: LifeformId,
        material: Handle<Material>,
        position: [f32; 2],
    ) -> Result<Entity, LifeformSystemError> {
        let definition = self
            .definition(id)
            .ok_or(LifeformSystemError::UnknownId(id))?;
        Ok(entities.spawn((
            Lifeform::new(id, position),
            Transform::new(position).with_scale(definition.sprite_scale),
            Collider::new(definition.collider_size[0], definition.collider_size[1])
                .with_material(0.0, 0.2),
            Health::new(definition.maximum_health),
            Sprite::new(material).with_tint(definition.tint),
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
            let Some(definition) = self.definition(lifeform.id) else {
                collider.velocity[0] = 0.0;
                continue;
            };
            lifeform.jump_cooldown_remaining = (lifeform.jump_cooldown_remaining - dt).max(0.0);

            let delta_x = target_position[0] - transform.position[0];
            let wants_to_move = delta_x.abs() > definition.stop_distance;
            let direction = if wants_to_move { delta_x.signum() } else { 0.0 };
            let control = if collider.on_ground {
                1.0
            } else {
                definition.air_control
            };
            collider.velocity[0] = move_towards(
                collider.velocity[0],
                direction * definition.max_speed,
                definition.acceleration * control * dt,
            );

            if direction != 0.0 {
                transform.scale[0] = definition.sprite_scale[0].copysign(direction);
            }

            let progress = (transform.position[0] - lifeform.last_x).abs();
            let insufficient_progress = progress < definition.minimum_progress_speed * dt;
            if wants_to_move && collider.on_ground && (collider.hit_wall || insufficient_progress) {
                lifeform.stuck_for += dt;
            } else {
                lifeform.stuck_for = 0.0;
            }
            lifeform.last_x = transform.position[0];

            if collider.on_ground
                && lifeform.stuck_for >= definition.stuck_duration
                && lifeform.jump_cooldown_remaining <= 0.0
            {
                collider.velocity[1] = -definition.jump_speed;
                collider.on_ground = false;
                collider.hit_wall = false;
                lifeform.stuck_for = 0.0;
                lifeform.jump_cooldown_remaining = definition.jump_cooldown;
            }
        }
    }
}

/// Definitions installed by `LifeformSystem::with_built_ins`. New enemy
/// species can use the same registration path without changing ECS systems.
pub fn built_in_lifeform_definitions() -> [LifeformDefinition; 1] {
    [LifeformDefinition::walker(
        LifeformId::WALKER,
        "Surface Walker",
    )]
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
    fn grounded_walker_jumps_after_being_stuck() {
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

        system.update(&mut entities, target, 0.1);
        system.update(&mut entities, target, 0.1);

        let collider = entities.get::<&Collider>(walker).unwrap();
        assert!(collider.velocity[1] < 0.0);
        assert!(!collider.on_ground);
    }
}
