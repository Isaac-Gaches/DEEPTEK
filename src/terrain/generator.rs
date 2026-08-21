use super::{
    BackgroundTile, CHUNK_SIZE, ForegroundTile, Layer, SEA_LEVEL_PERCENT, World, WorldError,
    available_threads, nature, parallel_mut,
};

const NOISE_ONE: i64 = 1 << 10;
const OVERHANG_DEPTH: i64 = 18;
const CAVE_LANES: usize = 3;

/// Deterministic, chunk-parallel terrain generator.
///
/// Large-scale features are sampled once per world column. Chunks then generate
/// independently using integer noise, so output is reproducible across platforms
/// and worker counts without allocating a world-sized density field.
#[derive(Clone, Copy, Debug)]
pub struct WorldGenerator {
    seed: u64,
    threads: usize,
}

#[derive(Clone, Copy)]
struct ColumnProfile {
    surface: u32,
    cave_centres: [i32; CAVE_LANES],
    cave_radius_sq: [i32; CAVE_LANES],
}

impl WorldGenerator {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            threads: available_threads(),
        }
    }

    pub fn with_threads(mut self, threads: usize) -> Self {
        self.threads = threads.max(1);
        self
    }

    pub fn generate(self, width: u32, height: u32) -> Result<World, WorldError> {
        let mut world = World::empty(width, height, self.seed)?;
        let columns: Vec<_> = (0..width)
            .map(|x| column_profile(self.seed, x, height))
            .collect();
        let chunks_wide = world.chunks_wide;

        parallel_mut(&mut world.chunks, self.threads, |index, chunk| {
            let chunk_x = index as u32 % chunks_wide;
            let chunk_y = index as u32 / chunks_wide;
            let origin_x = chunk_x * CHUNK_SIZE as u32;
            let origin_y = chunk_y * CHUNK_SIZE as u32;
            let valid_width = (width - origin_x).min(CHUNK_SIZE as u32) as usize;
            let valid_height = (height - origin_y).min(CHUNK_SIZE as u32) as usize;

            for local_x in 0..valid_width {
                let x = origin_x + local_x as u32;
                let profile = columns[x as usize];
                for local_y in 0..valid_height {
                    let y = origin_y + local_y as u32;
                    let nominal_depth = i64::from(y) - i64::from(profile.surface);
                    let solid = is_ground(self.seed, x, y, height, nominal_depth, profile);
                    if solid {
                        let foreground = if nominal_depth <= 5
                            && has_air_neighbour(self.seed, x, y, width, height, &columns)
                        {
                            ForegroundTile::GRASS
                        } else if nominal_depth <= 5 {
                            ForegroundTile::DIRT
                        } else {
                            ForegroundTile::STONE
                        };
                        chunk.set_tile(local_x, local_y, Layer::Foreground, foreground);
                    }

                    // Carving affects the foreground only. Retained walls make
                    // underground caves dark and visually distinct from sky.
                    if nominal_depth >= 0 {
                        let background = if nominal_depth <= 10 {
                            BackgroundTile::DIRT_WALL
                        } else {
                            BackgroundTile::STONE_WALL
                        };
                        chunk.set_tile(local_x, local_y, Layer::Background, background);
                    }
                }
            }
        })?;
        nature::populate_natural_objects(&mut world);
        Ok(world)
    }
}

/// Uses the same eight-neighbour footprint as the marching-squares renderer.
/// Coordinates beyond the world are not air, matching `neighbour_mask`.
#[inline]
fn has_air_neighbour(
    seed: u64,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    columns: &[ColumnProfile],
) -> bool {
    for dx in -1..=1 {
        for dy in -1..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let Some((sample_x, sample_y)) = x
                .checked_add_signed(dx)
                .zip(y.checked_add_signed(dy))
                .filter(|&(sample_x, sample_y)| sample_x < width && sample_y < height)
            else {
                continue;
            };
            let profile = columns[sample_x as usize];
            let depth = i64::from(sample_y) - i64::from(profile.surface);
            if !is_ground(seed, sample_x, sample_y, height, depth, profile) {
                return true;
            }
        }
    }
    false
}

