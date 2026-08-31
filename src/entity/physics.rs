use super::{Collider, Transform};
use crate::World;
use hecs::World as EntityWorld;

const COLLISION_EPSILON: f32 = 0.0001;
const BOUNCE_REST_SPEED: f32 = 2.0;
const GROUND_SPIN_EPSILON: f32 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicsConfig {
    /// Positive Y points down in terrain coordinates.
    pub gravity: f32,
    pub terminal_velocity: f32,
    /// Maximum movement per collision step. Values below one prevent tunnelling.
    pub max_step_distance: f32,
    pub max_frame_time: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            gravity: 60.0,
            terminal_velocity: 100.0,
            max_step_distance: 0.45,
            max_frame_time: 0.1,
        }
    }
}

/// Advances every enabled `Transform + Collider` pair against foreground tiles.
///
/// Collision work is proportional to actual displacement and collider footprint;
/// the complete terrain is never scanned.
pub fn update_colliders(
    entities: &mut EntityWorld,
    terrain: &World,
    elapsed: f32,
    config: PhysicsConfig,
) {
    let dt = elapsed.clamp(0.0, config.max_frame_time.max(0.0));
    if dt == 0.0 {
        return;
    }

    for (_, (transform, collider)) in entities.query::<(&mut Transform, &mut Collider)>().iter() {
        if !collider.enabled {
            continue;
        }

        update_collider(transform, collider, terrain, dt, config);
    }
}

pub(super) fn update_collider(
    transform: &mut Transform,
    collider: &mut Collider,
    terrain: &World,
    dt: f32,
    config: PhysicsConfig,
) {
    let linear_damping = (-collider.linear_drag * dt).exp();
    collider.velocity[0] *= linear_damping;
    collider.velocity[1] *= linear_damping;
    let terminal_velocity = config.terminal_velocity.abs();
    collider.velocity[1] = (collider.velocity[1] + config.gravity * collider.gravity_scale * dt)
        .clamp(-terminal_velocity, terminal_velocity);
    collider.on_ground = false;
    collider.hit_wall = false;

    move_axis(
        transform,
        collider,
        terrain,
        0,
        collider.velocity[0] * dt,
        config.max_step_distance,
        dt,
    );
    move_axis(
        transform,
        collider,
        terrain,
        1,
        collider.velocity[1] * dt,
        config.max_step_distance,
        dt,
    );

    if collider.on_ground {
        collider.velocity[0] *= (-collider.ground_drag * dt).exp();
        if collider.velocity[0].abs() < GROUND_SPIN_EPSILON {
            collider.velocity[0] = 0.0;
        }
    }

    if collider.rotation_enabled {
        let rotational_drag = collider.angular_drag
            + if collider.on_ground {
                collider.friction * 4.0
            } else {
                0.0
            };
        collider.angular_velocity *= (-rotational_drag * dt).exp();
        if collider.on_ground
            && collider.velocity[0].abs() < GROUND_SPIN_EPSILON
            && collider.angular_velocity.abs() < GROUND_SPIN_EPSILON
        {
            collider.angular_velocity = 0.0;
        }
        transform.rotation =
            (transform.rotation + collider.angular_velocity * dt).rem_euclid(std::f32::consts::TAU);
    } else {
        collider.angular_velocity = 0.0;
        transform.rotation = 0.0;
    }
}

fn move_axis(
    transform: &mut Transform,
    collider: &mut Collider,
    terrain: &World,
    axis: usize,
    distance: f32,
    max_step_distance: f32,
    dt: f32,
) {
    if distance.abs() <= f32::EPSILON {
        return;
    }

    let step_limit = max_step_distance.clamp(0.05, 0.95);
    let steps = (distance.abs() / step_limit).ceil().max(1.0) as u32;
    let step = distance / steps as f32;

    for _ in 0..steps {
        transform.position[axis] += step;
        let Some(hit) = first_overlapping_tile(transform, collider, terrain, axis, step) else {
            continue;
        };

        let half = collider.half_extents[axis];
        let offset = collider.offset[axis];
        let tile = if axis == 0 { hit[0] } else { hit[1] } as f32;
        if step > 0.0 {
            transform.position[axis] = tile - 0.5 - half - offset - COLLISION_EPSILON;
            if axis == 1 {
                collider.on_ground = true;
            }
        } else {
            transform.position[axis] = tile + 0.5 + half - offset + COLLISION_EPSILON;
        }

        let impact_velocity = collider.velocity[axis];
        if axis == 0 {
            collider.hit_wall = true;
        }
        let rebound_velocity = -impact_velocity * collider.restitution;
        collider.velocity[axis] = if rebound_velocity.abs() < BOUNCE_REST_SPEED {
            0.0
        } else {
            rebound_velocity
        };
        let tangent = 1 - axis;
        collider.velocity[tangent] *= (-collider.friction * 6.0 * dt).exp();

        if axis == 1 && collider.rotation_enabled {
            let rolling_velocity = -collider.velocity[tangent] / collider.rotation_radius;
            let spin_coupling = 1.0 - (-collider.friction * 8.0 * dt).exp();
            collider.angular_velocity +=
                (rolling_velocity - collider.angular_velocity) * spin_coupling;
        }
        break;
    }
}

