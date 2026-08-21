use super::{
    ChunkPos, FurnitureConfiguration, FurnitureInteraction, FurnitureObject, FurnitureSupport,
    LASER_BORE_MAX_LENGTH, LaserBoreBeam, Layer, LiftStationConfiguration, ObjectId,
    ObjectPlacementError, ObjectTypeId, POWERED_CABLE_OBJECT, ROPE_OBJECT, TargetPriority, TileId,
    TilePos, World, WorldObject, decoration_definition, furniture_definition,
};
use crate::items::ItemContainer;

mod lift;

impl World {
    pub(crate) const fn object_revision(&self) -> u64 {
        self.objects.revision()
    }

    pub(crate) const fn item_transport_revision(&self) -> u64 {
        self.objects.item_transport_revision()
    }

    pub(crate) const fn power_revision(&self) -> u64 {
        self.objects.power_revision()
    }

    pub fn object_count(&self) -> usize {
        self.objects.objects.len()
    }

    pub fn object(&self, id: ObjectId) -> Option<&WorldObject> {
        self.objects.object(id)
    }

    pub fn objects(&self) -> impl ExactSizeIterator<Item = &WorldObject> {
        self.objects.objects.iter()
    }

    pub fn objects_in_chunk(&self, position: ChunkPos) -> impl Iterator<Item = &WorldObject> {
        self.objects
            .ids_in_chunk(position)
            .iter()
            .filter_map(|&id| self.objects.object(id))
    }

    /// Iterates only objects registered under one stable type ID. Systems for
    /// rare furniture avoid scanning potentially large decoration populations.
    pub fn objects_of_type(&self, object_type: ObjectTypeId) -> impl Iterator<Item = &WorldObject> {
        self.objects
            .ids_of_type(object_type)
            .iter()
            .filter_map(|&id| self.objects.object(id))
    }

    pub fn object_at(&self, position: TilePos) -> Option<&WorldObject> {
        self.objects
            .occupying(position)
            .and_then(|id| self.objects.object(id))
    }

    pub fn furniture_at(&self, position: TilePos) -> Option<&WorldObject> {
        self.object_at(position)
            .filter(|object| furniture_definition(object.object_type).is_some())
    }

    /// Player block placement is supported only by an orthogonally adjacent
    /// tile in the same layer. Furniture and decorations deliberately do not
    /// count as structural tile support.
    pub fn can_place_tile_adjacent(&self, position: TilePos, layer: Layer) -> bool {
        position.x < self.width
            && position.y < self.height
            && self.tile_in_bounds(position.x, position.y, layer) == TileId::EMPTY
            && (layer != Layer::Foreground || !self.blocks_foreground_tile_placement(position))
            && ((layer == Layer::Foreground
                && self.tile_in_bounds(position.x, position.y, Layer::Background) != TileId::EMPTY)
                || orthogonal_neighbours(position, self.width, self.height).any(|neighbour| {
                    self.tile_in_bounds(neighbour.x, neighbour.y, layer) != TileId::EMPTY
                }))
    }

    pub fn furniture_interaction_at(
        &self,
        position: TilePos,
    ) -> Option<(ObjectId, FurnitureInteraction)> {
        let object = self.furniture_at(position)?;
        let definition = furniture_definition(object.object_type)?;
        Some((object.id, definition.interaction()))
    }

    /// Orbital launchers must sit wholly above the world's zero-elevation
    /// level and have an unobstructed vertical corridor across their roof.
    pub fn orbital_export_has_sky_access(&self, launcher: ObjectId) -> bool {
        let Some(object) = self.objects.object(launcher) else {
            return false;
        };
        if object.object_type != FurnitureObject::ORBITAL_EXPORT_LAUNCHER {
            return false;
        }
        let bottom = object
            .anchor
            .y
            .saturating_add(u32::from(object.height).saturating_sub(1));
        if bottom >= self.sea_level_y() {
            return false;
        }
        (0..u32::from(object.width)).all(|offset_x| {
            let x = object.anchor.x + offset_x;
            (0..object.anchor.y)
                .all(|y| self.tile_in_bounds(x, y, Layer::Foreground) == TileId::EMPTY)
        })
    }