fn column_profile(seed: u64, x: u32, height: u32) -> ColumnProfile {
    if height <= 2 {
        return ColumnProfile {
            surface: height.saturating_sub(1),
            cave_centres: [height as i32; CAVE_LANES],
            cave_radius_sq: [0; CAVE_LANES],
        };
    }

    let height_i = i64::from(height);
    let base = height_i * i64::from(SEA_LEVEL_PERCENT) / 100;
    let broad_hills = noise_1d(seed ^ 0xA11C_E551, x, 9) * height_i / (NOISE_ONE * 9);
    let foothills = noise_1d(seed ^ 0xB04D_1E55, x, 7) * height_i / (NOISE_ONE * 20);

    // A ridged low-frequency signal makes occasional tall mountain chains.
    let ridge_noise = noise_1d(seed ^ 0xC0FF_EE11, x, 8).abs();
    let mountain = (NOISE_ONE - ridge_noise).max(0) * height_i / (NOISE_ONE * 7);

    // Quantised continental noise creates true rock faces instead of making
    // every elevation change a shallow walkable slope.
    let plateau = noise_1d(seed ^ 0xC11F_F5E7, x, 8);
    let cliff = if plateau > NOISE_ONE / 3 {
        -(height_i / 14)
    } else if plateau < -NOISE_ONE / 3 {
        height_i / 18
    } else {
        0
    };

    let margin = (height_i / 12).clamp(1, 24);
    let surface = (base + broad_hills + foothills - mountain + cliff)
        .clamp(margin, height_i - margin.max(2)) as u32;

    let mut cave_centres = [0; CAVE_LANES];
    let mut cave_radius_sq = [0; CAVE_LANES];
    for lane in 0..CAVE_LANES {
        let lane_seed = seed ^ 0xCA7E_0000_0000_0000 ^ (lane as u64).wrapping_mul(0x9E37_79B9);
        let depth_percent = 48 + lane as i64 * 18;
        let winding = noise_1d(lane_seed, x, 7) * height_i / (NOISE_ONE * 10)
            + noise_1d(lane_seed ^ 0x51DE, x, 5) * height_i / (NOISE_ONE * 28);
        cave_centres[lane] = (height_i * depth_percent / 100 + winding) as i32;

        let base_radius = (height / 70).clamp(3, 12) as i64;
        let radius_noise = noise_1d(lane_seed ^ 0x0BAD_5EED, x, 6);
        let radius = (base_radius + radius_noise * base_radius / (NOISE_ONE * 3)).max(2);
        cave_radius_sq[lane] = (radius * radius) as i32;
    }

    ColumnProfile {
        surface,
        cave_centres,
        cave_radius_sq,
    }
}

#[inline]
fn is_ground(
    seed: u64,
    x: u32,
    y: u32,
    height: u32,
    nominal_depth: i64,
    profile: ColumnProfile,
) -> bool {
    let mut solid = nominal_depth >= 0;

    // Only sample the 2D overhang field near the surface. Its signed-density
    // perturbation creates shelves, arches, and undercuts while leaving deep
    // terrain on the very cheap path.
    if nominal_depth.abs() <= OVERHANG_DEPTH {
        let coarse = noise_2d(seed ^ 0x00AE_2AA9, x, y, 4);
        let detail = noise_2d(seed ^ 0x00DE_7A11, x, y, 3) / 3;
        let displacement = (coarse + detail) * 10 / NOISE_ONE;
        solid = nominal_depth + displacement >= 0;
    }
    if !solid || nominal_depth < 9 || y + 3 >= height {
        return solid;
    }

    // Three broad, continuous, independently winding paths cross chunk boundaries.
    // Squared distance avoids square roots in this per-tile hot path.
    for lane in 0..CAVE_LANES {
        let distance = y as i32 - profile.cave_centres[lane];
        if distance * distance <= profile.cave_radius_sq[lane] {
            return false;
        }
    }

    // Cellular-looking pockets from two octaves form small caves and short
    // connecting passages away from the guaranteed long cave paths.
    let caves = noise_2d(seed ^ 0x5A11_CA7E, x, y, 5) + noise_2d(seed ^ 0x5A11_DE7A, x, y, 3) / 2;
    caves <= NOISE_ONE * 2 / 3
}

