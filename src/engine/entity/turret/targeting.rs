use super::*;

pub(super) fn rebuild_target_buckets(
    targets: &[TargetSnapshot],
    buckets: &mut HashMap<[i32; 2], Vec<usize>>,
) {
    buckets.clear();
    for (index, target) in targets.iter().enumerate() {
        buckets
            .entry(target_cell(target.position))
            .or_default()
            .push(index);
    }
}

pub(super) fn target_cell(position: [f32; 2]) -> [i32; 2] {
    [
        (position[0] / TARGET_CELL_SIZE).floor() as i32,
        (position[1] / TARGET_CELL_SIZE).floor() as i32,
    ]
}

pub(super) fn turret_muzzle(anchor: crate::TilePos, size: [u16; 2]) -> [f32; 2] {
    [
        anchor.x as f32 + (f32::from(size[0]) - 1.0) * 0.5,
        anchor.y as f32 + f32::from(size[1]) * 0.28,
    ]
}

pub(super) fn spawn_turret_projectile(
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

pub(super) fn spawn_muzzle_particles(
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

#[cfg(test)]
pub(super) fn select_visible_target<'a>(
    origin: [f32; 2],
    range: f32,
    priority: TargetPriority,
    targets: &'a [TargetSnapshot],
    buckets: &HashMap<[i32; 2], Vec<usize>>,
    terrain: &TerrainWorld,
) -> Option<&'a TargetSnapshot> {
    select_target_solution(origin, range, priority, targets, buckets, terrain, |_| {
        Some([0.0; 2])
    })
    .map(|(target, _)| target)
}

pub(super) fn select_ballistic_shot<'a>(
    origin: [f32; 2],
    stats: TurretStats,
    facing: FurnitureFacing,
    priority: TargetPriority,
    targets: &'a [TargetSnapshot],
    buckets: &HashMap<[i32; 2], Vec<usize>>,
    terrain: &TerrainWorld,
) -> Option<(&'a TargetSnapshot, [f32; 2])> {
    select_target_solution(
        origin,
        stats.range,
        priority,
        targets,
        buckets,
        terrain,
        |target| {
            let velocity = ballistic_velocity(
                origin,
                target.position,
                stats.projectile_speed,
                stats.projectile_gravity,
                stats.projectile_lifetime,
            )?;
            velocity_within_facing_arc(velocity, facing).then_some(velocity)
        },
    )
}

pub(super) fn select_directional_shot<'a>(
    origin: [f32; 2],
    stats: TurretStats,
    facing: FurnitureFacing,
    targets: &'a [TargetSnapshot],
    buckets: &HashMap<[i32; 2], Vec<usize>>,
    terrain: &TerrainWorld,
) -> Option<(&'a TargetSnapshot, [f32; 2])> {
    select_target_solution(
        origin,
        stats.range,
        TargetPriority::Closest,
        targets,
        buckets,
        terrain,
        |target| {
            let centre_y = target.position[1] + target.offset[1];
            let in_front = (target.position[0] - origin[0]) * facing.horizontal_sign() > 0.0;
            let in_lane = (centre_y - origin[1]).abs() <= target.half_extents[1] + 0.35;
            (in_front && in_lane)
                .then_some([stats.projectile_speed * facing.horizontal_sign(), 0.0])
        },
    )
}

pub(super) fn velocity_within_facing_arc(velocity: [f32; 2], facing: FurnitureFacing) -> bool {
    if velocity[0] * facing.horizontal_sign() <= f32::EPSILON {
        return false;
    }
    let elevation = (-velocity[1]).atan2(velocity[0].abs()).to_degrees();
    (-f32::EPSILON..=MAX_TURRET_ELEVATION_DEGREES + f32::EPSILON).contains(&elevation)
}

#[allow(clippy::too_many_arguments)]
fn select_target_solution<'a>(
    origin: [f32; 2],
    range: f32,
    priority: TargetPriority,
    targets: &'a [TargetSnapshot],
    buckets: &HashMap<[i32; 2], Vec<usize>>,
    terrain: &TerrainWorld,
    mut solution: impl FnMut(&TargetSnapshot) -> Option<[f32; 2]>,
) -> Option<(&'a TargetSnapshot, [f32; 2])> {
    let range_squared = range * range;
    let mut best: Option<(&TargetSnapshot, f32, [f32; 2])> = None;
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
                let Some(velocity) = solution(target) else {
                    continue;
                };
                let replace = best.is_none_or(|(current, current_distance, _)| {
                    target_is_better(
                        target,
                        distance_squared,
                        current,
                        current_distance,
                        priority,
                    )
                });
                if replace {
                    best = Some((target, distance_squared, velocity));
                }
            }
        }
    }
    best.map(|(target, _, velocity)| (target, velocity))
}

pub(super) fn target_is_better(
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

pub(super) fn squared_distance(left: [f32; 2], right: [f32; 2]) -> f32 {
    let delta = [right[0] - left[0], right[1] - left[1]];
    delta[0] * delta[0] + delta[1] * delta[1]
}

/// Traverses only the tile cells crossed by the sight segment. Tile centres
/// are integral world coordinates, so their boundaries lie on half units.
pub(super) fn has_line_of_sight(start: [f32; 2], end: [f32; 2], terrain: &TerrainWorld) -> bool {
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

pub(super) fn sight_step(delta: f32) -> i32 {
    if delta > 0.0 {
        1
    } else if delta < 0.0 {
        -1
    } else {
        0
    }
}

pub(super) fn sight_cell(position: [f32; 2]) -> [i64; 2] {
    [
        (position[0] + 0.5).floor() as i64,
        (position[1] + 0.5).floor() as i64,
    ]
}

pub(super) fn tile_blocks_view(terrain: &TerrainWorld, cell: [i64; 2]) -> bool {
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
pub(super) fn ballistic_velocity(
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

pub(super) fn segment_terrain_hit_fraction(
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

pub(super) fn point_hits_terrain(point: [f32; 2], terrain: &TerrainWorld) -> bool {
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

pub(super) fn first_target_hit(
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

pub(super) fn segment_aabb_fraction(
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
