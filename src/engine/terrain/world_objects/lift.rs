use super::super::{
    CARGO_LIFT_SPEED_MILLI_TILES_PER_SECOND, CargoLiftDirection, FurnitureBehavior,
    FurnitureObject, Layer, LiftStationConfiguration, LiftStationMode, ObjectId,
    ObjectPlacementError, ObjectTypeId, POWERED_CABLE_OBJECT, TileId, TilePos, World, WorldObject,
    furniture_definition,
};
use crate::PowerSystem;
use crate::items::{ItemContainer, ItemRegistry};
use std::time::Duration;

const LIFT_TRANSFER_INTERVAL: Duration = Duration::from_millis(50);
const MAX_LIFT_TRANSFER_TICKS: usize = 20;

impl World {
    pub fn cargo_lift_placement_target(
        &self,
        target: TilePos,
    ) -> Result<TilePos, ObjectPlacementError> {
        self.cargo_lift_placement_target_for(FurnitureObject::CARGO_LIFT, target)
    }

    pub fn cargo_lift_placement_target_for(
        &self,
        object_type: ObjectTypeId,
        target: TilePos,
    ) -> Result<TilePos, ObjectPlacementError> {
        self.cargo_lift_placement(object_type, target)
            .map(|(_, anchor)| anchor)
    }

    pub fn place_cargo_lift(&mut self, target: TilePos) -> Result<ObjectId, ObjectPlacementError> {
        self.place_cargo_lift_type(FurnitureObject::CARGO_LIFT, target)
    }

    pub fn place_cargo_lift_type(
        &mut self,
        object_type: ObjectTypeId,
        target: TilePos,
    ) -> Result<ObjectId, ObjectPlacementError> {
        let definition = furniture_definition(object_type)
            .filter(|definition| definition.behavior() == FurnitureBehavior::CargoLift)
            .ok_or(ObjectPlacementError::UnsupportedType(object_type))?;
        let (cable, anchor) = self.cargo_lift_placement(object_type, target)?;
        let [width, height] = definition.size();
        for y in anchor.y..anchor.y + u32::from(height) {
            for x in anchor.x..anchor.x + u32::from(width) {
                let position = TilePos::new(x, y);
                if self
                    .object_at(position)
                    .is_some_and(|object| super::is_natural_object(object.object_type()))
                {
                    self.objects.remove_occupying(position);
                }
            }
        }
        let id = self.objects.allocate_id();
        self.objects.insert(WorldObject {
            id,
            object_type,
            anchor,
            root: anchor,
            width,
            height,
            variant: CargoLiftDirection::Idle as u8,
            growth_stage: 0,
            active: true,
            health: definition.maximum_health().unwrap_or(0),
            stored_energy_milli: 0,
            machine_target_y: u32::MAX,
            kill_count: 0,
            linked_object: cable.raw(),
            motion_position_milli: anchor.y.saturating_mul(1_000),
            next_update_tick: u64::MAX,
        })?;
        self.objects.containers.insert(
            id,
            ItemContainer::new(usize::from(
                definition.interaction().container_slots().unwrap_or(0),
            )),
        );
        Ok(id)
    }

    pub fn lift_station_placement_target(
        &self,
        anchor: TilePos,
    ) -> Result<TilePos, ObjectPlacementError> {
        self.can_place_furniture(FurnitureObject::LIFT_STATION, anchor)?;
        Ok(anchor)
    }

    pub fn place_lift_station(
        &mut self,
        anchor: TilePos,
    ) -> Result<ObjectId, ObjectPlacementError> {
        self.place_furniture(FurnitureObject::LIFT_STATION, anchor)
    }

    pub fn lift_station_configuration(
        &self,
        station: ObjectId,
    ) -> Option<LiftStationConfiguration> {
        let station = self.objects.object(station)?;
        (station.object_type == FurnitureObject::LIFT_STATION)
            .then(|| LiftStationConfiguration::from_raw(station.variant))
            .flatten()
    }