    /// Starts or stops furniture that declares an activation control. Returns
    /// `false` for unknown, non-activatable, or already-matching objects.
    pub fn set_furniture_active(&mut self, id: ObjectId, active: bool) -> bool {
        let Some(object) = self.objects.object(id) else {
            return false;
        };
        let Some(definition) = furniture_definition(object.object_type) else {
            return false;
        };
        if !definition.interaction().is_activatable() || object.active == active {
            return false;
        }
        let is_laser_bore = object.object_type == FurnitureObject::LASER_BORE;
        if let Some(object) = self.objects.object_mut(id) {
            object.active = active;
            if is_laser_bore {
                object.variant = u8::MAX;
                object.growth_stage = 0;
                object.machine_target_y = u32::MAX;
            }
        }
        let next_tick = if active && is_laser_bore {
            self.simulation_tick.saturating_add(1)
        } else {
            u64::MAX
        };
        self.objects.schedule(id, next_tick);
        self.objects.mark_changed();
        true
    }

    pub fn furniture_target_priority(&self, id: ObjectId) -> Option<TargetPriority> {
        let object = self.objects.object(id)?;
        let interaction = furniture_definition(object.object_type)?.interaction();
        if interaction.configuration() != Some(FurnitureConfiguration::TargetPriority) {
            return None;
        }
        TargetPriority::from_raw(object.variant)
    }

    /// Changes a typed, definition-declared targeting configuration. Returns
    /// false for unknown/non-targeting furniture or an unchanged value.
    pub fn set_furniture_target_priority(
        &mut self,
        id: ObjectId,
        priority: TargetPriority,
    ) -> bool {
        let Some(object) = self.objects.object(id) else {
            return false;
        };
        let Some(definition) = furniture_definition(object.object_type) else {
            return false;
        };
        if definition.interaction().configuration() != Some(FurnitureConfiguration::TargetPriority)
            || object.variant == priority.raw()
        {
            return false;
        }
        self.objects.object_mut(id).unwrap().variant = priority.raw();
        self.objects.mark_changed();
        true
    }

    pub fn container(&self, object: ObjectId) -> Option<&ItemContainer> {
        self.objects.object(object)?;
        self.objects.containers.get(&object)
    }

    pub fn container_mut(&mut self, object: ObjectId) -> Option<&mut ItemContainer> {
        self.objects.object(object)?;
        self.objects.containers.get_mut(&object)
    }

    pub fn container_is_empty(&self, object: ObjectId) -> Option<bool> {
        self.container(object).map(ItemContainer::is_empty)
    }

    pub fn battery_charge_milli(&self, object: ObjectId) -> Option<u32> {
        let battery = self.objects.object(object)?;
        (battery.object_type == FurnitureObject::BATTERY).then_some(battery.stored_energy_milli)
    }

    pub fn set_battery_charge_milli(&mut self, object: ObjectId, charge: u32) -> bool {
        let Some(battery) = self.objects.object_mut(object) else {
            return false;
        };
        if battery.object_type != FurnitureObject::BATTERY {
            return false;
        }
        let charge = charge.min(crate::BATTERY_CAPACITY_MILLI);
        if battery.stored_energy_milli == charge {
            return false;
        }
        battery.stored_energy_milli = charge;
        true
    }

    pub fn laser_bore_target(&self, object: ObjectId, powered: bool) -> Option<TilePos> {
        let object = self.objects.object(object)?;
        self.laser_bore_beam(object, powered)?.target
    }

    pub fn turret_kill_count(&self, object: ObjectId) -> Option<u32> {
        let turret = self.objects.object(object)?;
        (turret.object_type == FurnitureObject::TURRET).then_some(turret.kill_count)
    }

