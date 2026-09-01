use crate::{
    FurnitureDefinition, FurnitureObject, Layer, ObjectId, TileId, TilePos, World,
    block_definition, furniture_definition,
};
use std::collections::{HashSet, VecDeque};

pub const MIN_HOUSE_INTERIOR_CELLS: usize = 12;
pub const MAX_HOUSE_INTERIOR_CELLS: usize = 1_024;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HouseRequirements {
    pub enclosed: bool,
    pub background_walls: bool,
    pub door: bool,
    pub light: bool,
    pub bed: bool,
    pub enough_space: bool,
}

impl HouseRequirements {
    pub const fn is_valid(self) -> bool {
        self.enclosed
            && self.background_walls
            && self.door
            && self.light
            && self.bed
            && self.enough_space
    }

    pub fn missing_labels(self) -> impl Iterator<Item = &'static str> {
        [
            (!self.enclosed).then_some("closed walls"),
            (!self.background_walls).then_some("background walls"),
            (!self.door).then_some("a door"),
            (!self.light).then_some("a light source"),
            (!self.bed).then_some("a bed"),
            (!self.enough_space).then_some("more interior space"),
        ]
        .into_iter()
        .flatten()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoomAssessment {
    terminal: ObjectId,
    requirements: HouseRequirements,
    interior_cells: Vec<TilePos>,
    standing_spots: Vec<[f32; 2]>,
    centre: [f32; 2],
}

impl RoomAssessment {
    pub const fn terminal(&self) -> ObjectId {
        self.terminal
    }

    pub const fn requirements(&self) -> HouseRequirements {
        self.requirements
    }

    pub const fn is_valid(&self) -> bool {
        self.requirements.is_valid()
    }

    pub fn interior_cells(&self) -> &[TilePos] {
        &self.interior_cells
    }

    pub fn standing_spots(&self) -> &[[f32; 2]] {
        &self.standing_spots
    }

    pub const fn centre(&self) -> [f32; 2] {
        self.centre
    }
}

pub fn assess_room(world: &World, terminal: ObjectId) -> Option<RoomAssessment> {
    let computer = world.object(terminal)?;
    if computer.object_type() != FurnitureObject::PROCUREMENT_TERMINAL {
        return None;
    }

    assess_room_from_anchor(world, terminal)
}

/// Uses the same complete housing assessment as specialists, while keeping
/// sleeping independent from the procurement-terminal ownership model.
pub fn assess_bed(world: &World, bed: ObjectId) -> Option<HouseRequirements> {
    let object = world.object(bed)?;
    if object.object_type() != FurnitureObject::BED {
        return None;
    }
    assess_room_from_anchor(world, bed).map(|assessment| assessment.requirements())
}

fn assess_room_from_anchor(world: &World, anchor: ObjectId) -> Option<RoomAssessment> {
    let seed = world.object(anchor)?.anchor();
    let mut frontier = VecDeque::from([seed]);
    let mut visited = HashSet::from([seed]);
    let mut interior = Vec::new();
    let mut doors = HashSet::new();
    let mut enclosed = true;
    let mut has_background_walls = true;
    let mut has_light = false;
    let mut has_bed = false;

    while let Some(position) = frontier.pop_front() {
        if interior.len() >= MAX_HOUSE_INTERIOR_CELLS {
            enclosed = false;
            break;
        }
        interior.push(position);
        has_bed |= world
            .object_at(position)
            .is_some_and(|object| object.object_type() == FurnitureObject::BED);
        has_background_walls &=
            world.tile_in_bounds(position.x, position.y, Layer::Background) != TileId::EMPTY;

        for neighbour in neighbours(position) {
            let Some(neighbour) = neighbour
                .filter(|position| position.x < world.width() && position.y < world.height())
            else {
                enclosed = false;
                continue;
            };
            let foreground = world.tile_in_bounds(neighbour.x, neighbour.y, Layer::Foreground);
            if foreground != TileId::EMPTY {
                has_light |= block_definition(foreground)
                    .is_some_and(|definition| definition.emitted_light().is_some());
                continue;
            }
            if let Some(object) = world.object_at(neighbour)
                && furniture_definition(object.object_type())
                    .is_some_and(FurnitureDefinition::is_room_boundary)
            {
                doors.insert(object.id());
                continue;
            }
            if visited.insert(neighbour) {
                frontier.push_back(neighbour);
            }
        }
    }

    let centre = if interior.is_empty() {
        [seed.x as f32, seed.y as f32]
    } else {
        let [sum_x, sum_y] = interior.iter().fold([0_u64; 2], |sum, position| {
            [
                sum[0] + u64::from(position.x),
                sum[1] + u64::from(position.y),
            ]
        });
        [
            sum_x as f32 / interior.len() as f32,
            sum_y as f32 / interior.len() as f32,
        ]
    };
    let standing_spots: Vec<[f32; 2]> = interior
        .iter()
        .copied()
        .filter_map(|position| standing_position(world, position))
        .collect();
    let requirements = HouseRequirements {
        enclosed,
        background_walls: has_background_walls,
        door: !doors.is_empty(),
        light: has_light,
        bed: has_bed,
        enough_space: interior.len() >= MIN_HOUSE_INTERIOR_CELLS && !standing_spots.is_empty(),
    };
    Some(RoomAssessment {
        terminal: anchor,
        requirements,
        interior_cells: interior,
        standing_spots,
        centre,
    })
}

fn neighbours(position: TilePos) -> [Option<TilePos>; 4] {
    [
        position
            .x
            .checked_sub(1)
            .map(|x| TilePos::new(x, position.y)),
        position
            .x
            .checked_add(1)
            .map(|x| TilePos::new(x, position.y)),
        position
            .y
            .checked_sub(1)
            .map(|y| TilePos::new(position.x, y)),
        position
            .y
            .checked_add(1)
            .map(|y| TilePos::new(position.x, y)),
    ]
}

fn standing_position(world: &World, position: TilePos) -> Option<[f32; 2]> {
    let support_y = position.y.checked_add(1)?;
    if support_y >= world.height()
        || world.tile_in_bounds(position.x, position.y, Layer::Foreground) != TileId::EMPTY
        || !world.is_solid_cell(TilePos::new(position.x, support_y))
    {
        return None;
    }
    let head_y = position.y.checked_sub(1)?;
    if world.tile_in_bounds(position.x, head_y, Layer::Foreground) != TileId::EMPTY
        || world.is_structural_furniture_at(TilePos::new(position.x, head_y))
    {
        return None;
    }
    Some([position.x as f32, position.y as f32 - 0.35])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BackgroundTile, ForegroundTile};