    pub fn set_lift_station_mode(&mut self, station: ObjectId, mode: LiftStationMode) -> bool {
        let Some(configuration) = self.lift_station_configuration(station) else {
            return false;
        };
        self.set_lift_station_configuration(station, configuration.with_mode(mode))
    }

    pub fn set_lift_station_departure(
        &mut self,
        station: ObjectId,
        departure: CargoLiftDirection,
    ) -> bool {
        let Some(configuration) = self.lift_station_configuration(station) else {
            return false;
        };
        let Some(configuration) = configuration.with_departure(departure) else {
            return false;
        };
        self.set_lift_station_configuration(station, configuration)
    }

    fn set_lift_station_configuration(
        &mut self,
        station: ObjectId,
        configuration: LiftStationConfiguration,
    ) -> bool {
        let Some(object) = self.objects.object(station) else {
            return false;
        };
        if object.object_type != FurnitureObject::LIFT_STATION
            || object.variant == configuration.raw()
        {
            return false;
        }
        self.objects.object_mut(station).unwrap().variant = configuration.raw();
        self.objects.invalidate_station_dock(station);
        self.objects.mark_changed();
        true
    }

    pub fn cargo_lift_direction(&self, lift: ObjectId) -> Option<CargoLiftDirection> {
        let lift = self.objects.object(lift)?;
        is_cargo_lift_type(lift.object_type)
            .then(|| CargoLiftDirection::from_raw(lift.variant))
            .flatten()
    }

    pub fn cargo_lift_cable(&self, lift: ObjectId) -> Option<ObjectId> {
        let lift = self.objects.object(lift)?;
        is_cargo_lift_type(lift.object_type)
            .then(|| lift.linked_object())
            .flatten()
    }

    pub fn set_cargo_lift_direction(
        &mut self,
        lift: ObjectId,
        direction: CargoLiftDirection,
    ) -> bool {
        let Some(snapshot) = self.objects.object(lift) else {
            return false;
        };
        if !is_cargo_lift_type(snapshot.object_type) || !snapshot.is_active() {
            return false;
        }
        let Some(cable) = snapshot
            .linked_object()
            .and_then(|id| self.objects.object(id))
        else {
            return false;
        };
        let Some((minimum, maximum)) = cargo_lift_bounds(cable, snapshot.height) else {
            return false;
        };
        let current = snapshot.motion_position_milli;
        let requested = match direction {
            CargoLiftDirection::Up if current <= minimum.saturating_mul(1_000) => {
                CargoLiftDirection::Idle
            }
            CargoLiftDirection::Down if current >= maximum.saturating_mul(1_000) => {
                CargoLiftDirection::Idle
            }
            direction => direction,
        };
        if snapshot.variant == requested as u8 {
            return false;
        }
        self.objects.object_mut(lift).unwrap().variant = requested as u8;
        self.objects.mark_changed();
        true
    }

    /// Advances powered lifts in fixed-point tile units. The ordered station
    /// index resolves the nearest stop without scanning world furniture. A
    /// command remains pending through an outage, and movement is always
    /// clamped to the next station or cable endpoint.
    pub fn update_cargo_lifts(
        &mut self,
        elapsed: Duration,
        power: &PowerSystem,
        registry: &ItemRegistry,
    ) -> usize {
        self.update_cargo_lifts_with_speed(elapsed, power, registry, 100)
    }