    pub fn increment_turret_kill_count(&mut self, object: ObjectId) -> bool {
        let Some(turret) = self.objects.object_mut(object) else {
            return false;
        };
        if turret.object_type != FurnitureObject::TURRET {
            return false;
        }
        turret.kill_count = turret.kill_count.saturating_add(1);
        self.objects.mark_changed();
        true
    }

    pub const fn simulation_tick(&self) -> u64 {
        self.simulation_tick
    }

    pub(crate) fn laser_bore_beam(
        &self,
        object: &WorldObject,
        powered: bool,
    ) -> Option<LaserBoreBeam> {
        if object.object_type != FurnitureObject::LASER_BORE || !object.active || !powered {
            return None;
        }
        let x = object.anchor.x + u32::from(object.width / 2);
        let first_y = object.anchor.y + u32::from(object.height);
        let length_limit = LASER_BORE_MAX_LENGTH.min(self.height.saturating_sub(first_y));
        let target = (0..length_limit).find_map(|offset| {
            let y = first_y + offset;
            (self.tile_in_bounds(x, y, Layer::Foreground) != TileId::EMPTY)
                .then_some(TilePos::new(x, y))
        });
        let length_tiles = target.map_or(length_limit, |position| position.y - first_y);
        Some(LaserBoreBeam {
            x,
            first_y,
            length_tiles,
            target,
        })
    }

    pub fn place_natural_object(
        &mut self,
        object_type: ObjectTypeId,
        anchor: TilePos,
        root: TilePos,
    ) -> Result<ObjectId, ObjectPlacementError> {
        if anchor.x >= self.width
            || anchor.y >= self.height
            || root.x >= self.width
            || root.y >= self.height
        {
            return Err(ObjectPlacementError::OutOfBounds);
        }
        if self.tile_in_bounds(root.x, root.y, Layer::Foreground) == TileId::EMPTY {
            return Err(ObjectPlacementError::RootIsEmpty(root));
        }
        if self.tile_in_bounds(anchor.x, anchor.y, Layer::Foreground) != TileId::EMPTY {
            return Err(ObjectPlacementError::Occupied(anchor));
        }
        let definition = decoration_definition(object_type)
            .ok_or(ObjectPlacementError::UnsupportedType(object_type))?;
        let next_update_tick = definition
            .first_update_delay()
            .map_or(u64::MAX, |delay| self.simulation_tick.saturating_add(delay));
        let id = self.objects.allocate_id();
        self.objects.insert(WorldObject {
            id,
            object_type,
            anchor,
            root,
            width: 1,
            height: 1,
            variant: 0,
            growth_stage: 0,
            active: true,
            stored_energy_milli: 0,
            machine_target_y: u32::MAX,
            kill_count: 0,
            linked_object: 0,
            motion_position_milli: 0,
            next_update_tick,
        })?;
        Ok(id)
    }

    /// Resolves a rope use to the next cell at the bottom of its column. Using
    /// any existing segment therefore keeps extending the same rope downward.
    pub fn rope_placement_target(&self, target: TilePos) -> Result<TilePos, ObjectPlacementError> {
        if target.x >= self.width || target.y >= self.height {
            return Err(ObjectPlacementError::OutOfBounds);
        }
        if let Some(rope) = self
            .object_at(target)
            .filter(|object| object.object_type == ROPE_OBJECT)
        {
            let y = rope
                .anchor
                .y
                .checked_add(u32::from(rope.height))
                .filter(|&y| y < self.height)
                .ok_or(ObjectPlacementError::OutOfBounds)?;
            let next = TilePos::new(rope.anchor.x, y);
            return self.validate_rope_cell(next).map(|()| next);
        }
        self.validate_rope_cell(target)?;
        if let Some(above) = target.y.checked_sub(1).map(|y| TilePos::new(target.x, y))
            && let Some(rope) = self
                .object_at(above)
                .filter(|object| object.object_type == ROPE_OBJECT)
        {
            return (target.y == rope.anchor.y + u32::from(rope.height))
                .then_some(target)
                .ok_or(ObjectPlacementError::Occupied(target));
        }
        if self.rope_support(target).is_none() {
            return Err(ObjectPlacementError::RootIsEmpty(target));
        }
        Ok(target)
    }