    fn furnished_house() -> (World, ObjectId) {
        let mut world = World::empty(16, 14, 1).unwrap();
        for y in 2..=10 {
            for x in 2..=11 {
                let boundary = x == 2 || x == 11 || y == 2 || y == 10;
                world
                    .set_tile(
                        x,
                        y,
                        Layer::Foreground,
                        if boundary {
                            ForegroundTile::STONE
                        } else {
                            TileId::EMPTY
                        },
                    )
                    .unwrap();
                world
                    .set_tile(x, y, Layer::Background, BackgroundTile::STONE_WALL)
                    .unwrap();
            }
        }
        for y in 7..=9 {
            world
                .set_tile(2, y, Layer::Foreground, TileId::EMPTY)
                .unwrap();
        }
        world
            .set_tile(7, 2, Layer::Foreground, TileId::new(4))
            .unwrap();
        world
            .place_furniture(FurnitureObject::DOOR, TilePos::new(2, 7))
            .unwrap();
        let terminal = world
            .place_furniture(FurnitureObject::PROCUREMENT_TERMINAL, TilePos::new(6, 8))
            .unwrap();
        world
            .place_furniture(FurnitureObject::BED, TilePos::new(8, 9))
            .unwrap();
        (world, terminal)
    }

    #[test]
    fn enclosed_furnished_house_is_valid() {
        let (world, terminal) = furnished_house();
        let room = assess_room(&world, terminal).unwrap();
        assert!(room.is_valid());
        assert!(!room.standing_spots().is_empty());
    }

    #[test]
    fn missing_door_and_background_are_reported_independently() {
        let (mut world, terminal) = furnished_house();
        world.remove_object_at(TilePos::new(2, 8)).unwrap();
        world
            .set_tile(5, 5, Layer::Background, TileId::EMPTY)
            .unwrap();
        let requirements = assess_room(&world, terminal).unwrap().requirements();
        assert!(!requirements.enclosed);
        assert!(!requirements.door);
        assert!(!requirements.background_walls);
    }

    #[test]
    fn open_door_remains_a_housing_boundary() {
        let (mut world, terminal) = furnished_house();
        let door = world.object_at(TilePos::new(2, 8)).unwrap().id();
        let bed = world.object_at(TilePos::new(8, 9)).unwrap().id();
        assert!(world.is_collision_cell(TilePos::new(2, 8)));
        assert!(world.toggle_door(door));
        assert!(!world.is_collision_cell(TilePos::new(2, 8)));

        let requirements = assess_room(&world, terminal).unwrap().requirements();
        assert!(requirements.is_valid());
        assert!(assess_bed(&world, bed).is_some_and(HouseRequirements::is_valid));
    }

    #[test]
    fn a_bed_is_required_and_can_assess_its_own_room() {
        let (mut world, terminal) = furnished_house();
        let bed = world.object_at(TilePos::new(8, 9)).unwrap().id();
        assert!(assess_bed(&world, bed).is_some_and(HouseRequirements::is_valid));

        world.remove_object(bed).unwrap();
        let requirements = assess_room(&world, terminal).unwrap().requirements();
        assert!(!requirements.bed);
        assert_eq!(requirements.missing_labels().collect::<Vec<_>>(), ["a bed"]);
    }
}