    pub fn update_cargo_lifts_with_speed(
        &mut self,
        elapsed: Duration,
        power: &PowerSystem,
        registry: &ItemRegistry,
        speed_percent: u16,
    ) -> usize {
        let distance = cargo_lift_distance(elapsed, speed_percent);
        let lifts: Vec<_> = super::super::BUILT_IN_FURNITURE
            .iter()
            .filter(|definition| definition.behavior() == FurnitureBehavior::CargoLift)
            .flat_map(|definition| {
                self.objects
                    .ids_of_type(definition.object_type())
                    .iter()
                    .copied()
            })
            .collect();
        let mut moved = 0;
        for lift_id in lifts {
            let Some(lift) = self.objects.object(lift_id).cloned() else {
                continue;
            };
            if !lift.is_active() {
                continue;
            }
            let Some(direction) = CargoLiftDirection::from_raw(lift.variant) else {
                continue;
            };
            let Some(cable_id) = lift.linked_object() else {
                continue;
            };
            let Some(bounds) = self
                .objects
                .object(cable_id)
                .and_then(|cable| cargo_lift_bounds(cable, lift.height))
            else {
                let _ = self.set_cargo_lift_direction(lift_id, CargoLiftDirection::Idle);
                continue;
            };

            let current = lift.motion_position_milli;
            if direction == CargoLiftDirection::Idle {
                if current.is_multiple_of(1_000) {
                    let height = current / 1_000;
                    if let Some(station) = self.objects.lift_station_at(cable_id, height) {
                        self.objects.set_docked_station(lift_id, station);
                        let ticks = self.objects.lift_transfer_ticks(
                            lift_id,
                            elapsed,
                            LIFT_TRANSFER_INTERVAL,
                            MAX_LIFT_TRANSFER_TICKS,
                        );
                        for _ in 0..ticks {
                            if !self.service_lift_station_once(lift_id, station, registry) {
                                if let Some(configuration) =
                                    self.lift_station_configuration(station)
                                    && !self.empty_lift_waits_at_load_station(
                                        lift_id,
                                        station,
                                        configuration,
                                    )
                                {
                                    let _ = self.set_cargo_lift_direction(
                                        lift_id,
                                        configuration.departure(),
                                    );
                                }
                                break;
                            }
                        }
                    } else {
                        self.objects.clear_docked_station(lift_id);
                    }
                }
                continue;
            }

            let station = self.objects.next_lift_station(
                cable_id,
                current,
                direction == CargoLiftDirection::Up,
            );
            let station_height = station
                .and_then(|station| self.objects.object(station))
                .map(|station| station.anchor.y);
            let target = station_height.map_or_else(
                || match direction {
                    CargoLiftDirection::Up => bounds.0.saturating_mul(1_000),
                    CargoLiftDirection::Down => bounds.1.saturating_mul(1_000),
                    CargoLiftDirection::Idle => unreachable!(),
                },
                |height| height.saturating_mul(1_000),
            );
            if current == target {
                let _ = self.set_cargo_lift_direction(lift_id, CargoLiftDirection::Idle);
                if let Some(station) = station {
                    self.objects.set_docked_station(lift_id, station);
                }
                continue;
            }
            if distance == 0 || !power.is_powered(lift_id) {
                continue;
            }
            let next = match direction {
                CargoLiftDirection::Up => current.saturating_sub(distance).max(target),
                CargoLiftDirection::Down => current.saturating_add(distance).min(target),
                CargoLiftDirection::Idle => unreachable!(),
            };
            let next_anchor_y = next.saturating_add(500) / 1_000;
            let anchor_x = lift.anchor.x;
            let old_anchor_y = lift.anchor.y;
            if next_anchor_y != old_anchor_y {
                let next_anchor = TilePos::new(anchor_x, next_anchor_y);
                if !self.lift_cells_clear(next_anchor, lift.size(), Some(lift_id), false)
                    || !self.objects.relocate_free(lift_id, next_anchor)
                {
                    let _ = self.set_cargo_lift_direction(lift_id, CargoLiftDirection::Idle);
                    continue;
                }
            }
            let lift = self.objects.object_mut(lift_id).unwrap();
            lift.motion_position_milli = next;
            if next == target {
                lift.variant = CargoLiftDirection::Idle as u8;
            }
            self.objects.clear_docked_station(lift_id);
            self.objects.mark_changed();
            moved += 1;
            if next == target
                && let Some(station) = station
            {
                self.objects.set_docked_station(lift_id, station);
            }
        }
        moved
    }