    pub fn place_or_extend_rope(
        &mut self,
        target: TilePos,
    ) -> Result<TilePos, ObjectPlacementError> {
        let placement = self.rope_placement_target(target)?;
        let extension = self
            .object_at(target)
            .filter(|object| object.object_type == ROPE_OBJECT)
            .map(WorldObject::id)
            .or_else(|| {
                placement.y.checked_sub(1).and_then(|y| {
                    self.object_at(TilePos::new(placement.x, y))
                        .filter(|object| object.object_type == ROPE_OBJECT)
                        .map(WorldObject::id)
                })
            });
        if let Some(rope) = extension {
            return self
                .objects
                .extend_down(rope)
                .then_some(placement)
                .ok_or(ObjectPlacementError::Occupied(placement));
        }

        let root = self
            .rope_support(placement)
            .ok_or(ObjectPlacementError::RootIsEmpty(placement))?;
        let id = self.objects.allocate_id();
        self.objects.insert(WorldObject {
            id,
            object_type: ROPE_OBJECT,
            anchor: placement,
            root,
            width: 1,
            height: 1,
            variant: 0,
            growth_stage: 0,
            active: true,
            stored_energy_milli: 0,
            machine_target_y: u32::MAX,
            kill_count: 0,
            linked_object: 0,
            motion_position_milli: 0,
            next_update_tick: u64::MAX,
        })?;
        Ok(placement)
    }

    fn validate_rope_cell(&self, position: TilePos) -> Result<(), ObjectPlacementError> {
        if position.x >= self.width || position.y >= self.height {
            return Err(ObjectPlacementError::OutOfBounds);
        }
        if self.tile_in_bounds(position.x, position.y, Layer::Foreground) != TileId::EMPTY
            || self.objects.occupying(position).is_some()
        {
            return Err(ObjectPlacementError::Occupied(position));
        }
        Ok(())
    }

    fn rope_support(&self, position: TilePos) -> Option<TilePos> {
        if self.tile_in_bounds(position.x, position.y, Layer::Background) != TileId::EMPTY {
            return Some(position);
        }
        [
            position
                .y
                .checked_sub(1)
                .map(|y| TilePos::new(position.x, y)),
            position
                .x
                .checked_sub(1)
                .map(|x| TilePos::new(x, position.y)),
            position
                .x
                .checked_add(1)
                .filter(|&x| x < self.width)
                .map(|x| TilePos::new(x, position.y)),
        ]
        .into_iter()
        .flatten()
        .find(|support| {
            self.tile_in_bounds(support.x, support.y, Layer::Foreground) != TileId::EMPTY
        })
    }

