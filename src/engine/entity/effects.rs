use super::{Collider, DynamicLight, Projectile, Sprite, Transform};
use crate::{
    DroppedItemContext, Layer, ProjectileKind, TerrainRenderer, TilePos, World,
    terrain_renderer::{LaserParticleEmitter, LaserParticleKind},
};
use easy_gpu::assets::Material;
use easy_gpu::assets_manager::Handle;
use hecs::{Entity, World as EntityWorld};

const BOMB_SPEED: f32 = 40.0;
const BOMB_FUSE: f32 = 3.0;
const BOMB_RADIUS: u32 = 8;
const SPARK_COUNT: u32 = 64;
const SMOKE_COUNT: u32 = 72;
const BOMB_LIGHT: [f32; 3] = [0.5, 0.1, 0.0];
const SPARK_LIGHT: [f32; 3] = [1.0, 0.7, 0.12];
const LASER_PARTICLE_INTERVAL: f32 = 1.0 / 8.0;
const MAX_LASER_EMITTERS: usize = 16;
const MAX_LASER_PULSES_PER_FRAME: usize = 2;
const LASER_IMPACT_PARTICLES_PER_PULSE: usize = 8;
const LASER_DUST_PARTICLES_PER_PULSE: usize = 10;
const LASER_PARTICLE_COLOUR: [f32; 4] = [0.0, 1.0, 1.0, 1.0];
const DUST_PARTICLE_COLOUR: [f32; 4] = [0.58, 0.46, 0.34, 0.8];
const LASER_ENERGY_LIGHT: [f32; 3] = [0.0, 0.72, 0.9];
const RED_LASER_PARTICLE_COLOUR: [f32; 4] = [1.0, 0.03, 0.01, 1.0];
const RED_LASER_DUST_COLOUR: [f32; 4] = [0.78, 0.06, 0.02, 0.8];
const RED_LASER_ENERGY_LIGHT: [f32; 3] = [1.0, 0.025, 0.01];
const MAX_LASER_EMITTER_WIDTH: usize = 4;
const EXPORT_SPARK_COUNT: usize = 18;
const EXPORT_SPARK_COLOUR: [f32; 4] = [0.0, 1.0, 1.0, 1.0];
const EXPORT_SPARK_LIGHT: [f32; 3] = [0.0, 0.58, 0.82];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bomb {
    fuse: f32,
    radius: u32,
}

impl Bomb {
    pub const fn new(fuse: f32, radius: u32) -> Self {
        Self { fuse, radius }
    }

    pub const fn remaining_fuse(self) -> f32 {
        self.fuse
    }