    fn service_lift_station_once(
        &mut self,
        lift: ObjectId,
        station: ObjectId,
        registry: &ItemRegistry,
    ) -> bool {
        if self
            .objects
            .object(station)
            .is_none_or(|station| !station.is_active())
        {
            return false;
        }
        let Some(configuration) = self.lift_station_configuration(station) else {
            return false;
        };
        let (source, destination) = match configuration.mode() {
            LiftStationMode::Load => (station, lift),
            LiftStationMode::Unload => (lift, station),
        };
        self.transfer_one_container_item(source, destination, registry)
    }

    fn empty_lift_waits_at_load_station(
        &self,
        lift: ObjectId,
        station: ObjectId,
        configuration: LiftStationConfiguration,
    ) -> bool {
        configuration.mode() == LiftStationMode::Load
            && self.container(lift).is_some_and(ItemContainer::is_empty)
            && self.container(station).is_some_and(ItemContainer::is_empty)
    }

    fn cargo_lift_placement(
        &self,
        object_type: ObjectTypeId,
        target: TilePos,
    ) -> Result<(ObjectId, TilePos), ObjectPlacementError> {
        let size = furniture_definition(object_type)
            .filter(|definition| definition.behavior() == FurnitureBehavior::CargoLift)
            .map(|definition| definition.size())
            .ok_or(ObjectPlacementError::UnsupportedType(object_type))?;
        if let Some(cable) = self
            .object_at(target)
            .filter(|object| object.object_type == POWERED_CABLE_OBJECT)
        {
            return self.cargo_lift_placement_from_cable(cable, target.y, size);
        }

        self.adjacent_cables(target, size[0])
            .into_iter()
            .flatten()
            .find(|cable| self.cargo_lift_anchor_is_valid(cable, target, size))
            .map(|cable| (cable.id, target))
            .ok_or(ObjectPlacementError::MissingPoweredCableAttachment(target))
    }

    fn cargo_lift_placement_from_cable(
        &self,
        cable: &WorldObject,
        target_y: u32,
        size: [u16; 2],
    ) -> Result<(ObjectId, TilePos), ObjectPlacementError> {
        let Some((minimum, maximum)) = cargo_lift_bounds(cable, size[1]) else {
            return Err(ObjectPlacementError::MissingPoweredCableAttachment(
                cable.anchor,
            ));
        };
        if self.cable_has_lift(cable.id) {
            return Err(ObjectPlacementError::MissingPoweredCableAttachment(
                TilePos::new(cable.anchor.x, target_y),
            ));
        }
        let top_y = target_y.clamp(minimum, maximum);
        let right = cable
            .anchor
            .x
            .checked_add(1)
            .map(|x| TilePos::new(x, top_y))
            .filter(|&anchor| self.cargo_lift_anchor_is_valid(cable, anchor, size));
        let left = cable
            .anchor
            .x
            .checked_sub(u32::from(size[0]))
            .map(|x| TilePos::new(x, top_y))
            .filter(|&anchor| self.cargo_lift_anchor_is_valid(cable, anchor, size));
        right
            .or(left)
            .map(|anchor| (cable.id, anchor))
            .ok_or(ObjectPlacementError::Occupied(cable.anchor))
    }

    fn cargo_lift_anchor_is_valid(
        &self,
        cable: &WorldObject,
        anchor: TilePos,
        size: [u16; 2],
    ) -> bool {
        let Some((minimum, maximum)) = cargo_lift_bounds(cable, size[1]) else {
            return false;
        };
        let beside_cable = anchor.x == cable.anchor.x.saturating_add(1)
            || anchor.x.checked_add(u32::from(size[0])) == Some(cable.anchor.x);
        beside_cable
            && (minimum..=maximum).contains(&anchor.y)
            && !self.cable_has_lift(cable.id)
            && self.lift_cells_clear(anchor, size, None, true)
    }

    fn cable_has_lift(&self, cable: ObjectId) -> bool {
        self.objects.cargo_lift_for_cable(cable).is_some()
    }