    /// Resolves use on an anchor, any existing cable segment, or the empty cell
    /// immediately below a cable to the next segment at the column's bottom.
    pub fn powered_cable_placement_target(
        &self,
        target: TilePos,
    ) -> Result<TilePos, ObjectPlacementError> {
        if target.x >= self.width || target.y >= self.height {
            return Err(ObjectPlacementError::OutOfBounds);
        }
        if let Some(object) = self.object_at(target) {
            if object.object_type == POWERED_CABLE_OBJECT {
                let next = TilePos::new(
                    object.anchor.x,
                    object
                        .anchor
                        .y
                        .checked_add(u32::from(object.height))
                        .filter(|&y| y < self.height)
                        .ok_or(ObjectPlacementError::OutOfBounds)?,
                );
                return self.validate_powered_cable_cell(next).map(|()| next);
            }
            if object.object_type == FurnitureObject::POWERED_CABLE_ANCHOR {
                let below = TilePos::new(
                    target.x,
                    target
                        .y
                        .checked_add(1)
                        .filter(|&y| y < self.height)
                        .ok_or(ObjectPlacementError::OutOfBounds)?,
                );
                if self
                    .object_at(below)
                    .is_some_and(|object| object.object_type == POWERED_CABLE_OBJECT)
                {
                    return self.powered_cable_placement_target(below);
                }
                return self.validate_powered_cable_cell(below).map(|()| below);
            }
        }

        self.validate_powered_cable_cell(target)?;
        let above = target
            .y
            .checked_sub(1)
            .map(|y| TilePos::new(target.x, y))
            .ok_or(ObjectPlacementError::MissingPoweredCableAttachment(target))?;
        let attached = self.object_at(above).is_some_and(|object| {
            object.object_type == POWERED_CABLE_OBJECT
                && object.anchor.y + u32::from(object.height) == target.y
                || object.object_type == FurnitureObject::POWERED_CABLE_ANCHOR
        });
        attached
            .then_some(target)
            .ok_or(ObjectPlacementError::MissingPoweredCableAttachment(target))
    }

    pub fn place_or_extend_powered_cable(
        &mut self,
        target: TilePos,
    ) -> Result<TilePos, ObjectPlacementError> {
        let placement = self.powered_cable_placement_target(target)?;
        let extension = self
            .object_at(target)
            .filter(|object| object.object_type == POWERED_CABLE_OBJECT)
            .map(WorldObject::id)
            .or_else(|| {
                placement.y.checked_sub(1).and_then(|y| {
                    self.object_at(TilePos::new(placement.x, y))
                        .filter(|object| object.object_type == POWERED_CABLE_OBJECT)
                        .map(WorldObject::id)
                })
            });
        if let Some(cable) = extension {
            return self
                .objects
                .extend_down(cable)
                .then_some(placement)
                .ok_or(ObjectPlacementError::Occupied(placement));
        }

        let root = TilePos::new(placement.x, placement.y - 1);
        if !self
            .object_at(root)
            .is_some_and(|object| object.object_type == FurnitureObject::POWERED_CABLE_ANCHOR)
        {
            return Err(ObjectPlacementError::MissingPoweredCableAttachment(root));
        }
        let id = self.objects.allocate_id();
        self.objects.insert(WorldObject {
            id,
            object_type: POWERED_CABLE_OBJECT,
            anchor: placement,
            root,
            width: 1,
            height: 1,
            variant: 0,
            growth_stage: 0,
            active: true,
            stored_energy_milli: 0,
            machine_target_y: u32::MAX,
            kill_count: 0,
            linked_object: 0,
            motion_position_milli: 0,
            next_update_tick: u64::MAX,
        })?;
        Ok(placement)
    }

    fn validate_powered_cable_cell(&self, position: TilePos) -> Result<(), ObjectPlacementError> {
        if position.x >= self.width || position.y >= self.height {
            return Err(ObjectPlacementError::OutOfBounds);
        }
        if self.tile_in_bounds(position.x, position.y, Layer::Foreground) != TileId::EMPTY
            || self.objects.occupying(position).is_some()
        {
            return Err(ObjectPlacementError::Occupied(position));
        }
        Ok(())
    }

    /// Clicking any cable segment with an anchor item targets the first empty
    /// cell below the column, making the lower conductive endpoint easy to add.
    pub fn powered_cable_anchor_placement_target(
        &self,
        target: TilePos,
    ) -> Result<TilePos, ObjectPlacementError> {
        let anchor = if let Some(cable) = self
            .object_at(target)
            .filter(|object| object.object_type == POWERED_CABLE_OBJECT)
        {
            TilePos::new(
                cable.anchor.x,
                cable
                    .anchor
                    .y
                    .checked_add(u32::from(cable.height))
                    .filter(|&y| y < self.height)
                    .ok_or(ObjectPlacementError::OutOfBounds)?,
            )
        } else {
            target
        };
        self.can_place_furniture(FurnitureObject::POWERED_CABLE_ANCHOR, anchor)?;
        Ok(anchor)
    }

