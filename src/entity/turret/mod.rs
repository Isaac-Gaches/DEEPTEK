mod targeting;

use super::{Collider, DynamicLight, Health, Lifeform, Particle, ParticleKind, Sprite, Transform};
use targeting::*;
// Powered turret targeting, ballistics, and projectile simulation.

use crate::{
    CHUNK_SIZE, ChunkPos, FurnitureFacing, FurnitureObject, ItemId, Layer, ObjectId, PowerSystem,
    SpecialistBonuses, TargetPriority, TileId, World as TerrainWorld,
};
use easy_gpu::assets::Material;
use easy_gpu::assets_manager::Handle;
use hecs::{Entity, World};
use std::collections::{HashMap, HashSet};

const PROJECTILE_COLLISION_STEP: f32 = 0.2;
const TARGET_CELL_SIZE: f32 = 16.0;
const MUZZLE_PARTICLE_COUNT: usize = 4;
const MUZZLE_PARTICLE_COLOUR: [f32; 4] = [0.0, 1.0, 1.0, 1.0];
const TURRET_PROJECTILE_LIGHT: [f32; 3] = [0.0, 0.72, 0.92];
const TURRET_MUZZLE_LIGHT: [f32; 3] = [0.0, 0.52, 0.68];
pub const MAX_TURRET_ELEVATION_DEGREES: f32 = 60.0;

const AMMO_TURRET_STATS: TurretStats = TurretStats {
    range: 30.0,
    fire_interval: 0.28,
    projectile_speed: 50.0,
    projectile_gravity: 14.0,
    projectile_lifetime: 4.0,
    damage: 9,
};
const DIRECTIONAL_SENTRY_STATS: TurretStats = TurretStats {
    range: 24.0,
    fire_interval: 0.55,
    projectile_speed: 58.0,
    projectile_gravity: 0.0,
    projectile_lifetime: 1.0,
    damage: 10,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TurretKind {
    Energy,
    Ammunition,
    Directional,
}

impl TurretKind {
    const fn from_object_type(object_type: crate::ObjectTypeId) -> Option<Self> {
        match object_type {
            FurnitureObject::TURRET => Some(Self::Energy),
            FurnitureObject::AMMO_TURRET => Some(Self::Ammunition),
            FurnitureObject::DIRECTIONAL_SENTRY => Some(Self::Directional),
            _ => None,
        }
    }

    const fn stats(self, energy_stats: TurretStats) -> TurretStats {
        match self {
            Self::Energy => energy_stats,
            Self::Ammunition => AMMO_TURRET_STATS,
            Self::Directional => DIRECTIONAL_SENTRY_STATS,
        }
    }
}

/// All balance values for the built-in turret live in one data object. Games
/// can construct `TurretSystem::new` with a modified value without changing
/// targeting, persistence, UI, or projectile integration code.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurretStats {
    pub range: f32,
    pub fire_interval: f32,
    pub projectile_speed: f32,
    pub projectile_gravity: f32,
    pub projectile_lifetime: f32,
    pub damage: u16,
}

impl Default for TurretStats {
    fn default() -> Self {
        Self {
            range: 30.0,
            fire_interval: 0.75,
            projectile_speed: 42.0,
            projectile_gravity: 18.0,
            projectile_lifetime: 4.0,
            damage: 12,
        }
    }
}

impl TurretStats {
    fn sanitized(self) -> Self {
        Self {
            range: positive_or(self.range, 30.0),
            fire_interval: positive_or(self.fire_interval, 0.75),
            projectile_speed: positive_or(self.projectile_speed, 42.0),
            projectile_gravity: non_negative_or(self.projectile_gravity, 18.0),
            projectile_lifetime: positive_or(self.projectile_lifetime, 4.0),
            damage: if self.damage == 0 { 12 } else { self.damage },
        }
    }
}

fn positive_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

fn non_negative_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        fallback
    }
}