fn first_overlapping_tile(
    transform: &Transform,
    collider: &Collider,
    terrain: &World,
    axis: usize,
    direction: f32,
) -> Option<[i32; 2]> {
    let centre = [
        transform.position[0] + collider.offset[0],
        transform.position[1] + collider.offset[1],
    ];
    let min = [
        centre[0] - collider.half_extents[0],
        centre[1] - collider.half_extents[1],
    ];
    let max = [
        centre[0] + collider.half_extents[0],
        centre[1] + collider.half_extents[1],
    ];
    let min_tile = [(min[0] + 0.5).floor() as i32, (min[1] + 0.5).floor() as i32];
    let max_tile = [(max[0] - 0.5).ceil() as i32, (max[1] - 0.5).ceil() as i32];

    let mut best = None;
    for y in min_tile[1]..=max_tile[1] {
        for x in min_tile[0]..=max_tile[0] {
            if tile_is_solid(terrain, x, y) && aabb_overlaps_tile(min, max, x as f32, y as f32) {
                let hit = [x, y];
                let replace = best.is_none_or(|current: [i32; 2]| {
                    if direction > 0.0 {
                        hit[axis] < current[axis]
                    } else {
                        hit[axis] > current[axis]
                    }
                });
                if replace {
                    best = Some(hit);
                }
            }
        }
    }
    best
}

#[inline]
fn aabb_overlaps_tile(min: [f32; 2], max: [f32; 2], x: f32, y: f32) -> bool {
    max[0] > x - 0.5 && min[0] < x + 0.5 && max[1] > y - 0.5 && min[1] < y + 0.5
}