    fn tick(&mut self, elapsed: f32) -> bool {
        self.fuse -= elapsed.max(0.0);
        self.fuse <= 0.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParticleKind {
    Spark,
    ExportSpark,
    LaserEnergy,
    RedLaserEnergy,
    Smoke,
    Dust,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    pub kind: ParticleKind,
    remaining: f32,
    lifetime: f32,
    velocity: [f32; 2],
    start_scale: f32,
    end_scale: f32,
}

impl Particle {
    pub fn new(
        kind: ParticleKind,
        lifetime: f32,
        velocity: [f32; 2],
        start_scale: f32,
        end_scale: f32,
    ) -> Self {
        Self {
            kind,
            remaining: lifetime,
            lifetime,
            velocity,
            start_scale,
            end_scale,
        }
    }

    pub fn normalized_remaining(self) -> f32 {
        (self.remaining / self.lifetime.max(f32::EPSILON)).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy)]
pub struct EffectsMaterials {
    pub projectile: Handle<Material>,
    pub particle: Handle<Material>,
}

#[derive(Clone, Copy)]
struct Explosion {
    entity: Entity,
    position: [f32; 2],
    radius: u32,
}

/// Updates bomb fuses and particles while retaining all temporary allocations.
pub struct EffectsSystem {
    explosions: Vec<Explosion>,
    expired_particles: Vec<Entity>,
    affected_tiles: Vec<TilePos>,
    random_state: u64,
    laser_particle_accumulator: f32,
}

impl Default for EffectsSystem {
    fn default() -> Self {
        Self {
            explosions: Vec::new(),
            expired_particles: Vec::new(),
            affected_tiles: Vec::new(),
            random_state: 0xB0B5_5A75_D15C_A11E,
            laser_particle_accumulator: 0.0,
        }
    }
}

impl EffectsSystem {
    pub fn update(
        &mut self,
        entities: &mut EntityWorld,
        terrain: &mut World,
        renderer: &mut TerrainRenderer,
        particle_material: Handle<Material>,
        mut dropped_items: DroppedItemContext<'_>,
        elapsed: f32,
    ) {
        let elapsed = elapsed.max(0.0);
        self.update_particles(entities, elapsed);
        self.explosions.clear();
        for (entity, (bomb, transform)) in entities.query::<(&mut Bomb, &Transform)>().iter() {
            if bomb.tick(elapsed) {
                self.explosions.push(Explosion {
                    entity,
                    position: transform.position,
                    radius: bomb.radius,
                });
            }
        }

        while let Some(explosion) = self.explosions.pop() {
            self.destroy_terrain(
                entities,
                terrain,
                renderer,
                &mut dropped_items,
                explosion.position,
                explosion.radius,
            );
            self.spawn_explosion_particles(
                entities,
                particle_material,
                explosion.position,
                explosion.radius as f32,
            );
            let _ = entities.despawn(explosion.entity);
        }
    }

    /// Emits particles only for laser bores already resident in the terrain
    /// renderer. Off-screen bores keep mining without creating invisible ECS work.
    pub fn emit_laser_particles(
        &mut self,
        entities: &mut EntityWorld,
        renderer: &TerrainRenderer,
        particle_material: Handle<Material>,
        elapsed: f32,
    ) {
        self.emit_laser_particles_from(
            entities,
            renderer.laser_particle_emitters(),
            particle_material,
            elapsed,
        );
    }

    fn emit_laser_particles_from(
        &mut self,
        entities: &mut EntityWorld,
        emitters: &[LaserParticleEmitter],
        particle_material: Handle<Material>,
        elapsed: f32,
    ) {
        if emitters.is_empty() {
            self.laser_particle_accumulator = 0.0;
            return;
        }
        let maximum_catch_up = LASER_PARTICLE_INTERVAL * MAX_LASER_PULSES_PER_FRAME as f32;
        self.laser_particle_accumulator = (self.laser_particle_accumulator
            + elapsed.clamp(0.0, maximum_catch_up))
        .min(maximum_catch_up);
        let mut pulses = 0;
        while self.laser_particle_accumulator >= LASER_PARTICLE_INTERVAL
            && pulses < MAX_LASER_PULSES_PER_FRAME
        {
            self.laser_particle_accumulator -= LASER_PARTICLE_INTERVAL;
            pulses += 1;
            for &emitter in emitters.iter().take(MAX_LASER_EMITTERS) {
                if let Some(impact) = emitter.impact {
                    let density = (emitter.width.ceil() as usize).clamp(1, MAX_LASER_EMITTER_WIDTH);
                    for _ in 0..LASER_IMPACT_PARTICLES_PER_PULSE * density {
                        let position = self.random_laser_impact(impact, emitter.width);
                        self.spawn_laser_energy_particle(
                            entities,
                            particle_material,
                            position,
                            emitter.kind,
                        );
                    }
                    for _ in 0..LASER_DUST_PARTICLES_PER_PULSE * density {
                        let position = self.random_laser_impact(impact, emitter.width);
                        self.spawn_laser_dust_particle(
                            entities,
                            particle_material,
                            position,
                            emitter.kind,
                        );
                    }
                }
            }
        }
    }

    /// Emits a fixed, bounded burst from one completed orbital shipment.
    pub fn emit_export_launch_sparks(
        &mut self,
        entities: &mut EntityWorld,
        particle_material: Handle<Material>,
        origin: [f32; 2],
    ) {
        for _ in 0..EXPORT_SPARK_COUNT {
            let horizontal = (self.random_unit() * 2.0 - 1.0) * 3.2;
            let vertical = -(8.0 + self.random_unit() * 10.0);
            let lifetime = 0.8 + self.random_unit() * 0.7;
            let start_scale = 0.32 + self.random_unit() * 0.25;
            entities.spawn((
                Particle::new(
                    ParticleKind::ExportSpark,
                    lifetime,
                    [horizontal, vertical],
                    start_scale,
                    0.06,
                ),
                DynamicLight::new(EXPORT_SPARK_LIGHT),
                Transform::new([
                    origin[0] + (self.random_unit() * 2.0 - 1.0) * 0.7,
                    origin[1],
                ])
                .with_scale([start_scale; 2]),
                Sprite::new(particle_material)
                    .with_frame(0)
                    .with_tint(EXPORT_SPARK_COLOUR)
                    .with_emissive(1.0)
                    .with_depth(0.07),
            ));
        }
    }

    fn update_particles(&mut self, entities: &mut EntityWorld, elapsed: f32) {
        self.expired_particles.clear();
        for (entity, (particle, transform, sprite)) in entities
            .query::<(&mut Particle, &mut Transform, &mut Sprite)>()
            .iter()
        {
            particle.remaining -= elapsed;
            if particle.remaining <= 0.0 {
                self.expired_particles.push(entity);
                continue;
            }
            let remaining = particle.normalized_remaining();
            let progress = 1.0 - remaining;
            let scale =
                particle.start_scale + (particle.end_scale - particle.start_scale) * progress;
            transform.scale = [scale; 2];
            sprite.tint[3] = match particle.kind {
                ParticleKind::Spark | ParticleKind::ExportSpark => remaining,
                ParticleKind::LaserEnergy | ParticleKind::RedLaserEnergy => remaining.sqrt(),
                ParticleKind::Smoke => remaining.powi(3) * 0.7,
                ParticleKind::Dust => remaining.powi(2) * 0.8,
            };

            match particle.kind {
                ParticleKind::Smoke => {
                    let damping = (-4.5 * elapsed).exp();
                    particle.velocity[0] *= damping;
                    particle.velocity[1] = particle.velocity[1] * damping - 1.8 * elapsed;
                    transform.position[0] += particle.velocity[0] * elapsed;
                    transform.position[1] += particle.velocity[1] * elapsed;
                }
                ParticleKind::Dust => {
                    let damping = (-2.5 * elapsed).exp();
                    particle.velocity[0] *= damping;
                    particle.velocity[1] = particle.velocity[1] * damping + 8.0 * elapsed;
                    transform.position[0] += particle.velocity[0] * elapsed;
                    transform.position[1] += particle.velocity[1] * elapsed;
                }
                ParticleKind::LaserEnergy
                | ParticleKind::RedLaserEnergy
                | ParticleKind::ExportSpark => {
                    let damping = (-1.8 * elapsed).exp();
                    particle.velocity[0] *= damping;
                    particle.velocity[1] *= damping;
                    transform.position[0] += particle.velocity[0] * elapsed;
                    transform.position[1] += particle.velocity[1] * elapsed;
                }
                ParticleKind::Spark => {}
            }
        }
        for (_, (particle, light)) in entities.query::<(&Particle, &mut DynamicLight)>().iter() {
            let base = match particle.kind {
                ParticleKind::Spark => SPARK_LIGHT,
                ParticleKind::ExportSpark => EXPORT_SPARK_LIGHT,
                ParticleKind::LaserEnergy => LASER_ENERGY_LIGHT,
                ParticleKind::RedLaserEnergy => RED_LASER_ENERGY_LIGHT,
                ParticleKind::Smoke | ParticleKind::Dust => continue,
            };
            let intensity = particle.normalized_remaining();
            light.colour = base.map(|channel| channel * intensity);
        }
        for entity in self.expired_particles.drain(..) {
            let _ = entities.despawn(entity);
        }
    }

    fn destroy_terrain(
        &mut self,
        entities: &mut EntityWorld,
        terrain: &mut World,
        renderer: &mut TerrainRenderer,
        dropped_items: &mut DroppedItemContext<'_>,
        position: [f32; 2],
        radius: u32,
    ) {
        collect_explosion_tiles(terrain, position, radius, &mut self.affected_tiles);
        for tile in self.affected_tiles.drain(..) {
            dropped_items.system.break_target(
                entities,
                dropped_items.material,
                dropped_items.registry,
                terrain,
                renderer,
                tile,
                Layer::Foreground,
            );
        }
    }

    fn spawn_explosion_particles(
        &mut self,
        entities: &mut EntityWorld,
        material: Handle<Material>,
        position: [f32; 2],
        power: f32,
    ) {
        for _ in 0..SPARK_COUNT {
            let angle = std::f32::consts::TAU * self.random_unit();
            let speed = power * (1.2 + self.random_unit() * 2.0);
            let lifetime = 0.3 + self.random_unit() * 0.9;
            entities.spawn((
                Particle::new(ParticleKind::Spark, lifetime, [0.0; 2], 1.0, 0.15),
                Transform::new(position).with_scale([1.0; 2]),
                Collider::new(0.12, 0.12)
                    .with_velocity([angle.cos() * speed, angle.sin() * speed])
                    .with_material(0.35, 0.75)
                    .with_gravity_scale(0.65)
                    .with_drag(0.15, 7.0),
                DynamicLight::new(SPARK_LIGHT),
                Sprite::new(material)
                    .with_frame(0)
                    .with_tint([1.0, 0.85, 0.3, 1.0])
                    .with_emissive(1.0)
                    .with_depth(0.08),
            ));
        }
        for _ in 0..SMOKE_COUNT {
            let lifetime = 0.7 + self.random_unit() * 0.8;
            let direction = std::f32::consts::TAU * self.random_unit();
            let speed = power * (1.2 + self.random_unit() * 2.0);
            let velocity = [direction.cos() * speed, direction.sin() * speed];
            let start_scale = 0.5 + self.random_unit() * 0.35;
            let end_scale = 2.6 + self.random_unit() * 0.8;
            entities.spawn((
                Particle::new(
                    ParticleKind::Smoke,
                    lifetime,
                    velocity,
                    start_scale,
                    end_scale,
                ),
                Transform::new(position).with_scale([start_scale; 2]),
                Sprite::new(material)
                    .with_frame(1)
                    .with_tint([0.72, 0.7, 0.68, 0.7])
                    .with_emissive(0.2)
                    .with_depth(0.09),
            ));
        }
    }

    fn spawn_laser_energy_particle(
        &mut self,
        entities: &mut EntityWorld,
        material: Handle<Material>,
        position: [f32; 2],
        emitter_kind: LaserParticleKind,
    ) {
        let horizontal = (self.random_unit() * 2.0 - 1.0) * 5.0;
        let vertical = -(2.5 + self.random_unit() * 5.5);
        let lifetime = 0.45 + self.random_unit() * 0.25;
        let start_scale = 0.9 + self.random_unit() * 0.3;
        let (particle_kind, colour, light) = match emitter_kind {
            LaserParticleKind::Cyan => (
                ParticleKind::LaserEnergy,
                LASER_PARTICLE_COLOUR,
                LASER_ENERGY_LIGHT,
            ),
            LaserParticleKind::Red => (
                ParticleKind::RedLaserEnergy,
                RED_LASER_PARTICLE_COLOUR,
                RED_LASER_ENERGY_LIGHT,
            ),
        };
        entities.spawn((
            Particle::new(
                particle_kind,
                lifetime,
                [horizontal, vertical],
                start_scale,
                0.2,
            ),
            DynamicLight::new(light),
            Transform::new(position).with_scale([start_scale; 2]),
            Sprite::new(material)
                .with_frame(0)
                .with_tint(colour)
                .with_emissive(1.0)
                .with_depth(0.08),
        ));
    }

    fn spawn_laser_dust_particle(
        &mut self,
        entities: &mut EntityWorld,
        material: Handle<Material>,
        position: [f32; 2],
        emitter_kind: LaserParticleKind,
    ) {
        let velocity = [
            (self.random_unit() * 2.0 - 1.0) * 5.5,
            -(3.0 + self.random_unit() * 5.0),
        ];
        let lifetime = 0.55 + self.random_unit() * 0.35;
        let start_scale = 0.3 + self.random_unit() * 0.18;
        let colour = match emitter_kind {
            LaserParticleKind::Cyan => DUST_PARTICLE_COLOUR,
            LaserParticleKind::Red => RED_LASER_DUST_COLOUR,
        };
        entities.spawn((
            Particle::new(ParticleKind::Dust, lifetime, velocity, start_scale, 0.65),
            Transform::new(position).with_scale([start_scale; 2]),
            Sprite::new(material)
                .with_frame(1)
                .with_tint(colour)
                .with_emissive(0.05)
                .with_depth(0.09),
        ));
    }

    fn random_laser_impact(&mut self, centre: [f32; 2], width: f32) -> [f32; 2] {
        [
            centre[0] + (self.random_unit() - 0.5) * width.max(0.0),
            centre[1],
        ]
    }

    fn random_unit(&mut self) -> f32 {
        let mut value = self.random_state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.random_state = value;
        (value >> 40) as f32 / (1_u32 << 24) as f32
    }
}

pub fn spawn_bomb(
    entities: &mut EntityWorld,
    material: Handle<Material>,
    origin: [f32; 2],
    target: [f32; 2],
) -> Entity {
    let direction = throw_direction(origin, target);
    entities.spawn((
        Projectile {
            kind: ProjectileKind::Bomb,
        },
        Bomb::new(BOMB_FUSE, BOMB_RADIUS),
        DynamicLight::new(BOMB_LIGHT),
        Transform::new(origin).with_scale([0.9, 0.9]),
        Collider::new(0.9, 0.9)
            .with_velocity([direction[0] * BOMB_SPEED, direction[1] * BOMB_SPEED])
            .with_material(0.35, 0.55)
            .with_angular_motion(if direction[0] < 0.0 { 10.0 } else { -10.0 }, 0.25, 0.45)
            .with_drag(0.05, 2.0),
        Sprite::new(material).with_frame(0).with_emissive(0.3),
    ))
}

fn throw_direction(origin: [f32; 2], target: [f32; 2]) -> [f32; 2] {
    let difference = [target[0] - origin[0], target[1] - origin[1]];
    let length = difference[0].hypot(difference[1]);
    if length > f32::EPSILON {
        [difference[0] / length, difference[1] / length]
    } else {
        [1.0, 0.0]
    }
}

fn collect_explosion_tiles(
    terrain: &World,
    position: [f32; 2],
    radius: u32,
    output: &mut Vec<TilePos>,
) {
    output.clear();
    let centre = [position[0].round() as i64, position[1].round() as i64];
    let radius = i64::from(radius);
    for offset_y in -radius..=radius {
        for offset_x in -radius..=radius {
            if offset_x * offset_x + offset_y * offset_y > radius * radius {
                continue;
            }
            let x = centre[0] + offset_x;
            let y = centre[1] + offset_y;
            if x >= 0 && y >= 0 && x < i64::from(terrain.width()) && y < i64::from(terrain.height())
            {
                output.push(TilePos::new(x as u32, y as u32));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::marker::PhantomData;

    fn test_material() -> Handle<Material> {
        Handle {
            index: 0,
            generation: 0,
            _marker: PhantomData,
        }
    }

    #[test]
    fn explosion_circle_is_clipped_to_the_world() {
        let terrain = World::empty(10, 10, 0).unwrap();
        let mut tiles = Vec::new();
        collect_explosion_tiles(&terrain, [0.0, 0.0], 2, &mut tiles);
        assert!(tiles.contains(&TilePos::new(0, 0)));
        assert!(tiles.contains(&TilePos::new(2, 0)));
        assert!(!tiles.iter().any(|tile| tile.x >= 10 || tile.y >= 10));
    }

    #[test]
    fn bomb_fuse_is_configurable() {
        let mut bomb = Bomb::new(3.0, 8);
        assert_eq!(bomb.remaining_fuse(), 3.0);
        assert!(!bomb.tick(2.9));
        assert!(bomb.tick(0.1));
    }

    #[test]
    fn laser_emitter_spawns_only_dense_energy_and_dust_at_the_impact() {
        let mut entities = EntityWorld::new();
        let mut system = EffectsSystem::default();
        system.emit_laser_particles_from(
            &mut entities,
            &[LaserParticleEmitter {
                impact: Some([4.0, 12.5]),
                width: 1.0,
                kind: LaserParticleKind::Cyan,
            }],
            test_material(),
            LASER_PARTICLE_INTERVAL,
        );

        assert_eq!(
            entities.len() as usize,
            LASER_IMPACT_PARTICLES_PER_PULSE + LASER_DUST_PARTICLES_PER_PULSE
        );
        assert_eq!(entities.query::<&Collider>().iter().count(), 0);
        let mut energy_particles = 0;
        let mut dust = 0;
        for (_, (particle, sprite)) in entities.query::<(&Particle, &Sprite)>().iter() {
            match particle.kind {
                ParticleKind::LaserEnergy => {
                    energy_particles += 1;
                    assert_eq!(sprite.tint, LASER_PARTICLE_COLOUR);
                    assert_eq!(sprite.frame, 0);
                }
                ParticleKind::RedLaserEnergy => {
                    panic!("cyan laser bore should not emit red energy")
                }
                ParticleKind::Dust => {
                    dust += 1;
                    assert_eq!(sprite.tint, DUST_PARTICLE_COLOUR);
                    assert_eq!(sprite.frame, 1);
                }
                ParticleKind::Spark | ParticleKind::ExportSpark | ParticleKind::Smoke => {
                    panic!("laser bore should only emit laser energy and dust")
                }
            }
        }
        assert_eq!(energy_particles, LASER_IMPACT_PARTICLES_PER_PULSE);
        assert_eq!(dust, LASER_DUST_PARTICLES_PER_PULSE);
    }

    #[test]
    fn laser_particle_work_is_capped_after_a_long_frame() {
        let mut entities = EntityWorld::new();
        let mut system = EffectsSystem::default();
        let emitters = vec![
            LaserParticleEmitter {
                impact: Some([1.0, 1.0]),
                width: 1.0,
                kind: LaserParticleKind::Cyan,
            };
            MAX_LASER_EMITTERS + 8
        ];

        system.emit_laser_particles_from(&mut entities, &emitters, test_material(), 10.0);

        assert_eq!(
            entities.len() as usize,
            MAX_LASER_EMITTERS
                * MAX_LASER_PULSES_PER_FRAME
                * (LASER_IMPACT_PARTICLES_PER_PULSE + LASER_DUST_PARTICLES_PER_PULSE)
        );
    }

    #[test]
    fn wide_red_laser_emitter_spawns_red_particles_and_lights_across_its_width() {
        let mut entities = EntityWorld::new();
        let mut system = EffectsSystem::default();
        system.emit_laser_particles_from(
            &mut entities,
            &[LaserParticleEmitter {
                impact: Some([12.5, 11.5]),
                width: 4.0,
                kind: LaserParticleKind::Red,
            }],
            test_material(),
            LASER_PARTICLE_INTERVAL,
        );

        assert_eq!(
            entities.len() as usize,
            MAX_LASER_EMITTER_WIDTH
                * (LASER_IMPACT_PARTICLES_PER_PULSE + LASER_DUST_PARTICLES_PER_PULSE)
        );
        let mut energy = 0;
        let mut dust = 0;
        for (_, (particle, transform, sprite)) in
            entities.query::<(&Particle, &Transform, &Sprite)>().iter()
        {
            assert!((10.5..=14.5).contains(&transform.position[0]));
            match particle.kind {
                ParticleKind::RedLaserEnergy => {
                    energy += 1;
                    assert_eq!(sprite.tint, RED_LASER_PARTICLE_COLOUR);
                }
                ParticleKind::Dust => {
                    dust += 1;
                    assert_eq!(sprite.tint, RED_LASER_DUST_COLOUR);
                }
                _ => panic!("red shaft bore emitted a particle with the wrong palette"),
            }
        }
        for (_, light) in entities.query::<&DynamicLight>().iter() {
            assert_eq!(light.colour, RED_LASER_ENERGY_LIGHT);
        }
        assert_eq!(energy, LASER_IMPACT_PARTICLES_PER_PULSE * 4);
        assert_eq!(dust, LASER_DUST_PARTICLES_PER_PULSE * 4);
    }

    #[test]
    fn export_launch_sparks_are_bounded_glowing_and_upward() {
        let mut entities = EntityWorld::new();
        let mut system = EffectsSystem::default();
        system.emit_export_launch_sparks(&mut entities, test_material(), [8.0, 5.0]);

        assert_eq!(entities.len() as usize, EXPORT_SPARK_COUNT);
        for (_, (particle, light, sprite)) in entities
            .query::<(&Particle, &DynamicLight, &Sprite)>()
            .iter()
        {
            assert_eq!(particle.kind, ParticleKind::ExportSpark);
            assert!(particle.velocity[1] < 0.0);
            assert_eq!(light.colour, EXPORT_SPARK_LIGHT);
            assert_eq!(sprite.tint, EXPORT_SPARK_COLOUR);
            assert_eq!(sprite.emissive, 1.0);
        }
    }
}
