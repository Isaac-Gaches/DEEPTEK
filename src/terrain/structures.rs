//! Small, deterministic surface structures stamped after terrain generation.
//!
//! Designs intentionally live in data-only blueprints. Replacing the landing
//! pod or adding outpost variants should only require changing these rows and
//! furniture markers, not the terrain generator.

use super::{
    BackgroundTile, ForegroundTile, FurnitureObject, Layer, ObjectTypeId, TileId, TilePos, World,
    WorldError,
};

const OUTPOST_SPACING: u32 = 360;
const OUTPOST_JITTER: i32 = 72;
const LANDING_CLEARANCE: u32 = 80;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BlueprintFurniture {
    pub(crate) object_type: ObjectTypeId,
    pub(crate) offset: [u16; 2],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StructureBlueprint {
    pub(crate) name: &'static str,
    pub(crate) rows: &'static [&'static str],
    pub(crate) furniture: &'static [BlueprintFurniture],
    /// Tile containing the player's feet. The actual saved position is moved
    /// upward by half the player collider when this marker is present.
    pub(crate) spawn: Option<[u16; 2]>,
}

impl StructureBlueprint {
    pub(crate) fn width(self) -> u32 {
        self.rows.first().map_or(0, |row| row.len() as u32)
    }

    pub(crate) fn height(self) -> u32 {
        self.rows.len() as u32
    }
}

const LANDING_FURNITURE: &[BlueprintFurniture] = &[
    BlueprintFurniture {
        object_type: FurnitureObject::DOOR,
        offset: [0, 3],
    },
    BlueprintFurniture {
        object_type: FurnitureObject::BED,
        offset: [3, 5],
    },
];

/// `#` is solid stone, `l` is a red light, and every other cell is air.
/// The complete footprint receives stone background wall.
pub(crate) const LANDING_POD_BLUEPRINT: StructureBlueprint = StructureBlueprint {
    name: "landing pod",
    rows: &[
        "##############",
        "#wwwwwwwwwwww#",
        "#wlwwwwwwwwww#",
        "wwwwwwwwwwwww#",
        "wwwwwwwwwwwww#",
        "wwwwwwwwwwwww#",
        "##############",
    ],
    furniture: LANDING_FURNITURE,
    spawn: Some([7, 6]),
};

const OUTPOST_FURNITURE: &[BlueprintFurniture] = &[BlueprintFurniture {
    object_type: FurnitureObject::DOOR,
    offset: [0, 2],
}];

pub(crate) const SURFACE_OUTPOST_BLUEPRINT: StructureBlueprint = StructureBlueprint {
    name: "surface outpost",
    rows: &[
        "##########",
        "#wwwwwwww#",
        "wwlwwwwww#",
        "wwwwwwwww#",
        "wwwwwwwww#",
        "##########",
    ],
    furniture: OUTPOST_FURNITURE,
    spawn: None,
};

pub(crate) fn populate_generated_structures(
    world: &mut World,
    surfaces: &[u32],
) -> Result<(), WorldError> {
    if surfaces.len() != world.width() as usize {
        return Err(WorldError::InvalidData(
            "structure surface profile does not match world width".into(),
        ));
    }
    validate_blueprint(LANDING_POD_BLUEPRINT)?;
    validate_blueprint(SURFACE_OUTPOST_BLUEPRINT)?;

    let landing_centre = world.width() / 2;
    if let Some(origin) = surface_origin(world, surfaces, LANDING_POD_BLUEPRINT, landing_centre) {
        stamp(world, LANDING_POD_BLUEPRINT, origin)?;
        if let Some([spawn_x, spawn_y]) = LANDING_POD_BLUEPRINT.spawn {
            let feet_y = origin.y + u32::from(spawn_y);
            world.set_player_position(Some([
                origin.x as f32 + f32::from(spawn_x),
                feet_y as f32 - 1.85,
            ]))?;
        }
    }

    let mut nominal_centre = OUTPOST_SPACING / 2;
    while nominal_centre < world.width() {
        let jitter = deterministic_jitter(world.seed(), nominal_centre);
        let candidate = nominal_centre
            .checked_add_signed(jitter)
            .unwrap_or(nominal_centre)
            .min(world.width().saturating_sub(1));
        if candidate.abs_diff(landing_centre) > LANDING_CLEARANCE
            && let Some(origin) =
                surface_origin(world, surfaces, SURFACE_OUTPOST_BLUEPRINT, candidate)
        {
            stamp(world, SURFACE_OUTPOST_BLUEPRINT, origin)?;
        }
        nominal_centre = nominal_centre.saturating_add(OUTPOST_SPACING);
    }
    Ok(())
}

fn validate_blueprint(blueprint: StructureBlueprint) -> Result<(), WorldError> {
    let width = blueprint.width();
    if width == 0
        || blueprint.height() == 0
        || blueprint
            .rows
            .iter()
            .any(|row| row.len() as u32 != width || !row.is_ascii())
    {
        return Err(WorldError::InvalidData(format!(
            "{} blueprint has inconsistent rows",
            blueprint.name
        )));
    }
    Ok(())
}