    pub(crate) fn powered_cable_anchor_ids(&self, cable_id: ObjectId) -> [Option<ObjectId>; 2] {
        let Some(cable) = self
            .objects
            .object(cable_id)
            .filter(|object| object.object_type == POWERED_CABLE_OBJECT)
        else {
            return [None; 2];
        };
        let top = cable.anchor.y.checked_sub(1).and_then(|y| {
            self.object_at(TilePos::new(cable.anchor.x, y))
                .filter(|object| object.object_type == FurnitureObject::POWERED_CABLE_ANCHOR)
                .map(WorldObject::id)
        });
        let bottom_y = cable.anchor.y + u32::from(cable.height);
        let bottom = (bottom_y < self.height)
            .then(|| self.object_at(TilePos::new(cable.anchor.x, bottom_y)))
            .flatten()
            .filter(|object| object.object_type == FurnitureObject::POWERED_CABLE_ANCHOR)
            .map(WorldObject::id);
        [top, bottom]
    }

    pub fn can_remove_object(&self, id: ObjectId) -> bool {
        let Some(object) = self.objects.object(id) else {
            return false;
        };
        if self
            .objects
            .containers
            .get(&id)
            .is_some_and(|container| !container.is_empty())
        {
            return false;
        }
        if object.object_type == FurnitureObject::POWERED_CABLE_ANCHOR {
            let anchor = object.anchor;
            let adjacent_cable = [anchor.y.checked_sub(1), anchor.y.checked_add(1)]
                .into_iter()
                .flatten()
                .filter(|&y| y < self.height)
                .any(|y| {
                    self.object_at(TilePos::new(anchor.x, y))
                        .is_some_and(|object| object.object_type == POWERED_CABLE_OBJECT)
                });
            if adjacent_cable {
                return false;
            }
        }
        object.object_type != POWERED_CABLE_OBJECT
            || !self.objects.has_lift_station_for_cable(id)
                && self.objects.cargo_lift_for_cable(id).is_none()
    }

    /// Checks a top-left furniture anchor against its complete footprint,
    /// attachment rule, and any required support row. Item placement converts
    /// the cursor's bottom-left tile to this canonical anchor first.
    pub fn can_place_furniture(
        &self,
        object_type: ObjectTypeId,
        anchor: TilePos,
    ) -> Result<(), ObjectPlacementError> {
        let definition = furniture_definition(object_type)
            .ok_or(ObjectPlacementError::UnsupportedType(object_type))?;
        let [width, height] = definition.size();
        let end_x = anchor
            .x
            .checked_add(u32::from(width).saturating_sub(1))
            .ok_or(ObjectPlacementError::OutOfBounds)?;
        let end_y = anchor
            .y
            .checked_add(u32::from(height).saturating_sub(1))
            .ok_or(ObjectPlacementError::OutOfBounds)?;
        if width == 0 || height == 0 || end_x >= self.width || end_y >= self.height {
            return Err(ObjectPlacementError::OutOfBounds);
        }
        for y in anchor.y..=end_y {
            for x in anchor.x..=end_x {
                let position = TilePos::new(x, y);
                if self.tile_in_bounds(x, y, Layer::Foreground) != TileId::EMPTY
                    || self.objects.occupying(position).is_some()
                {
                    return Err(ObjectPlacementError::Occupied(position));
                }
            }
        }
        if definition.is_item_transport_connector() {
            self.validate_item_transport_connector_placement(anchor)?;
        }
        if object_type == FurnitureObject::CARGO_LIFT {
            return Err(ObjectPlacementError::MissingPoweredCableAttachment(anchor));
        }
        if object_type == FurnitureObject::POWERED_CABLE_ANCHOR {
            self.validate_powered_cable_anchor_attachment(anchor)?;
        }
        match definition.support() {
            FurnitureSupport::Floor | FurnitureSupport::FloorEdges => {
                let support_y = end_y
                    .checked_add(1)
                    .filter(|&y| y < self.height)
                    .ok_or(ObjectPlacementError::OutOfBounds)?;
                for column in 0..width {
                    if !definition.support().requires_column(column, width) {
                        continue;
                    }
                    let x = anchor.x + u32::from(column);
                    let support = TilePos::new(x, support_y);
                    if self.tile_in_bounds(x, support_y, Layer::Foreground) == TileId::EMPTY {
                        return Err(ObjectPlacementError::RootIsEmpty(support));
                    }
                }
            }
            FurnitureSupport::Side => {
                if self.side_or_background_support(anchor).is_none() {
                    return Err(ObjectPlacementError::RootIsEmpty(anchor));
                }
            }
            FurnitureSupport::Free => {}
        }
        Ok(())
    }

