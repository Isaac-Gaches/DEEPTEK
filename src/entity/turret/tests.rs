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
fn security_bonuses_improve_stats_and_unlock_targeting() {
    let base = TurretStats::default();
    let no_bonuses = SpecialistBonuses::default();
    assert_eq!(
        effective_target_priority(TargetPriority::Strongest, no_bonuses),
        TargetPriority::Closest
    );

    let bonuses = SpecialistBonuses::from_ids([crate::SpecialistId::SECURITY_OFFICER]);
    let improved = apply_security_bonuses(base, bonuses);
    assert_eq!(improved.damage, 15);
    assert!((improved.fire_interval - 0.6).abs() < f32::EPSILON);
    assert_eq!(
        effective_target_priority(TargetPriority::Strongest, bonuses),
        TargetPriority::Strongest
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
fn turret_arc_is_horizontal_to_sixty_degrees_in_its_facing_direction() {
    let velocity = |degrees: f32| {
        let radians = degrees.to_radians();
        [radians.cos() * 40.0, -radians.sin() * 40.0]
    };

    assert!(velocity_within_facing_arc(
        velocity(0.0),
        FurnitureFacing::Right
    ));
    assert!(velocity_within_facing_arc(
        velocity(59.9),
        FurnitureFacing::Right
    ));
    assert!(!velocity_within_facing_arc(
        velocity(60.1),
        FurnitureFacing::Right
    ));
    assert!(!velocity_within_facing_arc(
        velocity(-1.0),
        FurnitureFacing::Right
    ));
    assert!(velocity_within_facing_arc(
        [-velocity(45.0)[0], velocity(45.0)[1]],
        FurnitureFacing::Left
    ));
    assert!(!velocity_within_facing_arc(
        velocity(45.0),
        FurnitureFacing::Left
    ));
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
fn ammunition_turret_consumes_one_round_only_when_it_fires() {
    let mut terrain = TerrainWorld::empty(40, 24, 0).unwrap();
    for x in 4..=5 {
        terrain
            .set_tile(x, 12, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let turret = terrain
        .place_furniture(FurnitureObject::AMMO_TURRET, TilePos::new(4, 10))
        .unwrap();
    assert!(terrain.set_furniture_active(turret, true));
    let mut entities = World::new();
    LifeformSystem::with_built_ins()
        .spawn(&mut entities, LifeformId::WALKER, material(), [18.0, 10.5])
        .unwrap();
    let power = place_power_network(&mut terrain);
    let mut system = TurretSystem::default();

    system.update(
        &mut entities,
        &mut terrain,
        &power,
        material(),
        material(),
        1.0 / 60.0,
    );
    assert_eq!(entities.query::<&TurretProjectile>().iter().count(), 0);

    assert!(
        terrain
            .container_mut(turret)
            .unwrap()
            .set_slot(0, crate::ItemStack::new(ItemId::TURRET_AMMO, 2))
    );
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
        terrain.container(turret).unwrap().slot(0),
        crate::ItemStack::new(ItemId::TURRET_AMMO, 1)
    );
}

#[test]
fn directional_sentry_fires_only_forward_along_its_lane() {
    let mut terrain = TerrainWorld::empty(40, 24, 0).unwrap();
    terrain
        .set_tile(4, 10, Layer::Background, crate::BackgroundTile::STONE_WALL)
        .unwrap();
    let sentry = terrain
        .place_furniture_facing(
            FurnitureObject::DIRECTIONAL_SENTRY,
            TilePos::new(4, 10),
            FurnitureFacing::Right,
        )
        .unwrap();
    assert!(terrain.set_furniture_active(sentry, true));
    let mut entities = World::new();
    LifeformSystem::with_built_ins()
        .spawn(&mut entities, LifeformId::WALKER, material(), [2.0, 10.3])
        .unwrap();
    LifeformSystem::with_built_ins()
        .spawn(&mut entities, LifeformId::WALKER, material(), [12.0, 5.0])
        .unwrap();
    LifeformSystem::with_built_ins()
        .spawn(&mut entities, LifeformId::WALKER, material(), [18.0, 10.3])
        .unwrap();
    let power = place_power_network(&mut terrain);

    TurretSystem::default().update(
        &mut entities,
        &mut terrain,
        &power,
        material(),
        material(),
        1.0 / 60.0,
    );

    let projectiles: Vec<_> = entities
        .query::<&TurretProjectile>()
        .iter()
        .map(|(_, projectile)| *projectile)
        .collect();
    assert_eq!(projectiles.len(), 1);
    assert_eq!(
        projectiles[0].velocity,
        [DIRECTIONAL_SENTRY_STATS.projectile_speed, 0.0]
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