#[inline]
fn tile_is_solid(terrain: &World, x: i32, y: i32) -> bool {
    let Ok(x) = u32::try_from(x) else {
        return true;
    };
    let Ok(y) = u32::try_from(y) else {
        return true;
    };
    if x >= terrain.width() || y >= terrain.height() {
        return true;
    }
    terrain.is_collision_cell(crate::TilePos::new(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BackgroundTile, ForegroundTile, FurnitureObject, Layer, TilePos};

    fn floor_world() -> World {
        let mut terrain = World::empty(12, 12, 0).unwrap();
        for x in 0..terrain.width() {
            terrain
                .set_tile(x, 6, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        terrain
    }

    #[test]
    fn falling_body_lands_without_tunnelling() {
        let terrain = floor_world();
        let mut entities = EntityWorld::new();
        let entity = entities.spawn((
            Transform::new([4.0, 1.0]),
            Collider::new(1.0, 1.0)
                .with_velocity([0.0, 100.0])
                .with_gravity_scale(0.0),
        ));

        update_colliders(&mut entities, &terrain, 0.1, PhysicsConfig::default());

        let transform = entities.get::<&Transform>(entity).unwrap();
        let collider = entities.get::<&Collider>(entity).unwrap();
        assert!((transform.position[1] - 4.9999).abs() < 0.001);
        assert!(collider.on_ground);
        assert_eq!(collider.velocity[1], 0.0);
    }

    #[test]
    fn horizontal_collision_checks_only_the_local_footprint() {
        let mut terrain = World::empty(12, 12, 0).unwrap();
        terrain
            .set_tile(6, 3, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        let mut entities = EntityWorld::new();
        let entity = entities.spawn((
            Transform::new([2.0, 3.0]),
            Collider::new(1.0, 1.0)
                .with_velocity([80.0, 0.0])
                .with_gravity_scale(0.0),
        ));

        update_colliders(&mut entities, &terrain, 0.1, PhysicsConfig::default());

        let transform = entities.get::<&Transform>(entity).unwrap();
        let collider = entities.get::<&Collider>(entity).unwrap();
        assert!((transform.position[0] - 4.9999).abs() < 0.001);
        assert!(collider.hit_wall);
    }

    #[test]
    fn falling_body_lands_on_a_structural_directional_sentry() {
        let mut terrain = World::empty(12, 12, 0).unwrap();
        terrain
            .set_tile(5, 6, Layer::Background, BackgroundTile::STONE_WALL)
            .unwrap();
        terrain
            .place_furniture(FurnitureObject::DIRECTIONAL_SENTRY, TilePos::new(5, 6))
            .unwrap();
        let mut entities = EntityWorld::new();
        let entity = entities.spawn((
            Transform::new([5.0, 2.0]),
            Collider::new(1.0, 1.0)
                .with_velocity([0.0, 100.0])
                .with_gravity_scale(0.0),
        ));

        update_colliders(&mut entities, &terrain, 0.1, PhysicsConfig::default());

        let transform = entities.get::<&Transform>(entity).unwrap();
        let collider = entities.get::<&Collider>(entity).unwrap();
        assert!((transform.position[1] - 4.9999).abs() < 0.001);
        assert!(collider.on_ground);
    }

    #[test]
    fn angular_motion_is_damped_by_the_physics_body() {
        let terrain = World::empty(12, 12, 0).unwrap();
        let mut entities = EntityWorld::new();
        let entity = entities.spawn((
            Transform::new([4.0, 4.0]),
            Collider::new(0.2, 0.2)
                .with_gravity_scale(0.0)
                .with_angular_motion(14.0, 1.0, 0.32),
        ));

        update_colliders(&mut entities, &terrain, 0.1, PhysicsConfig::default());

        let transform = entities.get::<&Transform>(entity).unwrap();
        let collider = entities.get::<&Collider>(entity).unwrap();
        assert!(transform.rotation > 0.0);
        assert!(collider.angular_velocity < 14.0);
    }

    #[test]
    fn small_bounces_settle_on_the_floor() {
        let terrain = floor_world();
        let mut entities = EntityWorld::new();
        let entity = entities.spawn((
            Transform::new([4.0, 4.0]),
            Collider::new(0.2, 0.2)
                .with_velocity([1.0, 2.0])
                .with_material(0.7, 0.7)
                .with_angular_motion(8.0, 0.35, 0.32),
        ));

        for _ in 0..300 {
            update_colliders(
                &mut entities,
                &terrain,
                1.0 / 60.0,
                PhysicsConfig::default(),
            );
        }

        let collider = entities.get::<&Collider>(entity).unwrap();
        assert!(collider.on_ground);
        assert_eq!(collider.velocity[1], 0.0);
        assert!(collider.velocity[0].abs() < 0.01);
        assert!(collider.angular_velocity.abs() < 0.1);
    }

    #[test]
    fn grounded_drag_slows_horizontal_motion() {
        let terrain = floor_world();
        let mut entities = EntityWorld::new();
        let entity = entities.spawn((
            Transform::new([4.0, 5.39]),
            Collider::new(0.2, 0.2)
                .with_velocity([10.0, 0.0])
                .with_drag(0.0, 3.0),
        ));

        for _ in 0..30 {
            update_colliders(
                &mut entities,
                &terrain,
                1.0 / 60.0,
                PhysicsConfig::default(),
            );
        }

        let collider = entities.get::<&Collider>(entity).unwrap();
        assert!(collider.velocity[0] < 3.0);
    }

    #[test]
    fn ordinary_colliders_remain_upright() {
        let terrain = floor_world();
        let mut entities = EntityWorld::new();
        let entity = entities.spawn((
            Transform::new([4.0, 5.39]).with_rotation(1.25),
            Collider::new(1.0, 1.0)
                .with_velocity([8.0, 0.0])
                .with_material(0.0, 0.8),
        ));

        update_colliders(
            &mut entities,
            &terrain,
            1.0 / 60.0,
            PhysicsConfig::default(),
        );

        let transform = entities.get::<&Transform>(entity).unwrap();
        let collider = entities.get::<&Collider>(entity).unwrap();
        assert_eq!(transform.rotation, 0.0);
        assert_eq!(collider.angular_velocity, 0.0);
    }
}
