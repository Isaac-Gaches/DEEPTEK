use super::{
    FurnitureObject, Layer, ObjectId, SUBSURFACE_SURVEY_DEPTH, SUBSURFACE_SURVEY_WIDTH, World,
    block_definition, furniture_definition, objects::TilePos,
};
use crate::ItemId;

pub const MAX_SURVEY_ORE_TYPES: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OreEstimate {
    pub item: ItemId,
    pub estimated_yield: u32,
    /// Signed elevation of the shallowest detected block, in decimetres.
    pub first_depth_decimetres: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurveyedArea {
    pub(crate) minimum: TilePos,
    pub(crate) maximum_exclusive: TilePos,
}

impl SurveyedArea {
    pub const fn contains(self, position: TilePos) -> bool {
        position.x >= self.minimum.x
            && position.x < self.maximum_exclusive.x
            && position.y >= self.minimum.y
            && position.y < self.maximum_exclusive.y
    }

    pub(crate) const fn intersects(self, minimum: TilePos, maximum_exclusive: TilePos) -> bool {
        self.minimum.x < maximum_exclusive.x
            && self.maximum_exclusive.x > minimum.x
            && self.minimum.y < maximum_exclusive.y
            && self.maximum_exclusive.y > minimum.y
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubsurfaceSurvey {
    pub width_tiles: u32,
    pub depth_tiles: u32,
    estimates: [Option<OreEstimate>; MAX_SURVEY_ORE_TYPES],
}

impl SubsurfaceSurvey {
    pub fn estimates(self) -> impl Iterator<Item = OreEstimate> {
        self.estimates.into_iter().flatten()
    }
}

impl World {
    /// Counts registered ore blocks in the bounded strip below a surveyor.
    /// The result is intentionally expressed as an estimate so richer ore
    /// grades can later adjust yields without changing the machine UI.
    pub fn subsurface_survey(&mut self, object: ObjectId) -> Option<SubsurfaceSurvey> {
        let object = self.object(object)?;
        if object.object_type() != FurnitureObject::SUBSURFACE_SURVEYOR {
            return None;
        }
        let [machine_width, machine_height] = furniture_definition(object.object_type())?.size();
        let centre_x = object.anchor().x + u32::from(machine_width.saturating_sub(1)) / 2;
        let width_tiles = SUBSURFACE_SURVEY_WIDTH.min(self.width());
        let first_x = centre_x
            .saturating_sub(width_tiles / 2)
            .min(self.width().saturating_sub(width_tiles));
        let first_y = object.anchor().y.saturating_add(u32::from(machine_height));
        let last_y = first_y
            .saturating_add(SUBSURFACE_SURVEY_DEPTH)
            .min(self.height());
        let depth_tiles = last_y.saturating_sub(first_y);
        let mut estimates: [Option<OreEstimate>; MAX_SURVEY_ORE_TYPES] =
            [None; MAX_SURVEY_ORE_TYPES];

        for y in first_y..last_y {
            for x in first_x..first_x + width_tiles {
                let tile = self.tile_in_bounds(x, y, Layer::Foreground);
                let Some(item) = block_definition(tile).and_then(|block| block.ore_yield()) else {
                    continue;
                };
                if let Some(estimate) = estimates
                    .iter_mut()
                    .flatten()
                    .find(|estimate| estimate.item == item)
                {
                    estimate.estimated_yield = estimate.estimated_yield.saturating_add(1);
                } else if let Some(slot) = estimates.iter_mut().find(|slot| slot.is_none()) {
                    *slot = Some(OreEstimate {
                        item,
                        estimated_yield: 1,
                        first_depth_decimetres: self.elevation_decimetres(y as f32),
                    });
                }
            }
        }

        let surveyed_area = SurveyedArea {
            minimum: TilePos::new(first_x, first_y),
            maximum_exclusive: TilePos::new(first_x + width_tiles, last_y),
        };
        if !self.surveyed_areas.contains(&surveyed_area) {
            self.surveyed_areas.push(surveyed_area);
            self.survey_revision = self.survey_revision.wrapping_add(1);
        }

        Some(SubsurfaceSurvey {
            width_tiles,
            depth_tiles,
            estimates,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ForegroundTile, TilePos};

    #[test]
    fn survey_counts_only_registered_ore_below_its_footprint() {
        let mut world = World::empty(80, 80, 1).unwrap();
        for x in 0..80 {
            world
                .set_tile(x, 20, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        let scanner = world
            .place_furniture(
                FurnitureObject::SUBSURFACE_SURVEYOR,
                TilePos { x: 38, y: 18 },
            )
            .unwrap();
        for position in [TilePos { x: 10, y: 30 }, TilePos { x: 60, y: 50 }] {
            world
                .set_tile(
                    position.x,
                    position.y,
                    Layer::Foreground,
                    ForegroundTile::IRON_ORE,
                )
                .unwrap();
        }
        world
            .set_tile(5, 17, Layer::Foreground, ForegroundTile::IRON_ORE)
            .unwrap();

        let survey = world.subsurface_survey(scanner).unwrap();
        assert_eq!(survey.width_tiles, 64);
        assert_eq!(survey.depth_tiles, 60);
        assert_eq!(
            survey.estimates().collect::<Vec<_>>(),
            vec![OreEstimate {
                item: ItemId::IRON_ORE,
                estimated_yield: 2,
                first_depth_decimetres: world.elevation_decimetres(30.0),
            }]
        );
        assert!(world.is_surveyed(TilePos::new(10, 30)));
        assert!(!world.is_surveyed(TilePos::new(5, 17)));
    }
}