/// Cubic-interpolated value noise in the range approximately -1024..=1024.
#[inline]
fn noise_1d(seed: u64, x: u32, scale_shift: u32) -> i64 {
    let cell = x >> scale_shift;
    let mask = (1_u32 << scale_shift) - 1;
    let fraction = i64::from(x & mask);
    let span = 1_i64 << scale_shift;
    let weight = smooth_weight(fraction, span);
    lerp(
        hash_signed(seed, u64::from(cell)),
        hash_signed(seed, u64::from(cell + 1)),
        weight,
    )
}

#[inline]
fn noise_2d(seed: u64, x: u32, y: u32, scale_shift: u32) -> i64 {
    let cell_x = x >> scale_shift;
    let cell_y = y >> scale_shift;
    let mask = (1_u32 << scale_shift) - 1;
    let span = 1_i64 << scale_shift;
    let weight_x = smooth_weight(i64::from(x & mask), span);
    let weight_y = smooth_weight(i64::from(y & mask), span);
    let key = |cx: u32, cy: u32| (u64::from(cy) << 32) | u64::from(cx);
    let top = lerp(
        hash_signed(seed, key(cell_x, cell_y)),
        hash_signed(seed, key(cell_x + 1, cell_y)),
        weight_x,
    );
    let bottom = lerp(
        hash_signed(seed, key(cell_x, cell_y + 1)),
        hash_signed(seed, key(cell_x + 1, cell_y + 1)),
        weight_x,
    );
    lerp(top, bottom, weight_y)
}

#[inline]
fn smooth_weight(value: i64, span: i64) -> i64 {
    // Smoothstep in fixed point, scaled to NOISE_ONE.
    value * value * (3 * span - 2 * value) * NOISE_ONE / (span * span * span)
}

#[inline]
fn lerp(left: i64, right: i64, weight: i64) -> i64 {
    left + (right - left) * weight / NOISE_ONE
}