fn surface_origin(
    world: &World,
    surfaces: &[u32],
    blueprint: StructureBlueprint,
    centre_x: u32,
) -> Option<TilePos> {
    let width = blueprint.width();
    let height = blueprint.height();
    if world.width() < width + 4 || world.height() < height + 2 {
        return None;
    }
    let left = centre_x
        .saturating_sub(width / 2)
        .clamp(2, world.width() - width - 2);
    let floor_y = surfaces[(left + width / 2) as usize];
    let top = floor_y.checked_sub(height - 1)?;
    (top + height < world.height()).then_some(TilePos::new(left, top))
}

fn stamp(
    world: &mut World,
    blueprint: StructureBlueprint,
    origin: TilePos,
) -> Result<(), WorldError> {
    let width = blueprint.width();
    let height = blueprint.height();
    let floor_y = origin.y + height - 1;

    // Clear a narrow apron so the entrance remains usable on uneven terrain.
    let apron_left = origin.x.saturating_sub(2);
    let apron_right = (origin.x + width + 1).min(world.width() - 1);
    for x in apron_left..=apron_right {
        for y in origin.y..floor_y {
            world.remove_object_at(TilePos::new(x, y));
            world.set_tile(x, y, Layer::Foreground, TileId::EMPTY)?;
        }
    }

    for (row, cells) in blueprint.rows.iter().enumerate() {
        for (column, cell) in cells.bytes().enumerate() {
            let x = origin.x + column as u32;
            let y = origin.y + row as u32;
            world.remove_object_at(TilePos::new(x, y));
            world.set_tile(x, y, Layer::Background, BackgroundTile::STONE_WALL)?;
            let foreground = match cell {
                b'#' => ForegroundTile::STONE,
                b'l' => TileId::new(4),
                _ => TileId::EMPTY,
            };
            world.set_tile(x, y, Layer::Foreground, foreground)?;
        }
    }

    for furniture in blueprint.furniture {
        let anchor = TilePos::new(
            origin.x + u32::from(furniture.offset[0]),
            origin.y + u32::from(furniture.offset[1]),
        );
        world
            .place_furniture(furniture.object_type, anchor)
            .map_err(|error| {
                WorldError::InvalidData(format!(
                    "could not place {} furniture: {error}",
                    blueprint.name
                ))
            })?;
    }

    repair_exposed_soil(world, apron_left, apron_right, origin.y, floor_y);
    Ok(())
}

fn repair_exposed_soil(world: &mut World, left: u32, right: u32, top: u32, bottom: u32) {
    let scan_left = left.saturating_sub(1);
    let scan_right = (right + 1).min(world.width() - 1);
    let scan_top = top.saturating_sub(1);
    let scan_bottom = (bottom + 1).min(world.height() - 1);
    let mut exposed = Vec::new();
    for x in scan_left..=scan_right {
        for y in scan_top..=scan_bottom {
            if world.tile_in_bounds(x, y, Layer::Foreground) != ForegroundTile::DIRT {
                continue;
            }
            let touches_air = (-1..=1).any(|dx| {
                (-1..=1).any(|dy| {
                    (dx != 0 || dy != 0)
                        && x.checked_add_signed(dx)
                            .zip(y.checked_add_signed(dy))
                            .filter(|&(sample_x, sample_y)| {
                                sample_x < world.width() && sample_y < world.height()
                            })
                            .is_some_and(|(sample_x, sample_y)| {
                                world.tile_in_bounds(sample_x, sample_y, Layer::Foreground)
                                    == TileId::EMPTY
                            })
                })
            });
            if touches_air {
                exposed.push(TilePos::new(x, y));
            }
        }
    }
    for position in exposed {
        world
            .set_tile(
                position.x,
                position.y,
                Layer::Foreground,
                ForegroundTile::GRASS,
            )
            .expect("soil repair stays within the validated world bounds");
    }
}

fn deterministic_jitter(seed: u64, position: u32) -> i32 {
    let mut value = seed ^ u64::from(position).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    let span = (OUTPOST_JITTER * 2 + 1) as u64;
    (value % span) as i32 - OUTPOST_JITTER
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HouseRequirements, assess_bed};

    #[test]
    fn built_in_blueprints_have_consistent_dimensions() {
        validate_blueprint(LANDING_POD_BLUEPRINT).unwrap();
        validate_blueprint(SURFACE_OUTPOST_BLUEPRINT).unwrap();
    }

    #[test]
    fn generated_landing_pod_is_a_valid_house_and_sets_spawn() {
        let world = crate::WorldGenerator::new(91).generate(600, 200).unwrap();
        let bed = world
            .objects_of_type(FurnitureObject::BED)
            .next()
            .expect("landing pod bed is generated");
        assert!(assess_bed(&world, bed.id()).is_some_and(HouseRequirements::is_valid));
        assert_eq!(
            world
                .objects_of_type(FurnitureObject::PROCUREMENT_TERMINAL)
                .count(),
            0
        );
        assert!(world.player_position().is_some());
        assert!(world.objects_of_type(FurnitureObject::DOOR).count() >= 2);
    }
}