    pub fn place_furniture(
        &mut self,
        object_type: ObjectTypeId,
        anchor: TilePos,
    ) -> Result<ObjectId, ObjectPlacementError> {
        self.can_place_furniture(object_type, anchor)?;
        let definition = furniture_definition(object_type)
            .ok_or(ObjectPlacementError::UnsupportedType(object_type))?;
        let [width, height] = definition.size();
        let root = match definition.support() {
            FurnitureSupport::Floor | FurnitureSupport::FloorEdges => {
                TilePos::new(anchor.x, anchor.y + u32::from(height))
            }
            FurnitureSupport::Side => self
                .side_or_background_support(anchor)
                .expect("validated side-supported furniture has a root"),
            FurnitureSupport::Free => anchor,
        };
        let interaction = definition.interaction();
        let starts_active = !interaction.is_activatable();
        let variant = if object_type == FurnitureObject::LIFT_STATION {
            LiftStationConfiguration::DEFAULT.raw()
        } else {
            match interaction.configuration() {
                Some(FurnitureConfiguration::TargetPriority) => TargetPriority::default().raw(),
                None if interaction.is_activatable() => u8::MAX,
                None => 0,
            }
        };
        let linked_object = if object_type == FurnitureObject::LIFT_STATION {
            self.adjacent_station_cable(anchor).map_or(0, ObjectId::raw)
        } else {
            0
        };
        let id = self.objects.allocate_id();
        self.objects.insert(WorldObject {
            id,
            object_type,
            anchor,
            root,
            width,
            height,
            variant,
            growth_stage: 0,
            active: starts_active,
            stored_energy_milli: 0,
            machine_target_y: u32::MAX,
            kill_count: 0,
            linked_object,
            motion_position_milli: 0,
            next_update_tick: u64::MAX,
        })?;
        if let Some(slots) = definition.interaction().container_slots() {
            self.objects
                .containers
                .insert(id, ItemContainer::new(usize::from(slots)));
        }
        Ok(id)
    }

    pub(crate) fn transfer_one_container_item(
        &mut self,
        source: ObjectId,
        destination: ObjectId,
        registry: &crate::ItemRegistry,
    ) -> bool {
        if source == destination {
            return false;
        }
        let Some(mut source_container) = self.objects.containers.remove(&source) else {
            return false;
        };
        let moved = self
            .objects
            .containers
            .get_mut(&destination)
            .is_some_and(|destination| source_container.transfer_one_to(destination, registry));
        self.objects.containers.insert(source, source_container);
        moved
    }

    pub fn remove_object(&mut self, id: ObjectId) -> Option<WorldObject> {
        if !self.can_remove_object(id) {
            return None;
        }
        self.objects.remove(id)
    }

    pub fn remove_object_at(&mut self, position: TilePos) -> Option<WorldObject> {
        let id = self.objects.occupying(position)?;
        self.remove_object(id)
    }