#[inline]
fn hash_signed(seed: u64, position: u64) -> i64 {
    let mut value = seed ^ position.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    (value & 0x7ff) as i64 - NOISE_ONE
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NaturalObject, TilePos};

    #[test]
    fn generation_is_deterministic_across_worker_counts() {
        let serial = WorldGenerator::new(42)
            .with_threads(1)
            .generate(257, 129)
            .unwrap();
        let parallel = WorldGenerator::new(42)
            .with_threads(4)
            .generate(257, 129)
            .unwrap();
        assert_eq!(serial, parallel);
    }

    #[test]
    fn generator_populates_ground_and_leaves_sky() {
        let world = WorldGenerator::new(7).generate(100, 100).unwrap();
        assert_eq!(
            world.tile(50, 0, Layer::Foreground).unwrap(),
            ForegroundTile::AIR
        );
        assert_ne!(
            world.tile(50, 99, Layer::Foreground).unwrap(),
            ForegroundTile::AIR
        );
        assert_ne!(
            world.tile(50, 99, Layer::Background).unwrap(),
            BackgroundTile::NONE
        );
    }

    #[test]
    fn generated_world_has_relief_overhangs_and_caves() {
        let world = WorldGenerator::new(0xD33F_7E57)
            .generate(1_024, 300)
            .unwrap();
        let mut surface_min = world.height();
        let mut surface_max = 0;
        let mut enclosed_air = 0;
        let mut cave_wall_tiles = 0;
        let mut underground_wall_tiles = 0;
        let mut overhang_columns = 0;

        for x in 0..world.width() {
            let occupied: Vec<_> = (0..world.height())
                .map(|y| world.tile(x, y, Layer::Foreground).unwrap() != ForegroundTile::AIR)
                .collect();
            if let Some(first) = occupied.iter().position(|&solid| solid) {
                surface_min = surface_min.min(first as u32);
                surface_max = surface_max.max(first as u32);
                if occupied[first + 1..].iter().any(|&solid| !solid) {
                    overhang_columns += 1;
                }
            }
            enclosed_air += occupied
                .windows(3)
                .filter(|window| window[0] && !window[1] && window[2])
                .count();
            cave_wall_tiles += (0..world.height())
                .filter(|&y| {
                    world.tile(x, y, Layer::Foreground).unwrap() == ForegroundTile::AIR
                        && world.tile(x, y, Layer::Background).unwrap() != BackgroundTile::NONE
                })
                .count();
            underground_wall_tiles += (0..world.height())
                .filter(|&y| world.tile(x, y, Layer::Background).unwrap() != BackgroundTile::NONE)
                .count();
        }

        assert!(
            surface_max - surface_min >= 30,
            "expected substantial terrain relief"
        );
        assert!(
            overhang_columns >= 100,
            "expected caves or undercuts below surfaces"
        );
        assert!(enclosed_air > 0, "expected enclosed cave tiles");
        assert!(
            cave_wall_tiles * 100 >= underground_wall_tiles * 15,
            "expected prominent caves to occupy at least 15% of underground walls"
        );
    }

    #[test]
    fn all_exposed_dirt_including_inner_corners_is_grass() {
        let world = WorldGenerator::new(0xD33F_7E57)
            .generate(1_024, 300)
            .unwrap();

        for x in 0..world.width() {
            for y in 0..world.height() {
                let tile = world.tile(x, y, Layer::Foreground).unwrap();
                let touches_air = (-1..=1).any(|dx| {
                    (-1..=1).any(|dy| {
                        (dx != 0 || dy != 0)
                            && x.checked_add_signed(dx)
                                .zip(y.checked_add_signed(dy))
                                .and_then(|(sample_x, sample_y)| {
                                    world.tile(sample_x, sample_y, Layer::Foreground).ok()
                                })
                                == Some(ForegroundTile::AIR)
                    })
                });
                assert!(
                    tile != ForegroundTile::DIRT || !touches_air,
                    "exposed dirt at ({x}, {y}) should be grass"
                );
            }
        }
    }

    #[test]
    fn natural_objects_are_generated_and_spatially_indexed() {
        let world = WorldGenerator::new(0xD33F_7E57)
            .generate(1_024, 300)
            .unwrap();
        assert!(world.object_count() > 100);
        let mut grass = 0;
        let mut pebbles = 0;
        let mut grass_pebbles = 0;
        let mut vines = 0;
        assert!(world.objects().all(|object| {
            let anchor = object.anchor();
            let root = object.root();
            match object.object_type() {
                NaturalObject::GRASS => {
                    grass += 1;
                    assert_eq!(root, TilePos::new(anchor.x, anchor.y + 1));
                    assert_eq!(
                        world.tile(root.x, root.y, Layer::Foreground).unwrap(),
                        ForegroundTile::GRASS
                    );
                }
                NaturalObject::PEBBLE => {
                    pebbles += 1;
                    assert_eq!(root, TilePos::new(anchor.x, anchor.y + 1));
                    let root_tile = world.tile(root.x, root.y, Layer::Foreground).unwrap();
                    assert_ne!(root_tile, ForegroundTile::AIR);
                    grass_pebbles += usize::from(root_tile == ForegroundTile::GRASS);
                }
                NaturalObject::VINE => {
                    vines += 1;
                    assert_eq!(root, TilePos::new(anchor.x, anchor.y - 1));
                    assert_eq!(
                        world.tile(root.x, root.y, Layer::Foreground).unwrap(),
                        ForegroundTile::GRASS
                    );
                }
                _ => unreachable!("generator emitted an unknown natural object"),
            }
            world
                .objects_in_chunk(object.anchor().chunk())
                .any(|candidate| candidate.id() == object.id())
        }));
        assert!(grass > pebbles, "pebbles should be rarer than grass");
        assert!(pebbles > 0, "pebbles should appear on solid surfaces");
        assert!(grass_pebbles > 0, "some pebbles should use grass roots");
        assert!(vines > 0, "vines should seed beneath grass overhangs");
    }
}
