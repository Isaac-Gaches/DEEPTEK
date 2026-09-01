use super::{Collider, Health, Lifeform, Player, Transform};
use crate::{FurnitureObject, TilePos, World as TerrainWorld};
use hecs::{Entity, World};
use std::collections::HashMap;

pub const SPIKE_CONTACT_DAMAGE: u16 = 12;
pub const SPIKE_DAMAGE_INTERVAL_SECONDS: f32 = 0.6;

const FOOT_SAMPLE_EPSILON: f32 = 0.01;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpikeDamageUpdate {
    pub contacts_damaged: usize,
    pub damage_dealt: u32,
    pub lifeforms_killed: usize,
}

#[derive(Clone, Copy, Debug)]
struct VulnerableEntity {
    entity: Entity,
    dies_at_zero: bool,
}

/// Applies paced contact damage without scanning every placed spike. Each player
/// or lifeform samples only the furniture cells directly beneath its feet.
#[derive(Debug, Default)]
pub struct SpikeDamageSystem {
    cooldowns: HashMap<Entity, f32>,
    vulnerable: Vec<VulnerableEntity>,
    killed: Vec<Entity>,
}

impl SpikeDamageSystem {
    pub fn update(
        &mut self,
        entities: &mut World,
        terrain: &TerrainWorld,
        elapsed: f32,
    ) -> SpikeDamageUpdate {
        let elapsed = elapsed.max(0.0);
        self.cooldowns.retain(|entity, remaining| {
            *remaining = (*remaining - elapsed).max(0.0);
            entities.contains(*entity)
        });
        self.vulnerable.clear();
        self.killed.clear();

        for (entity, (_, transform, collider)) in
            entities.query::<(&Player, &Transform, &Collider)>().iter()
        {
            if standing_on_spikes(terrain, transform, collider) {
                self.vulnerable.push(VulnerableEntity {
                    entity,
                    dies_at_zero: false,
                });
            }
        }
        for (entity, (_, transform, collider)) in entities
            .query::<(&Lifeform, &Transform, &Collider)>()
            .iter()
        {
            if standing_on_spikes(terrain, transform, collider) {
                self.vulnerable.push(VulnerableEntity {
                    entity,
                    dies_at_zero: true,
                });
            }
        }

        let mut update = SpikeDamageUpdate::default();
        for vulnerable in self.vulnerable.iter().copied() {
            if self
                .cooldowns
                .get(&vulnerable.entity)
                .is_some_and(|remaining| *remaining > 0.0)
            {
                continue;
            }
            let Some((applied, killed)) =
                entities
                    .get::<&mut Health>(vulnerable.entity)
                    .ok()
                    .map(|mut health| {
                        let applied = health.damage(SPIKE_CONTACT_DAMAGE);
                        (applied, health.current() == 0)
                    })
            else {
                continue;
            };
            if applied == 0 {
                continue;
            }
            self.cooldowns
                .insert(vulnerable.entity, SPIKE_DAMAGE_INTERVAL_SECONDS);
            update.contacts_damaged += 1;
            update.damage_dealt = update.damage_dealt.saturating_add(u32::from(applied));
            if killed && vulnerable.dies_at_zero {
                self.killed.push(vulnerable.entity);
            }
        }
        for entity in self.killed.drain(..) {
            self.cooldowns.remove(&entity);
            if entities.despawn(entity).is_ok() {
                update.lifeforms_killed += 1;
            }
        }
        update
    }
}

fn standing_on_spikes(terrain: &TerrainWorld, transform: &Transform, collider: &Collider) -> bool {
    if !collider.on_ground {
        return false;
    }
    let centre = [
        transform.position[0] + collider.offset[0],
        transform.position[1] + collider.offset[1],
    ];
    let feet_y = centre[1] + collider.half_extents[1];
    let tile_y = (feet_y + 0.5 - FOOT_SAMPLE_EPSILON).floor();
    let minimum_x = (centre[0] - collider.half_extents[0] + 0.5).floor();
    let maximum_x = (centre[0] + collider.half_extents[0] + 0.5).floor();
    if tile_y < 0.0 || minimum_x < 0.0 {
        return false;
    }
    let tile_y = tile_y as u32;
    if tile_y >= terrain.height() {
        return false;
    }
    let maximum_x = (maximum_x as u32).min(terrain.width().saturating_sub(1));
    for x in minimum_x as u32..=maximum_x {
        if terrain
            .object_at(TilePos::new(x, tile_y))
            .is_some_and(|object| object.object_type() == FurnitureObject::SPIKES)
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ForegroundTile, Layer, LifeformId, LifeformSystem};
    use easy_gpu::assets::Material;
    use easy_gpu::assets_manager::Handle;
    use std::marker::PhantomData;

    fn material() -> Handle<Material> {
        Handle {
            index: 0,
            generation: 0,
            _marker: PhantomData,
        }
    }

    fn spike_world() -> TerrainWorld {
        let mut terrain = TerrainWorld::empty(8, 8, 1).unwrap();
        terrain
            .set_tile(3, 5, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        terrain
            .place_furniture(FurnitureObject::SPIKES, TilePos::new(3, 4))
            .unwrap();
        terrain
    }

    #[test]
    fn grounded_player_takes_paced_spike_damage() {
        let terrain = spike_world();
        let mut entities = World::new();
        let player = entities.spawn((
            Player::default(),
            Transform::new([3.0, 4.0]),
            Collider {
                on_ground: true,
                ..Collider::new(1.0, 1.0)
            },
            Health::new(100),
        ));
        let mut system = SpikeDamageSystem::default();

        assert_eq!(system.update(&mut entities, &terrain, 0.0).damage_dealt, 12);
        assert_eq!(system.update(&mut entities, &terrain, 0.3).damage_dealt, 0);
        assert_eq!(system.update(&mut entities, &terrain, 0.3).damage_dealt, 12);
        assert_eq!(entities.get::<&Health>(player).unwrap().current(), 76);
    }

    #[test]
    fn airborne_player_does_not_trigger_spikes() {
        let terrain = spike_world();
        let mut entities = World::new();
        let player = entities.spawn((
            Player::default(),
            Transform::new([3.0, 4.0]),
            Collider::new(1.0, 1.0),
            Health::new(100),
        ));

        assert_eq!(
            SpikeDamageSystem::default()
                .update(&mut entities, &terrain, 1.0)
                .damage_dealt,
            0
        );
        assert_eq!(entities.get::<&Health>(player).unwrap().current(), 100);
    }

    #[test]
    fn grounded_lifeform_takes_spike_damage() {
        let terrain = spike_world();
        let mut entities = World::new();
        let lifeform = LifeformSystem::with_built_ins()
            .spawn(&mut entities, LifeformId::WALKER, material(), [3.0, 3.65])
            .unwrap();
        entities.get::<&mut Collider>(lifeform).unwrap().on_ground = true;

        let update = SpikeDamageSystem::default().update(&mut entities, &terrain, 0.0);

        assert_eq!(update.contacts_damaged, 1);
        assert_eq!(entities.get::<&Health>(lifeform).unwrap().current(), 28);
    }
}
