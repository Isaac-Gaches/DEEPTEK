use super::components::move_towards;
use super::{Collider, Energy, Health, Sprite, Transform, Wallet};
use crate::{Layer, POWERED_CABLE_OBJECT, ROPE_OBJECT, TilePos, World as TerrainWorld};
use easy_gpu::assets::Material;
use easy_gpu::assets_manager::Handle;
use hecs::{Entity, World};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Player {
    pub acceleration: f32,
    pub deceleration: f32,
    pub max_speed: f32,
    pub jump_speed: f32,
    pub air_control: f32,
    pub climb_speed: f32,
    /// How early a jump press may arrive before landing.
    pub jump_buffer_time: f32,
    /// How late a jump press may arrive after leaving an edge.
    pub coyote_time: f32,
    jump_buffer_remaining: f32,
    coyote_time_remaining: f32,
    climbing: bool,
    animation_elapsed: f32,
    facing: f32,
    arm: Option<Entity>,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            acceleration: 48.0,
            deceleration: 70.0,
            max_speed: 12.0,
            jump_speed: 24.0,
            air_control: 0.45,
            climb_speed: 18.0,
            jump_buffer_time: 0.12,
            coyote_time: 0.1,
            jump_buffer_remaining: 0.0,
            coyote_time_remaining: 0.0,
            climbing: false,
            animation_elapsed: 0.0,
            facing: 1.0,
            arm: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerInput {
    pub horizontal: f32,
    /// Negative climbs up and positive climbs down.
    pub vertical: f32,
    pub jump_pressed: bool,
}

pub fn spawn_player(
    entities: &mut World,
    body_material: Handle<Material>,
    arm_material: Handle<Material>,
    position: [f32; 2],
) -> Entity {
    let arm = entities.spawn((
        Transform::new([
            position[0] + PLAYER_ARM_OFFSET[0],
            position[1] + PLAYER_ARM_OFFSET[1],
        ])
        .with_scale(PLAYER_ARM_SCALE),
        Sprite::new(arm_material)
            .with_frame(PLAYER_ARM_MIDDLE_FRAME)
            .with_depth(PLAYER_ARM_DEPTH),
    ));
    entities.spawn((
        Player {
            arm: Some(arm),
            ..Player::default()
        },
        Transform::new(position).with_scale(PLAYER_BODY_SCALE),
        Collider::new(1.35, 2.7).with_material(0.0, 0.0),
        Health::new(100),
        Energy::new(100),
        Wallet::new(0),
        Sprite::new(body_material),
    ))
}

const PLAYER_BODY_SCALE: [f32; 2] = [1.55, 3.25];
const PLAYER_ARM_SCALE: [f32; 2] = [0.7, 1.24];
// Facing right: half a tile to the left and a quarter tile above the body centre.
// The horizontal component is mirrored with the rest of the player when facing left.
const PLAYER_ARM_OFFSET: [f32; 2] = [-0.06, 0.05];
const PLAYER_ARM_DEPTH: f32 = 0.09;
const PLAYER_BODY_FRAME_COUNT: u32 = 5;
const PLAYER_BODY_IDLE_FRAME: u32 = 5;
const PLAYER_ARM_MIDDLE_FRAME: u32 = 1;
const PLAYER_ARM_WALK_SEQUENCE: [u32; 4] = [0, 1, 0, 2];
const PLAYER_ANIMATION_FRAME_TIME: f32 = 0.1;

pub fn update_players(
    entities: &mut World,
    terrain: &TerrainWorld,
    input: PlayerInput,
    elapsed: f32,
) {
    let dt = elapsed.clamp(0.0, 0.1);
    let horizontal = input.horizontal.clamp(-1.0, 1.0);

    let vertical = input.vertical.clamp(-1.0, 1.0);

    for (_, (player, transform, collider)) in entities
        .query::<(&mut Player, &Transform, &mut Collider)>()
        .iter()
    {
        let overlaps_climbable = collider_overlaps_climbable(terrain, transform, collider);
        if overlaps_climbable && vertical.abs() > f32::EPSILON {
            player.climbing = true;
        } else if !overlaps_climbable {
            player.climbing = false;
        }
        if player.climbing && input.jump_pressed && vertical.abs() <= f32::EPSILON {
            player.climbing = false;
            collider.gravity_scale = 1.0;
            collider.velocity[1] = -player.jump_speed;
            collider.on_ground = false;
        } else if player.climbing {
            collider.gravity_scale = 0.0;
            collider.velocity[1] = vertical * player.climb_speed;
            collider.on_ground = false;
            player.jump_buffer_remaining = 0.0;
            player.coyote_time_remaining = 0.0;
        } else {
            collider.gravity_scale = 1.0;
        }

        let control = if collider.on_ground {
            1.0
        } else {
            player.air_control
        };
        let target = horizontal * player.max_speed;
        let rate = if horizontal.abs() > f32::EPSILON {
            player.acceleration * control
        } else {
            player.deceleration * control
        };
        collider.velocity[0] = move_towards(collider.velocity[0], target, rate * dt);

        if collider.on_ground {
            player.coyote_time_remaining = player.coyote_time.max(0.0);
        } else {
            player.coyote_time_remaining = (player.coyote_time_remaining - dt).max(0.0);
        }
        if input.jump_pressed && !player.climbing {
            player.jump_buffer_remaining = player.jump_buffer_time.max(0.0);
        } else {
            player.jump_buffer_remaining = (player.jump_buffer_remaining - dt).max(0.0);
        }

        if !player.climbing
            && player.jump_buffer_remaining > 0.0
            && player.coyote_time_remaining > 0.0
        {
            collider.velocity[1] = -player.jump_speed;
            collider.on_ground = false;
            player.jump_buffer_remaining = 0.0;
            player.coyote_time_remaining = 0.0;
        }
    }
}

/// Synchronizes the layered player sprites after physics has moved the body.
pub fn update_player_animation(entities: &mut World, elapsed: f32) {
    let dt = elapsed.clamp(0.0, 0.1);
    let mut arm_update = None;
    for (_, (player, transform, collider, sprite)) in entities
        .query::<(&mut Player, &mut Transform, &Collider, &mut Sprite)>()
        .iter()
    {
        if collider.velocity[0].abs() > 0.05 {
            player.facing = collider.velocity[0].signum();
        }
        let facing = player.facing;
        transform.scale[0] = PLAYER_BODY_SCALE[0] * facing;
        transform.scale[1] = PLAYER_BODY_SCALE[1];

        let walking = collider.on_ground && !player.climbing && collider.velocity[0].abs() > 0.2;
        let step = if walking {
            player.animation_elapsed += dt;
            (player.animation_elapsed / PLAYER_ANIMATION_FRAME_TIME) as usize
        } else {
            player.animation_elapsed = 0.0;
            0
        };
        sprite.frame = if walking {
            step as u32 % PLAYER_BODY_FRAME_COUNT
        } else {
            PLAYER_BODY_IDLE_FRAME
        };

        if let Some(arm) = player.arm {
            let local_offset = [PLAYER_ARM_OFFSET[0] * facing, PLAYER_ARM_OFFSET[1]];
            let cosine = transform.rotation.cos();
            let sine = transform.rotation.sin();
            let offset = [
                local_offset[0] * cosine - local_offset[1] * sine,
                local_offset[0] * sine + local_offset[1] * cosine,
            ];
            arm_update = Some((
                arm,
                [
                    transform.position[0] + offset[0],
                    transform.position[1] + offset[1],
                ],
                transform.rotation,
                facing,
                if walking {
                    PLAYER_ARM_WALK_SEQUENCE[step % PLAYER_ARM_WALK_SEQUENCE.len()]
                } else {
                    PLAYER_ARM_MIDDLE_FRAME
                },
            ));
        }
    }

    if let Some((arm, position, rotation, facing, frame)) = arm_update {
        if let Ok(mut transform) = entities.get::<&mut Transform>(arm) {
            transform.position = position;
            transform.rotation = rotation;
            transform.scale = [PLAYER_ARM_SCALE[0] * facing, PLAYER_ARM_SCALE[1]];
        }
        if let Ok(mut sprite) = entities.get::<&mut Sprite>(arm) {
            sprite.frame = frame;
        }
    }
}

fn collider_overlaps_climbable(
    terrain: &TerrainWorld,
    transform: &Transform,
    collider: &Collider,
) -> bool {
    let centre = [
        transform.position[0] + collider.offset[0],
        transform.position[1] + collider.offset[1],
    ];
    let minimum = [
        (centre[0] - collider.half_extents[0] - 0.5).ceil() as i32,
        (centre[1] - collider.half_extents[1] - 0.5).ceil() as i32,
    ];
    let maximum = [
        (centre[0] + collider.half_extents[0] + 0.5).floor() as i32,
        (centre[1] + collider.half_extents[1] + 0.5).floor() as i32,
    ];
    for y in minimum[1]..=maximum[1] {
        for x in minimum[0]..=maximum[0] {
            if x < 0 || y < 0 || x >= terrain.width() as i32 || y >= terrain.height() as i32 {
                continue;
            }
            if terrain
                .object_at(TilePos::new(x as u32, y as u32))
                .is_some_and(|object| {
                    matches!(object.object_type(), ROPE_OBJECT | POWERED_CABLE_OBJECT)
                })
                && terrain
                    .tile(x as u32, y as u32, Layer::Foreground)
                    .is_ok_and(|tile| tile == crate::TileId::EMPTY)
            {
                return true;
            }
        }
    }
    false
}

pub fn entity_position(entities: &World, entity: Entity) -> Option<[f32; 2]> {
    entities
        .get::<&Transform>(entity)
        .ok()
        .map(|transform| transform.position)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ForegroundTile;

    #[test]
    fn spawned_player_has_full_hud_resources() {
        // A material handle is an arena key; spawning does not dereference it.
        let mut entities = World::new();
        let body_material = Handle {
            index: 0,
            generation: 0,
            _marker: std::marker::PhantomData,
        };
        let arm_material = Handle {
            index: 1,
            generation: 0,
            _marker: std::marker::PhantomData,
        };
        let player = spawn_player(&mut entities, body_material, arm_material, [2.0, 3.0]);
        assert_eq!(*entities.get::<&Health>(player).unwrap(), Health::new(100));
        assert_eq!(*entities.get::<&Energy>(player).unwrap(), Energy::new(100));
        assert_eq!(entities.get::<&Wallet>(player).unwrap().money(), 0);
        assert_eq!(entities.len(), 2);
    }

    #[test]
    fn walking_animates_body_and_arms_in_the_requested_sequence() {
        let mut entities = World::new();
        let body_material = Handle {
            index: 0,
            generation: 0,
            _marker: std::marker::PhantomData,
        };
        let arm_material = Handle {
            index: 1,
            generation: 0,
            _marker: std::marker::PhantomData,
        };
        let player = spawn_player(&mut entities, body_material, arm_material, [4.0, 4.0]);
        {
            let mut collider = entities.get::<&mut Collider>(player).unwrap();
            collider.on_ground = true;
            collider.velocity[0] = 4.0;
        }
        let arm = entities.get::<&Player>(player).unwrap().arm.unwrap();

        let mut body_frames = Vec::new();
        let mut arm_frames = Vec::new();
        for elapsed in [0.0, 0.101, 0.101, 0.101, 0.101] {
            update_player_animation(&mut entities, elapsed);
            body_frames.push(entities.get::<&Sprite>(player).unwrap().frame);
            arm_frames.push(entities.get::<&Sprite>(arm).unwrap().frame);
        }

        assert_eq!(body_frames, [0, 1, 2, 3, 4]);
        assert_eq!(arm_frames, [0, 1, 0, 2, 0]);
    }

    #[test]
    fn stationary_player_uses_the_dedicated_idle_body_frame() {
        let mut entities = World::new();
        let material = Handle {
            index: 0,
            generation: 0,
            _marker: std::marker::PhantomData,
        };
        let player = spawn_player(&mut entities, material, material, [4.0, 4.0]);
        entities.get::<&mut Collider>(player).unwrap().on_ground = true;

        update_player_animation(&mut entities, 0.1);

        assert_eq!(
            entities.get::<&Sprite>(player).unwrap().frame,
            PLAYER_BODY_IDLE_FRAME
        );
    }

    #[test]
    fn player_body_and_attached_arm_flip_together() {
        let mut entities = World::new();
        let material = Handle {
            index: 0,
            generation: 0,
            _marker: std::marker::PhantomData,
        };
        let player = spawn_player(&mut entities, material, material, [4.0, 4.0]);
        entities.get::<&mut Collider>(player).unwrap().velocity[0] = -1.0;
        let arm = entities.get::<&Player>(player).unwrap().arm.unwrap();

        update_player_animation(&mut entities, 0.0);

        assert!(entities.get::<&Transform>(player).unwrap().scale[0] < 0.0);
        assert!(entities.get::<&Transform>(arm).unwrap().scale[0] < 0.0);
        assert!(
            entities.get::<&Transform>(arm).unwrap().position[0]
                > entities.get::<&Transform>(player).unwrap().position[0]
        );
    }

    #[test]
    fn grounded_player_accelerates_and_jumps() {
        let mut entities = World::new();
        let terrain = TerrainWorld::empty(12, 12, 0).unwrap();
        let entity = entities.spawn((
            Player::default(),
            Transform::new([4.0, 4.0]),
            Collider {
                on_ground: true,
                ..Collider::new(1.0, 2.0)
            },
        ));
        update_players(
            &mut entities,
            &terrain,
            PlayerInput {
                horizontal: 1.0,
                vertical: 0.0,
                jump_pressed: true,
            },
            0.1,
        );
        let collider = entities.get::<&Collider>(entity).unwrap();
        assert!(collider.velocity[0] > 0.0);
        assert_eq!(collider.velocity[1], -Player::default().jump_speed);
        assert!(!collider.on_ground);
    }

    #[test]
    fn grounded_player_does_not_auto_jump_a_one_tile_obstacle() {
        let mut terrain = TerrainWorld::empty(12, 12, 0).unwrap();
        for x in 0..terrain.width() {
            terrain
                .set_tile(x, 6, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        terrain
            .set_tile(5, 5, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        let mut entities = World::new();
        let entity = entities.spawn((
            Player::default(),
            Transform::new([4.0, 4.1499]),
            Collider {
                on_ground: true,
                ..Collider::new(1.35, 2.7)
            },
        ));

        update_players(
            &mut entities,
            &terrain,
            PlayerInput {
                horizontal: 1.0,
                ..PlayerInput::default()
            },
            0.016,
        );

        let collider = entities.get::<&Collider>(entity).unwrap();
        assert_eq!(collider.velocity[1], 0.0);
        assert!(collider.velocity[0] > 0.0);
        assert!(collider.on_ground);
    }

    #[test]
    fn jump_pressed_just_before_landing_is_buffered() {
        let mut entities = World::new();
        let terrain = TerrainWorld::empty(12, 12, 0).unwrap();
        let entity = entities.spawn((
            Player::default(),
            Transform::new([4.0, 4.0]),
            Collider::new(1.0, 2.0),
        ));

        update_players(
            &mut entities,
            &terrain,
            PlayerInput {
                horizontal: 0.0,
                vertical: 0.0,
                jump_pressed: true,
            },
            0.016,
        );
        assert_eq!(entities.get::<&Collider>(entity).unwrap().velocity[1], 0.0);

        entities.get::<&mut Collider>(entity).unwrap().on_ground = true;
        update_players(&mut entities, &terrain, PlayerInput::default(), 0.016);

        let collider = entities.get::<&Collider>(entity).unwrap();
        assert_eq!(collider.velocity[1], -Player::default().jump_speed);
        assert!(!collider.on_ground);
    }

    #[test]
    fn player_can_jump_just_after_leaving_an_edge() {
        let mut entities = World::new();
        let terrain = TerrainWorld::empty(12, 12, 0).unwrap();
        let entity = entities.spawn((
            Player::default(),
            Transform::new([4.0, 4.0]),
            Collider {
                on_ground: true,
                ..Collider::new(1.0, 2.0)
            },
        ));

        update_players(&mut entities, &terrain, PlayerInput::default(), 0.016);
        entities.get::<&mut Collider>(entity).unwrap().on_ground = false;
        update_players(
            &mut entities,
            &terrain,
            PlayerInput {
                horizontal: 0.0,
                vertical: 0.0,
                jump_pressed: true,
            },
            0.016,
        );

        assert_eq!(
            entities.get::<&Collider>(entity).unwrap().velocity[1],
            -Player::default().jump_speed
        );
    }

    #[test]
    fn overlapping_player_climbs_rope_and_can_jump_free() {
        let mut terrain = TerrainWorld::empty(12, 12, 0).unwrap();
        terrain
            .set_tile(4, 2, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        terrain.place_or_extend_rope(TilePos::new(4, 3)).unwrap();
        terrain.place_or_extend_rope(TilePos::new(4, 3)).unwrap();
        let mut entities = World::new();
        let entity = entities.spawn((
            Player::default(),
            Transform::new([4.0, 3.5]),
            Collider::new(1.0, 2.0),
        ));

        update_players(
            &mut entities,
            &terrain,
            PlayerInput {
                vertical: -1.0,
                ..PlayerInput::default()
            },
            0.016,
        );
        {
            let collider = entities.get::<&Collider>(entity).unwrap();
            assert_eq!(collider.gravity_scale, 0.0);
            assert_eq!(collider.velocity[1], -Player::default().climb_speed);
        }

        update_players(
            &mut entities,
            &terrain,
            PlayerInput {
                jump_pressed: true,
                ..PlayerInput::default()
            },
            0.016,
        );
        let collider = entities.get::<&Collider>(entity).unwrap();
        assert_eq!(collider.gravity_scale, 1.0);
        assert_eq!(collider.velocity[1], -Player::default().jump_speed);
    }

    #[test]
    fn powered_cable_is_climbable_like_rope() {
        let mut terrain = TerrainWorld::empty(12, 12, 0).unwrap();
        terrain
            .set_tile(4, 1, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        terrain
            .place_furniture(
                crate::FurnitureObject::POWERED_CABLE_ANCHOR,
                TilePos::new(4, 2),
            )
            .unwrap();
        terrain
            .place_or_extend_powered_cable(TilePos::new(4, 2))
            .unwrap();
        terrain
            .place_or_extend_powered_cable(TilePos::new(4, 2))
            .unwrap();
        let mut entities = World::new();
        let entity = entities.spawn((
            Player::default(),
            Transform::new([4.0, 3.5]),
            Collider::new(1.0, 2.0),
        ));

        update_players(
            &mut entities,
            &terrain,
            PlayerInput {
                vertical: 1.0,
                ..PlayerInput::default()
            },
            0.016,
        );
        let collider = entities.get::<&Collider>(entity).unwrap();
        assert_eq!(collider.gravity_scale, 0.0);
        assert_eq!(collider.velocity[1], Player::default().climb_speed);
    }
}