    fn validate_powered_cable_anchor_attachment(
        &self,
        anchor: TilePos,
    ) -> Result<(), ObjectPlacementError> {
        let attached = self.side_or_background_support(anchor).is_some()
            || orthogonal_neighbours(anchor, self.width, self.height).any(|position| {
                self.tile_in_bounds(position.x, position.y, Layer::Foreground) != TileId::EMPTY
                    || self
                        .object_at(position)
                        .is_some_and(|object| object.object_type == POWERED_CABLE_OBJECT)
            });
        attached
            .then_some(())
            .ok_or(ObjectPlacementError::MissingPoweredCableAttachment(anchor))
    }

    fn validate_item_transport_connector_placement(
        &self,
        anchor: TilePos,
    ) -> Result<(), ObjectPlacementError> {
        let neighbours = self.item_transport_neighbours(anchor);
        let beside_solid_tile = self.tile_in_bounds(anchor.x, anchor.y, Layer::Background)
            != TileId::EMPTY
            || orthogonal_neighbours(anchor, self.width, self.height).any(|position| {
                self.tile_in_bounds(position.x, position.y, Layer::Foreground) != TileId::EMPTY
            });
        if neighbours.is_empty() && !beside_solid_tile {
            return Err(ObjectPlacementError::MissingTransportConnection(anchor));
        }
        if neighbours.len() > 2 {
            return Err(ObjectPlacementError::UnsupportedTransportJunction(anchor));
        }
        for neighbour in neighbours {
            let Some(object) = self.objects.object(neighbour) else {
                continue;
            };
            let Some(definition) = furniture_definition(object.object_type) else {
                continue;
            };
            if definition.is_item_transport_connector()
                && self.item_transport_neighbours(object.anchor).len() >= 2
            {
                return Err(ObjectPlacementError::UnsupportedTransportJunction(
                    object.anchor,
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn side_or_background_support(&self, position: TilePos) -> Option<TilePos> {
        if self.tile_in_bounds(position.x, position.y, Layer::Background) != TileId::EMPTY {
            return Some(position);
        }
        orthogonal_neighbours(position, self.width, self.height).find(|neighbour| {
            self.tile_in_bounds(neighbour.x, neighbour.y, Layer::Foreground) != TileId::EMPTY
        })
    }

    fn blocks_foreground_tile_placement(&self, position: TilePos) -> bool {
        self.object_at(position).is_some_and(|object| {
            matches!(
                object.object_type(),
                POWERED_CABLE_OBJECT | FurnitureObject::CARGO_LIFT
            )
        })
    }

    fn item_transport_neighbours(&self, position: TilePos) -> Vec<ObjectId> {
        let mut neighbours = Vec::with_capacity(4);
        for adjacent in orthogonal_neighbours(position, self.width, self.height) {
            let Some(object) = self.object_at(adjacent) else {
                continue;
            };
            let Some(definition) = furniture_definition(object.object_type) else {
                continue;
            };
            if (definition.is_item_transport_connector()
                || definition.interaction().item_transport_role().is_some())
                && !neighbours.contains(&object.id)
            {
                neighbours.push(object.id);
            }
        }
        neighbours
    }
}

fn orthogonal_neighbours(
    position: TilePos,
    width: u32,
    height: u32,
) -> impl Iterator<Item = TilePos> {
    [
        position
            .x
            .checked_sub(1)
            .map(|x| TilePos::new(x, position.y)),
        position
            .x
            .checked_add(1)
            .filter(|&x| x < width)
            .map(|x| TilePos::new(x, position.y)),
        position
            .y
            .checked_sub(1)
            .map(|y| TilePos::new(position.x, y)),
        position
            .y
            .checked_add(1)
            .filter(|&y| y < height)
            .map(|y| TilePos::new(position.x, y)),
    ]
    .into_iter()
    .flatten()
}