fn apply_security_bonuses(mut stats: TurretStats, bonuses: SpecialistBonuses) -> TurretStats {
    stats.damage = ((u32::from(stats.damage) * u32::from(bonuses.turret_damage_percent().max(1)))
        .div_ceil(100))
    .min(u32::from(u16::MAX)) as u16;
    stats.fire_interval =
        stats.fire_interval * 100.0 / f32::from(bonuses.turret_fire_rate_percent().max(1));
    stats
}

fn effective_target_priority(
    configured: TargetPriority,
    bonuses: SpecialistBonuses,
) -> TargetPriority {
    if bonuses.advanced_turret_targeting() {
        configured
    } else {
        TargetPriority::Closest
    }
}

/// A constant-acceleration round owned exclusively by `TurretSystem`.
/// Keeping it out of the rigid-body integrator guarantees exact SUVAT motion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurretProjectile {
    pub velocity: [f32; 2],
    pub acceleration: [f32; 2],
    pub damage: u16,
    source: Option<ObjectId>,
    remaining_lifetime: f32,
}

impl TurretProjectile {
    pub const fn remaining_lifetime(self) -> f32 {
        self.remaining_lifetime
    }
}

#[derive(Clone, Copy)]
struct TargetSnapshot {
    entity: Entity,
    position: [f32; 2],
    half_extents: [f32; 2],
    offset: [f32; 2],
    health: u16,
}

/// Retains all per-frame scratch data and per-object firing cooldowns.
pub struct TurretSystem {
    stats: TurretStats,
    cooldowns: HashMap<ObjectId, f64>,
    simulation_time: f64,
    seen_turrets: HashSet<ObjectId>,
    candidate_turrets: Vec<ObjectId>,
    targets: Vec<TargetSnapshot>,
    target_buckets: HashMap<[i32; 2], Vec<usize>>,
    maximum_target_radius: f32,
    projectiles_to_remove: Vec<Entity>,
    hits: Vec<(Entity, Entity, u16, Option<ObjectId>)>,
}

impl Default for TurretSystem {
    fn default() -> Self {
        Self::new(TurretStats::default())
    }
}

impl TurretSystem {
    pub fn new(stats: TurretStats) -> Self {
        Self {
            stats: stats.sanitized(),
            cooldowns: HashMap::new(),
            simulation_time: 0.0,
            seen_turrets: HashSet::new(),
            candidate_turrets: Vec::new(),
            targets: Vec::new(),
            target_buckets: HashMap::new(),
            maximum_target_radius: 0.0,
            projectiles_to_remove: Vec::new(),
            hits: Vec::new(),
        }
    }

    pub const fn stats(&self) -> TurretStats {
        self.stats
    }

    pub fn set_stats(&mut self, stats: TurretStats) {
        self.stats = stats.sanitized();
    }

