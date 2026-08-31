use super::*;

impl World {
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
            health: 0,
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
            if self
                .object_at(next)
                .is_some_and(|lower| lower.object_type == ROPE_OBJECT && lower.anchor == next)
            {
                return Ok(next);
            }
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
            let lower = self
                .object_at(placement)
                .filter(|object| object.object_type == ROPE_OBJECT && object.id != rope)
                .map(WorldObject::id);
            let changed = match lower {
                Some(lower) => self.objects.merge_down(rope, lower),
                None => self.objects.extend_down(rope),
            };
            return changed
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
            health: 0,
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

    /// Resolves use on any existing cable segment or the empty cell immediately
    /// below it to the next segment at the column's bottom. A new cable carries
    /// its own top endpoint, so it only needs ordinary side/background support.
    /// Legacy anchor objects remain accepted so existing worlds stay editable.
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
        let extends_cable = target.y.checked_sub(1).is_some_and(|y| {
            self.object_at(TilePos::new(target.x, y))
                .is_some_and(|object| {
                    object.object_type == POWERED_CABLE_OBJECT
                        && object.anchor.y + u32::from(object.height) == target.y
                })
        });
        let legacy_anchor = target.y.checked_sub(1).is_some_and(|y| {
            self.object_at(TilePos::new(target.x, y))
                .is_some_and(|object| object.object_type == FurnitureObject::POWERED_CABLE_ANCHOR)
        });
        if extends_cable || legacy_anchor || self.powered_cable_support(target).is_some() {
            Ok(target)
        } else {
            Err(ObjectPlacementError::MissingPoweredCableAttachment(target))
        }
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

        let legacy_root = placement.y.checked_sub(1).and_then(|y| {
            let position = TilePos::new(placement.x, y);
            self.object_at(position)
                .is_some_and(|object| object.object_type == FurnitureObject::POWERED_CABLE_ANCHOR)
                .then_some(position)
        });
        let root = legacy_root
            .or_else(|| self.powered_cable_support(placement))
            .ok_or(ObjectPlacementError::MissingPoweredCableAttachment(
                placement,
            ))?;
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
            health: 0,
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

    fn powered_cable_support(&self, position: TilePos) -> Option<TilePos> {
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
        .find(|support| self.is_solid_cell(*support))
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

    pub fn can_remove_object(&self, id: ObjectId) -> bool {
        self.can_remove_object_with_dependents(id) && !self.objects.has_dependents(id)
    }

    pub(crate) fn can_remove_object_with_dependents(&self, id: ObjectId) -> bool {
        let Some(object) = self.objects.object(id) else {
            return false;
        };
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
                    if !self.is_solid_cell(support) {
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
        self.place_furniture_facing(object_type, anchor, FurnitureFacing::Right)
    }

    pub fn place_furniture_facing(
        &mut self,
        object_type: ObjectTypeId,
        anchor: TilePos,
        facing: FurnitureFacing,
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
        let starts_active = !interaction.is_activatable() && !interaction.toggles_door();
        let mut variant = if object_type == FurnitureObject::LIFT_STATION {
            LiftStationConfiguration::DEFAULT.raw()
        } else {
            match interaction.configuration() {
                Some(FurnitureConfiguration::TargetPriority) => TargetPriority::default().raw(),
                Some(FurnitureConfiguration::LaserAim) => LaserDrillAim::default().raw(),
                None if definition.supports_facing() => 0,
                None if interaction.is_activatable() => u8::MAX,
                None => 0,
            }
        };
        if definition.supports_facing() {
            variant = facing.apply_to_variant(variant);
        }
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
            health: definition.maximum_health().unwrap_or(0),
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

    pub fn remove_object(&mut self, id: ObjectId) -> Option<super::RemovedObject> {
        if !self.can_remove_object(id) {
            return None;
        }
        self.objects.remove(id)
    }

    pub fn remove_object_at(&mut self, position: TilePos) -> Option<super::RemovedObject> {
        let id = self.objects.occupying(position)?;
        self.remove_object(id)
    }

    pub(crate) fn remove_object_at_with_dependents(
        &mut self,
        position: TilePos,
    ) -> Vec<super::RemovedObject> {
        let Some(id) = self.objects.occupying(position) else {
            return Vec::new();
        };
        if !self.can_remove_object_with_dependents(id) {
            return Vec::new();
        }
        self.objects.remove_with_dependents(id)
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
        let neighbours = self.item_transport_connector_neighbours(anchor);
        if neighbours.len() > 2 {
            return Err(ObjectPlacementError::UnsupportedTransportJunction(anchor));
        }
        for neighbour in neighbours {
            let Some(object) = self.objects.object(neighbour) else {
                continue;
            };
            if self
                .item_transport_connector_neighbours(object.anchor)
                .len()
                >= 2
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
        orthogonal_neighbours(position, self.width, self.height)
            .find(|neighbour| self.is_solid_cell(*neighbour))
    }

    pub(crate) fn blocks_foreground_tile_placement(&self, position: TilePos) -> bool {
        self.object_at(position).is_some_and(|object| {
            matches!(
                object.object_type(),
                POWERED_CABLE_OBJECT | FurnitureObject::CARGO_LIFT
            ) || furniture_definition(object.object_type())
                .is_some_and(FurnitureDefinition::is_structural)
        })
    }

    fn item_transport_connector_neighbours(&self, position: TilePos) -> Vec<ObjectId> {
        let mut neighbours = Vec::with_capacity(4);
        for adjacent in orthogonal_neighbours(position, self.width, self.height) {
            let Some(object) = self.object_at(adjacent) else {
                continue;
            };
            if furniture_definition(object.object_type)
                .is_some_and(|definition| definition.is_item_transport_connector())
                && !neighbours.contains(&object.id)
            {
                neighbours.push(object.id);
            }
        }
        neighbours
    }
}