    pub(super) fn adjacent_station_cable(&self, anchor: TilePos) -> Option<ObjectId> {
        self.adjacent_cables(anchor, 2)
            .into_iter()
            .flatten()
            .find(|cable| {
                let lift_height = self
                    .objects
                    .cargo_lift_for_cable(cable.id)
                    .and_then(|id| self.objects.object(id))
                    .map_or(2, |lift| lift.height);
                let Some((minimum, maximum)) = cargo_lift_bounds(cable, lift_height) else {
                    return false;
                };
                let station_is_right = anchor.x > cable.anchor.x;
                (minimum..=maximum).contains(&anchor.y)
                    && self.objects.lift_station_at(cable.id, anchor.y).is_none()
                    && self
                        .objects
                        .cargo_lift_for_cable(cable.id)
                        .and_then(|lift| self.objects.object(lift))
                        .is_none_or(|lift| (lift.anchor.x > cable.anchor.x) != station_is_right)
            })
            .map(WorldObject::id)
    }

    /// Resolves the only two possible cable columns for a 2x2 lift-family
    /// footprint. This keeps placement constant-time regardless of world size.
    fn adjacent_cables(&self, anchor: TilePos, width: u16) -> [Option<&WorldObject>; 2] {
        let left = anchor.x.checked_sub(1).and_then(|x| {
            self.object_at(TilePos::new(x, anchor.y))
                .filter(|object| object.object_type == POWERED_CABLE_OBJECT)
        });
        let right = anchor.x.checked_add(u32::from(width)).and_then(|x| {
            (x < self.width)
                .then(|| self.object_at(TilePos::new(x, anchor.y)))
                .flatten()
                .filter(|object| object.object_type == POWERED_CABLE_OBJECT)
        });
        [left, right]
    }

    /// Checks the shared 2x2 footprint used by stations and moving lifts. The
    /// moving lift may overlap only its own indexed cells while it relocates.
    fn lift_cells_clear(
        &self,
        anchor: TilePos,
        size: [u16; 2],
        ignored: Option<ObjectId>,
        natural_objects_are_clearable: bool,
    ) -> bool {
        let (Some(end_x), Some(end_y)) = (
            anchor.x.checked_add(u32::from(size[0]).saturating_sub(1)),
            anchor.y.checked_add(u32::from(size[1]).saturating_sub(1)),
        ) else {
            return false;
        };
        if end_x >= self.width || end_y >= self.height {
            return false;
        }
        (anchor.y..=end_y).all(|y| {
            (anchor.x..=end_x).all(|x| {
                self.tile_in_bounds(x, y, Layer::Foreground) == TileId::EMPTY
                    && self
                        .objects
                        .occupying(TilePos::new(x, y))
                        .is_none_or(|occupant| {
                            Some(occupant) == ignored
                                || natural_objects_are_clearable
                                    && self.objects.object(occupant).is_some_and(|object| {
                                        super::is_natural_object(object.object_type())
                                    })
                        })
            })
        })
    }
}

fn cargo_lift_bounds(cable: &WorldObject, lift_height: u16) -> Option<(u32, u32)> {
    (cable.object_type == POWERED_CABLE_OBJECT && cable.height >= lift_height).then(|| {
        (
            cable.anchor.y,
            cable.anchor.y + u32::from(cable.height - lift_height),
        )
    })
}

fn is_cargo_lift_type(object_type: ObjectTypeId) -> bool {
    furniture_definition(object_type)
        .is_some_and(|definition| definition.behavior() == FurnitureBehavior::CargoLift)
}

fn cargo_lift_distance(elapsed: Duration, speed_percent: u16) -> u32 {
    (u128::from(CARGO_LIFT_SPEED_MILLI_TILES_PER_SECOND)
        * u128::from(speed_percent.max(1))
        * elapsed.as_nanos()
        / 100
        / 1_000_000_000)
        .min(u128::from(u32::MAX)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logistics_bonus_scales_lift_distance() {
        let elapsed = Duration::from_millis(100);
        assert_eq!(cargo_lift_distance(elapsed, 100), 600);
        assert_eq!(cargo_lift_distance(elapsed, 125), 750);
    }
}