    pub fn update(
        &mut self,
        entities: &mut World,
        terrain: &mut TerrainWorld,
        power: &PowerSystem,
        projectile_material: Handle<Material>,
        particle_material: Handle<Material>,
        elapsed: f32,
    ) {
        self.update_with_bonuses(
            entities,
            terrain,
            power,
            projectile_material,
            particle_material,
            elapsed,
            SpecialistBonuses::default(),
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_with_bonuses(
        &mut self,
        entities: &mut World,
        terrain: &mut TerrainWorld,
        power: &PowerSystem,
        projectile_material: Handle<Material>,
        particle_material: Handle<Material>,
        elapsed: f32,
        bonuses: SpecialistBonuses,
    ) {
        let elapsed = elapsed.clamp(0.0, 0.1);
        if elapsed <= 0.0 {
            return;
        }
        self.simulation_time += f64::from(elapsed);
        self.collect_targets(entities);
        self.update_projectiles(entities, terrain, elapsed);
        self.targets.retain_mut(|target| {
            let Some(health) = entities
                .get::<&Health>(target.entity)
                .ok()
                .map(|health| *health)
            else {
                return false;
            };
            target.health = health.current();
            target.health > 0
        });
        rebuild_target_buckets(&self.targets, &mut self.target_buckets);
        self.fire_turrets(
            entities,
            terrain,
            power,
            projectile_material,
            particle_material,
            bonuses,
        );
    }

    fn collect_targets(&mut self, entities: &World) {
        self.targets.clear();
        self.target_buckets.clear();
        self.maximum_target_radius = 0.0;
        self.targets.extend(
            entities
                .query::<(&Lifeform, &Transform, &Collider, &Health)>()
                .iter()
                .filter(|(_, (_, transform, collider, health))| {
                    collider.enabled
                        && health.current() > 0
                        && transform.position.into_iter().all(f32::is_finite)
                        && collider.half_extents.into_iter().all(f32::is_finite)
                        && collider.offset.into_iter().all(f32::is_finite)
                })
                .map(
                    |(entity, (_, transform, collider, health))| TargetSnapshot {
                        entity,
                        position: transform.position,
                        half_extents: collider.half_extents,
                        offset: collider.offset,
                        health: health.current(),
                    },
                ),
        );
        self.maximum_target_radius = self
            .targets
            .iter()
            .flat_map(|target| {
                [
                    target.offset[0].abs() + target.half_extents[0],
                    target.offset[1].abs() + target.half_extents[1],
                ]
            })
            .fold(0.0_f32, f32::max);
        rebuild_target_buckets(&self.targets, &mut self.target_buckets);
    }

    fn update_projectiles(
        &mut self,
        entities: &mut World,
        terrain: &mut TerrainWorld,
        elapsed: f32,
    ) {
        self.projectiles_to_remove.clear();
        self.hits.clear();
        for (entity, (projectile, transform)) in entities
            .query::<(&mut TurretProjectile, &mut Transform)>()
            .iter()
        {
            let start = transform.position;
            // Exact constant-acceleration SUVAT integration for this time step.
            let end = [
                start[0]
                    + projectile.velocity[0] * elapsed
                    + 0.5 * projectile.acceleration[0] * elapsed * elapsed,
                start[1]
                    + projectile.velocity[1] * elapsed
                    + 0.5 * projectile.acceleration[1] * elapsed * elapsed,
            ];
            projectile.velocity[0] += projectile.acceleration[0] * elapsed;
            projectile.velocity[1] += projectile.acceleration[1] * elapsed;
            projectile.remaining_lifetime -= elapsed;
            transform.position = end;
            transform.rotation = projectile.velocity[1].atan2(projectile.velocity[0]);

            if projectile.remaining_lifetime <= 0.0 {
                self.projectiles_to_remove.push(entity);
                continue;
            }
            let terrain_hit = segment_terrain_hit_fraction(start, end, terrain);
            let target_hit = first_target_hit(
                start,
                end,
                &self.targets,
                &self.target_buckets,
                self.maximum_target_radius,
            );
            if let Some((target, target_fraction)) = target_hit
                && terrain_hit.is_none_or(|terrain_fraction| target_fraction <= terrain_fraction)
            {
                self.hits
                    .push((entity, target, projectile.damage, projectile.source));
                self.projectiles_to_remove.push(entity);
            } else if terrain_hit.is_some() {
                self.projectiles_to_remove.push(entity);
            }
        }

        for &(_, target, damage, source) in &self.hits {
            let killed = entities
                .get::<&mut Health>(target)
                .ok()
                .is_some_and(|mut health| {
                    health.damage(damage);
                    health.current() == 0
                });
            if killed {
                let _ = entities.despawn(target);
                if let Some(source) = source {
                    terrain.increment_turret_kill_count(source);
                }
            }
        }
        self.projectiles_to_remove.sort_unstable();
        self.projectiles_to_remove.dedup();
        for entity in self.projectiles_to_remove.drain(..) {
            let _ = entities.despawn(entity);
        }
    }

    fn fire_turrets(
        &mut self,
        entities: &mut World,
        terrain: &mut TerrainWorld,
        power: &PowerSystem,
        projectile_material: Handle<Material>,
        particle_material: Handle<Material>,
        bonuses: SpecialistBonuses,
    ) {
        self.collect_candidate_turrets(terrain, power);
        for id in self.candidate_turrets.iter().copied() {
            let Some(object) = terrain.object(id) else {
                continue;
            };
            let Some(kind) = TurretKind::from_object_type(object.object_type()) else {
                continue;
            };
            if self
                .cooldowns
                .get(&id)
                .is_some_and(|&ready_at| ready_at > self.simulation_time)
            {
                continue;
            }

            let origin = turret_muzzle(object.anchor(), object.size());
            let facing = terrain.furniture_facing(id).unwrap_or_default();
            let priority = terrain
                .furniture_target_priority(object.id())
                .unwrap_or_default();
            let priority = effective_target_priority(priority, bonuses);
            let stats = apply_security_bonuses(kind.stats(self.stats), bonuses);
            let shot = match kind {
                TurretKind::Energy | TurretKind::Ammunition => select_ballistic_shot(
                    origin,
                    stats,
                    facing,
                    priority,
                    &self.targets,
                    &self.target_buckets,
                    terrain,
                ),
                TurretKind::Directional => select_directional_shot(
                    origin,
                    stats,
                    facing,
                    &self.targets,
                    &self.target_buckets,
                    terrain,
                ),
            };
            let Some((_, velocity)) = shot else {
                continue;
            };
            if kind == TurretKind::Ammunition
                && terrain
                    .container_mut(id)
                    .is_none_or(|container| container.remove_item(ItemId::TURRET_AMMO, 1) != 1)
            {
                continue;
            }
            spawn_turret_projectile(
                entities,
                projectile_material,
                origin,
                velocity,
                stats,
                Some(id),
            );
            spawn_muzzle_particles(entities, particle_material, origin, velocity);
            self.cooldowns
                .insert(id, self.simulation_time + f64::from(stats.fire_interval));
        }
        self.cooldowns.retain(|&id, _| {
            terrain.object(id).is_some_and(|object| {
                TurretKind::from_object_type(object.object_type()).is_some() && object.is_active()
            })
        });
    }

    fn collect_candidate_turrets(&mut self, terrain: &TerrainWorld, power: &PowerSystem) {
        self.seen_turrets.clear();
        self.candidate_turrets.clear();
        let chunk_size = CHUNK_SIZE as f32;
        let maximum_chunk = [
            terrain.width().saturating_sub(1) / CHUNK_SIZE as u32,
            terrain.height().saturating_sub(1) / CHUNK_SIZE as u32,
        ];
        let maximum_range = self
            .stats
            .range
            .max(AMMO_TURRET_STATS.range)
            .max(DIRECTIONAL_SENTRY_STATS.range);
        for target in &self.targets {
            let minimum = [
                ((target.position[0] - maximum_range).max(0.0) / chunk_size).floor() as u32,
                ((target.position[1] - maximum_range).max(0.0) / chunk_size).floor() as u32,
            ];
            let maximum = [
                ((target.position[0] + maximum_range).max(0.0) / chunk_size).floor() as u32,
                ((target.position[1] + maximum_range).max(0.0) / chunk_size).floor() as u32,
            ];
            for chunk_y in minimum[1]..=maximum[1].min(maximum_chunk[1]) {
                for chunk_x in minimum[0]..=maximum[0].min(maximum_chunk[0]) {
                    for object in terrain.objects_in_chunk(ChunkPos {
                        x: chunk_x,
                        y: chunk_y,
                    }) {
                        if TurretKind::from_object_type(object.object_type()).is_some()
                            && object.is_active()
                            && power.is_powered(object.id())
                            && self.seen_turrets.insert(object.id())
                        {
                            self.candidate_turrets.push(object.id());
                        }
                    }
                }
            }
        }
        self.candidate_turrets.sort_unstable();
    }
}

#[cfg(test)]
mod tests;
