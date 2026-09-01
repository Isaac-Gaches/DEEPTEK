use super::{Collider, Sprite, Transform};
use crate::{LightSource, ProjectileKind};
use easy_gpu::assets::Material;
use easy_gpu::assets_manager::Handle;
use hecs::{Entity, World};
use std::collections::HashMap;

const GLOWSTICK_SPEED: f32 = 50.0;
const GLOWSTICK_SPIN_SPEED: f32 = 14.0;
const GLOWSTICK_LIFETIME: f32 = 300.0;
const GLOWSTICK_LIGHT: [f32; 3] = [0.7, 1.0, 0.1];
const GLOWSTICK_RESTITUTION: f32 = 0.3;
const GLOWSTICK_FRICTION: f32 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projectile {
    pub kind: ProjectileKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lifetime {
    remaining: f32,
}

impl Lifetime {
    pub fn new(seconds: f32) -> Self {
        Self {
            remaining: seconds.max(0.0),
        }
    }

    pub const fn remaining(self) -> f32 {
        self.remaining
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DynamicLight {
    pub colour: [f32; 3],
}

impl DynamicLight {
    pub const fn new(colour: [f32; 3]) -> Self {
        Self { colour }
    }
}

pub fn spawn_glowstick(
    entities: &mut World,
    material: Handle<Material>,
    origin: [f32; 2],
    target: [f32; 2],
) -> Entity {
    let difference = [target[0] - origin[0], target[1] - origin[1]];
    let length = difference[0].hypot(difference[1]);
    let direction = if length > f32::EPSILON {
        [difference[0] / length, difference[1] / length]
    } else {
        [1.0, 0.0]
    };
    entities.spawn((
        Projectile {
            kind: ProjectileKind::GlowStick,
        },
        Lifetime::new(GLOWSTICK_LIFETIME),
        DynamicLight::new(GLOWSTICK_LIGHT),
        Transform::new(origin).with_scale([0.65, 0.65]),
        glowstick_collider(direction),
        Sprite::new(material).with_frame(1).with_emissive(0.65),
    ))
}

fn glowstick_collider(direction: [f32; 2]) -> Collider {
    Collider::new(0.2, 0.2)
        .with_velocity([
            direction[0] * GLOWSTICK_SPEED,
            direction[1] * GLOWSTICK_SPEED,
        ])
        .with_material(GLOWSTICK_RESTITUTION, GLOWSTICK_FRICTION)
        .with_angular_motion(
            if direction[0] < 0.0 {
                GLOWSTICK_SPIN_SPEED
            } else {
                -GLOWSTICK_SPIN_SPEED
            },
            0.35,
            0.32,
        )
        .with_drag(0.08, 3.0)
}

/// Retains scratch allocations across frames for expiry and light extraction.
#[derive(Default)]
pub struct ProjectileSystem {
    expired: Vec<Entity>,
    light_cells: HashMap<[i32; 2], [f32; 3]>,
    lights: Vec<LightSource>,
}

impl ProjectileSystem {
    pub fn update(&mut self, entities: &mut World, elapsed: f32) {
        self.expired.clear();
        let elapsed = elapsed.max(0.0);
        for (entity, lifetime) in entities.query::<&mut Lifetime>().iter() {
            lifetime.remaining -= elapsed;
            if lifetime.remaining <= 0.0 {
                self.expired.push(entity);
            }
        }
        for entity in self.expired.drain(..) {
            let _ = entities.despawn(entity);
        }
    }

    /// Collects one maximum-composited light per tile, matching the legacy behavior.
    pub fn collect_lights<'a>(&'a mut self, entities: &World) -> &'a [LightSource] {
        self.light_cells.clear();
        self.lights.clear();
        for (_, (light, transform)) in entities.query::<(&DynamicLight, &Transform)>().iter() {
            let cell = [
                transform.position[0].round() as i32,
                transform.position[1].round() as i32,
            ];
            self.light_cells
                .entry(cell)
                .and_modify(|colour| {
                    for (channel, incoming) in colour.iter_mut().zip(light.colour) {
                        *channel = channel.max(incoming);
                    }
                })
                .or_insert(light.colour);
        }
        self.lights.reserve(self.light_cells.len());
        self.lights.extend(
            self.light_cells
                .drain()
                .map(|(cell, colour)| LightSource::new([cell[0] as f32, cell[1] as f32], colour)),
        );
        &self.lights
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_entities_are_despawned() {
        let mut entities = World::new();
        let entity = entities.spawn((Lifetime::new(0.1),));
        let mut system = ProjectileSystem::default();
        system.update(&mut entities, 0.11);
        assert!(!entities.contains(entity));
    }

    #[test]
    fn colocated_lights_are_merged_by_maximum_channel() {
        let mut entities = World::new();
        entities.spawn((
            Transform::new([4.1, 5.1]),
            DynamicLight::new([0.7, 0.2, 0.1]),
        ));
        entities.spawn((
            Transform::new([4.2, 5.2]),
            DynamicLight::new([0.1, 1.0, 0.3]),
        ));
        let mut system = ProjectileSystem::default();
        let lights = system.collect_lights(&entities);
        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].position(), [4.0, 5.0]);
        assert_eq!(lights[0].colour, [0.7, 1.0, 0.3]);
    }

    #[test]
    fn glowsticks_use_a_low_restitution_material() {
        let collider = glowstick_collider([1.0, 0.0]);
        assert_eq!(collider.restitution, 0.3);
        assert_eq!(collider.friction, 0.8);
    }
}
