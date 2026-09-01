use super::{
    BoreBeamVisual, ChunkPos, FurnitureBehavior, FurnitureConfiguration, FurnitureFacing,
    FurnitureInteraction, FurnitureObject, FurnitureSupport, LASER_DRILL_MAX_LENGTH, LaserBoreBeam,
    LaserDrillAim, LaserDrillBeam, Layer, LiftStationConfiguration, NaturalObject, ObjectId,
    ObjectPlacementError, ObjectTypeId, POWERED_CABLE_OBJECT, ROPE_OBJECT, RedShaftBoreBeam,
    RemovedObject, SKY_MACHINE_MIN_ELEVATION_DECIMETRES, TargetPriority, TileId, TilePos, World,
    WorldObject, configuration_variant, decoration_definition, furniture_definition,
};
use crate::items::ItemContainer;

mod lift;
mod placement;

const fn is_natural_object(object_type: ObjectTypeId) -> bool {
    matches!(
        object_type,
        NaturalObject::GRASS
            | NaturalObject::PEBBLE
            | NaturalObject::VINE
            | NaturalObject::HANGING_STONE
    )
}

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

    pub(crate) fn power_changes_since(
        &self,
        revision: u64,
    ) -> Option<impl Iterator<Item = super::objects::PowerTopologyChange> + '_> {
        self.objects.power_changes_since(revision)
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

    pub fn machine_health(&self, id: ObjectId) -> Option<super::MachineHealth> {
        let object = self.objects.object(id)?;
        let maximum = furniture_definition(object.object_type)?.maximum_health()?;
        Some(super::MachineHealth::new(object.health, maximum))
    }

    /// Damages an active machine. Reaching zero health disables it immediately;
    /// broken machines cannot be reactivated until a repair path is introduced.
    pub fn damage_machine(&mut self, id: ObjectId, amount: u16) -> super::MachineDamage {
        let Some(health) = self.machine_health(id) else {
            return super::MachineDamage::default();
        };
        if health.is_disabled() || amount == 0 {
            return super::MachineDamage::default();
        }
        let applied = amount.min(health.current());
        let disabled = applied == health.current();
        let is_bore = self.objects.object(id).is_some_and(|object| {
            furniture_definition(object.object_type).is_some_and(|definition| {
                matches!(
                    definition.behavior(),
                    FurnitureBehavior::VerticalBore(_) | FurnitureBehavior::DirectionalDrill
                )
            })
        });
        if let Some(object) = self.objects.object_mut(id) {
            object.health -= applied;
            if disabled {
                object.active = false;
                if is_bore {
                    object.growth_stage = 0;
                    object.machine_target_y = u32::MAX;
                }
            }
        }
        if disabled {
            self.objects.schedule(id, u64::MAX);
        }
        self.objects.mark_changed();
        super::MachineDamage { applied, disabled }
    }

    /// Player block placement requires an adjacent tile or structural furniture.
    /// Object occupancy makes the structural lookup independent of object count.
    pub fn can_place_tile_adjacent(&self, position: TilePos, layer: Layer) -> bool {
        position.x < self.width
            && position.y < self.height
            && self.tile_in_bounds(position.x, position.y, layer) == TileId::EMPTY
            && (layer != Layer::Foreground || !self.blocks_foreground_tile_placement(position))
            && ((layer == Layer::Foreground
                && self.tile_in_bounds(position.x, position.y, Layer::Background) != TileId::EMPTY)
                || (layer == Layer::Background
                    && self.tile_in_bounds(position.x, position.y, Layer::Foreground)
                        != TileId::EMPTY)
                || orthogonal_neighbours(position, self.width, self.height).any(|neighbour| {
                    self.tile_in_bounds(neighbour.x, neighbour.y, layer) != TileId::EMPTY
                        || (layer == Layer::Foreground
                            && self.is_structural_furniture_at(neighbour))
                        || (layer == Layer::Background
                            && self.tile_in_bounds(neighbour.x, neighbour.y, Layer::Foreground)
                                != TileId::EMPTY)
                }))
    }

    #[inline]
    pub(crate) fn is_structural_furniture_at(&self, position: TilePos) -> bool {
        self.objects.structural_at(position)
    }

    #[inline]
    pub(crate) fn is_solid_cell(&self, position: TilePos) -> bool {
        self.tile_in_bounds(position.x, position.y, Layer::Foreground) != TileId::EMPTY
            || self.is_structural_furniture_at(position)
    }

    #[inline]
    pub(crate) fn is_collision_cell(&self, position: TilePos) -> bool {
        self.is_solid_cell(position)
            || self.object_at(position).is_some_and(|object| {
                object.object_type() == FurnitureObject::DOOR && !object.is_active()
            })
    }

    pub fn furniture_interaction_at(
        &self,
        position: TilePos,
    ) -> Option<(ObjectId, FurnitureInteraction)> {
        let object = self.furniture_at(position)?;
        let definition = furniture_definition(object.object_type)?;
        Some((object.id, definition.interaction()))
    }

    /// Orbital launchers require a clear vertical corridor across their roof
    /// and must sit above the supported orbital-service depth.
    pub fn orbital_export_has_sky_access(&self, launcher: ObjectId) -> bool {
        self.sky_machine_has_access(launcher, FurnitureObject::ORBITAL_EXPORT_LAUNCHER)
    }

    /// Solar arrays use the same open-sky and altitude restrictions as orbital
    /// exporters so underground panels cannot generate power.
    pub fn solar_array_has_sky_access(&self, solar: ObjectId) -> bool {
        self.sky_machine_has_access(solar, FurnitureObject::SOLAR_ARRAY)
    }

    fn sky_machine_has_access(&self, id: ObjectId, expected_type: ObjectTypeId) -> bool {
        let Some(object) = self.objects.object(id) else {
            return false;
        };
        if object.object_type != expected_type {
            return false;
        }
        let bottom = object
            .anchor
            .y
            .saturating_add(u32::from(object.height).saturating_sub(1));
        if self.elevation_decimetres(bottom as f32) <= SKY_MACHINE_MIN_ELEVATION_DECIMETRES {
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
        if !definition.interaction().is_activatable()
            || object.active == active
            || active && object.health == 0 && definition.maximum_health().is_some()
        {
            return false;
        }
        let is_bore = matches!(
            definition.behavior(),
            FurnitureBehavior::VerticalBore(_) | FurnitureBehavior::DirectionalDrill
        );
        if let Some(object) = self.objects.object_mut(id) {
            object.active = active;
            if is_bore {
                object.growth_stage = 0;
                object.machine_target_y = u32::MAX;
            }
        }
        let next_tick = if active && is_bore {
            self.simulation_tick.saturating_add(1)
        } else {
            u64::MAX
        };
        self.objects.schedule(id, next_tick);
        self.objects.mark_changed();
        true
    }

    pub fn toggle_door(&mut self, id: ObjectId) -> bool {
        let Some(object) = self.objects.object(id) else {
            return false;
        };
        if object.object_type != FurnitureObject::DOOR {
            return false;
        }
        let open = object.active;
        self.objects.object_mut(id).unwrap().active = !open;
        self.objects.mark_spatial_changed();
        true
    }

    pub fn furniture_target_priority(&self, id: ObjectId) -> Option<TargetPriority> {
        let object = self.objects.object(id)?;
        let interaction = furniture_definition(object.object_type)?.interaction();
        if interaction.configuration() != Some(FurnitureConfiguration::TargetPriority) {
            return None;
        }
        TargetPriority::from_raw(configuration_variant(object.variant))
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
            || configuration_variant(object.variant) == priority.raw()
        {
            return false;
        }
        let facing = FurnitureFacing::from_variant(object.variant);
        self.objects.object_mut(id).unwrap().variant = facing.apply_to_variant(priority.raw());
        self.objects.mark_changed();
        true
    }

    pub fn furniture_facing(&self, id: ObjectId) -> Option<FurnitureFacing> {
        let object = self.objects.object(id)?;
        furniture_definition(object.object_type)
            .filter(|definition| definition.supports_facing())
            .map(|_| FurnitureFacing::from_variant(object.variant))
    }

    pub fn laser_drill_aim(&self, id: ObjectId) -> Option<LaserDrillAim> {
        let object = self.objects.object(id)?;
        let interaction = furniture_definition(object.object_type)?.interaction();
        if interaction.configuration() != Some(FurnitureConfiguration::LaserAim) {
            return None;
        }
        LaserDrillAim::from_raw(object.variant)
    }

    pub fn set_laser_drill_aim(&mut self, id: ObjectId, aim: LaserDrillAim) -> bool {
        let Some(object) = self.objects.object(id) else {
            return false;
        };
        let Some(definition) = furniture_definition(object.object_type) else {
            return false;
        };
        if definition.interaction().configuration() != Some(FurnitureConfiguration::LaserAim)
            || object.variant == aim.raw()
        {
            return false;
        }
        let active = object.active;
        let object = self.objects.object_mut(id).unwrap();
        object.variant = aim.raw();
        object.growth_stage = 0;
        object.machine_target_y = u32::MAX;
        self.objects.schedule(
            id,
            if active {
                self.simulation_tick.saturating_add(1)
            } else {
                u64::MAX
            },
        );
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

    /// Atomically empties a persistent machine container and returns its stacks
    /// in slot order. This is used when a machine is disabled by world damage so
    /// its cargo can be recovered without removing the broken machine itself.
    pub fn take_container_contents(&mut self, object: ObjectId) -> Option<Vec<crate::ItemStack>> {
        let container = self.container_mut(object)?;
        let mut contents = Vec::new();
        for slot in 0..container.slots().len() {
            if let Some(stack) = container.take_stack(slot) {
                contents.push(stack);
            }
        }
        Some(contents)
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
        match furniture_definition(object.object_type)?.behavior() {
            FurnitureBehavior::VerticalBore(_) => {
                let beam = self.vertical_bore_beam(object, powered)?;
                beam.target_y
                    .map(|y| TilePos::new(beam.first_x + beam.width / 2, y))
            }
            FurnitureBehavior::DirectionalDrill => self.laser_drill_beam(object, powered)?.target,
            _ => None,
        }
    }

    pub fn turret_kill_count(&self, object: ObjectId) -> Option<u32> {
        let turret = self.objects.object(object)?;
        is_turret_type(turret.object_type).then_some(turret.kill_count)
    }

    pub fn increment_turret_kill_count(&mut self, object: ObjectId) -> bool {
        let Some(turret) = self.objects.object_mut(object) else {
            return false;
        };
        if !is_turret_type(turret.object_type) {
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
        if !matches!(
            furniture_definition(object.object_type)?.behavior(),
            FurnitureBehavior::VerticalBore(behavior) if behavior.beam == BoreBeamVisual::Cyan
        ) {
            return None;
        }
        self.vertical_bore_beam(object, powered)
    }

    pub(crate) fn vertical_bore_beam(
        &self,
        object: &WorldObject,
        powered: bool,
    ) -> Option<LaserBoreBeam> {
        let FurnitureBehavior::VerticalBore(behavior) =
            furniture_definition(object.object_type)?.behavior()
        else {
            return None;
        };
        if !object.active || !powered {
            return None;
        }
        let width = u32::from(behavior.shaft_width);
        let first_x = object.anchor.x + u32::from(object.width).saturating_sub(width) / 2;
        let first_y = object.anchor.y + u32::from(object.height);
        let length_limit = behavior
            .maximum_length
            .saturating_add(self.specialist_bonuses().drill_depth_tiles())
            .min(self.height.saturating_sub(first_y));
        let target_y = (u32::from(behavior.first_scan_offset)..length_limit).find_map(|offset| {
            let y = first_y + offset;
            (first_x..first_x + width)
                .any(|x| self.tile_in_bounds(x, y, Layer::Foreground) != TileId::EMPTY)
                .then_some(y)
        });
        let length_tiles = target_y.map_or(length_limit, |y| y - first_y);
        Some(LaserBoreBeam {
            first_x,
            width,
            first_y,
            length_tiles,
            target_y,
        })
    }

    pub(crate) fn red_shaft_bore_beam(
        &self,
        object: &WorldObject,
        powered: bool,
    ) -> Option<RedShaftBoreBeam> {
        if !matches!(
            furniture_definition(object.object_type)?.behavior(),
            FurnitureBehavior::VerticalBore(behavior) if behavior.beam == BoreBeamVisual::Red
        ) {
            return None;
        }
        let beam = self.vertical_bore_beam(object, powered)?;
        Some(RedShaftBoreBeam {
            first_x: beam.first_x,
            width: beam.width,
            first_y: beam.first_y,
            length_tiles: beam.length_tiles,
            target_y: beam.target_y,
        })
    }

    pub(crate) fn laser_drill_beam(
        &self,
        object: &WorldObject,
        powered: bool,
    ) -> Option<LaserDrillBeam> {
        if !matches!(
            furniture_definition(object.object_type)?.behavior(),
            FurnitureBehavior::DirectionalDrill
        ) || !object.active
            || !powered
        {
            return None;
        }
        let aim = LaserDrillAim::from_raw(object.variant)?;
        let first_tile = TilePos::new(
            object.anchor.x + u32::from(object.width / 2),
            object.anchor.y + u32::from(object.height),
        );
        let origin = [first_tile.x as f32, first_tile.y as f32 - 0.5];
        let mut endpoint = origin;
        let mut steps = 0;
        let mut target = None;
        let length_limit =
            LASER_DRILL_MAX_LENGTH.saturating_add(self.specialist_bonuses().drill_depth_tiles());
        for step in 0..length_limit {
            let Some(tile) =
                offset_tile(first_tile, aim.tile_offset(step), self.width, self.height)
            else {
                break;
            };
            steps = step + 1;
            endpoint = [tile.x as f32, tile.y as f32 - 0.5];
            if self.tile_in_bounds(tile.x, tile.y, Layer::Foreground) != TileId::EMPTY {
                target = Some(tile);
                break;
            }
        }
        Some(LaserDrillBeam {
            origin,
            endpoint,
            first_tile,
            steps,
            aim,
            target,
        })
    }
}

pub(crate) fn is_turret_type(object_type: ObjectTypeId) -> bool {
    furniture_definition(object_type).is_some_and(|definition| {
        matches!(
            definition.behavior(),
            FurnitureBehavior::EnergyTurret
                | FurnitureBehavior::AmmunitionTurret
                | FurnitureBehavior::DirectionalSentry
        )
    })
}

fn offset_tile(first: TilePos, offset: [i32; 2], width: u32, height: u32) -> Option<TilePos> {
    let x = i64::from(first.x) + i64::from(offset[0]);
    let y = i64::from(first.y) + i64::from(offset[1]);
    (x >= 0 && x < i64::from(width) && y >= 0 && y < i64::from(height))
        .then_some(TilePos::new(x as u32, y as u32))
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
