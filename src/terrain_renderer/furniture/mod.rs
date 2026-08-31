//! Instanced furniture, cable, and machine-effect rendering.

use super::lighting::{LightSource, LightingEngine};
use crate::{
    ChunkPos, FurnitureObject, FurnitureSupport, ItemTransportShape, ObjectId, PowerConnection,
    PowerSystem, TilePos, World, WorldObject, furniture_definition, item_transport_shape,
    render_common,
};
use easy_gpu::assets::{
    Buffer, BufferLayout, GpuInstance, GpuVertex, Material, MaterialBuilder, Mesh,
    RenderPipelineBuilder, SamplerBuilder, render_texture, render_uniform, sampler,
};
use easy_gpu::assets_manager::Handle;
use easy_gpu::frame::Frame;
use easy_gpu::wgpu::{BlendState, FilterMode, TextureFormat, VertexFormat, VertexStepMode};
use std::collections::HashSet;

const FURNITURE_ATLAS_COLUMNS: u16 = 13;
const FURNITURE_ATLAS_ROWS: u16 = 1;
const FURNITURE_GROUND_INSET: f32 = 0.20;
const LASER_BEAM_WIDTH: f32 = 0.28;
const LASER_LIGHT_COLOUR: [f32; 3] = [0.0, 1.0, 1.0];
const RED_LASER_LIGHT_COLOUR: [f32; 3] = [1.0, 0.03, 0.01];
const PYLON_LIGHT_COLOUR: [f32; 3] = [0.0, 0.22, 0.28];
const BATTERY_LIGHT_COLOUR: [f32; 3] = [0.0, 0.16, 0.20];
const CABLE_SEGMENTS: usize = 10;
const CABLE_WIDTH: f32 = 0.09;
const CABLE_MIN_SAG: f32 = 0.12;
const CABLE_MAX_SAG: f32 = 0.55;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct FurnitureInstance {
    position: [f32; 3],
    size: [f32; 2],
    uv_rect: [f32; 4],
    visual_kind: u32,
}

impl GpuInstance for FurnitureInstance {
    fn buffer_layout() -> BufferLayout {
        BufferLayout::new()
            .stride(size_of::<Self>() as u64)
            .step_mode(VertexStepMode::Instance)
            .attribute(1, 0, VertexFormat::Float32x3)
            .attribute(2, 12, VertexFormat::Float32x2)
            .attribute(3, 20, VertexFormat::Float32x4)
            .attribute(4, 36, VertexFormat::Uint32)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct LaserBeamInstance {
    position: [f32; 3],
    size: [f32; 2],
    direction: [f32; 2],
    beam_kind: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct CableSegmentInstance {
    position: [f32; 3],
    size: [f32; 2],
    direction: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LaserParticleEmitter {
    pub(crate) impact: Option<[f32; 2]>,
    pub(crate) width: f32,
    pub(crate) kind: LaserParticleKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LaserParticleKind {
    Cyan,
    Red,
}

impl LaserParticleEmitter {
    const fn cyan(impact: Option<[f32; 2]>) -> Self {
        Self {
            impact,
            width: 1.0,
            kind: LaserParticleKind::Cyan,
        }
    }

    const fn red(impact: Option<[f32; 2]>, width: f32) -> Self {
        Self {
            impact,
            width,
            kind: LaserParticleKind::Red,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FurnitureLightKind {
    Laser,
    RedLaser,
    Pylon,
    Battery,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FurnitureLightSpec {
    position: [f32; 2],
    phase: f32,
    kind: FurnitureLightKind,
}

impl GpuInstance for LaserBeamInstance {
    fn buffer_layout() -> BufferLayout {
        BufferLayout::new()
            .stride(size_of::<Self>() as u64)
            .step_mode(VertexStepMode::Instance)
            .attribute(1, 0, VertexFormat::Float32x3)
            .attribute(2, 12, VertexFormat::Float32x2)
            .attribute(3, 20, VertexFormat::Float32x2)
            .attribute(4, 28, VertexFormat::Uint32)
    }
}

impl GpuInstance for CableSegmentInstance {
    fn buffer_layout() -> BufferLayout {
        BufferLayout::new()
            .stride(size_of::<Self>() as u64)
            .step_mode(VertexStepMode::Instance)
            .attribute(1, 0, VertexFormat::Float32x3)
            .attribute(2, 12, VertexFormat::Float32x2)
            .attribute(3, 20, VertexFormat::Float32x2)
    }
}

pub(super) struct FurnitureRenderer {
    material: Handle<Material>,
    laser_material: Handle<Material>,
    cable_material: Handle<Material>,
    quad: Handle<Mesh>,
    instances: Vec<FurnitureInstance>,
    laser_instances: Vec<LaserBeamInstance>,
    cable_instances: Vec<CableSegmentInstance>,
    laser_light_specs: Vec<FurnitureLightSpec>,
    power_light_specs: Vec<FurnitureLightSpec>,
    flickering_lights: Vec<LightSource>,
    laser_particle_emitters: Vec<LaserParticleEmitter>,
    seen_objects: HashSet<ObjectId>,
    seen_connections: HashSet<[ObjectId; 2]>,
}

impl FurnitureRenderer {
    pub(super) fn new(
        gpu: &mut easy_gpu::Renderer,
        camera_buffer: Handle<Buffer>,
        lighting: &LightingEngine,
    ) -> Self {
        let quad = render_common::create_unit_quad(gpu);
        let shader = gpu.load_shader(include_str!("../furniture.wgsl"));
        let pipeline = RenderPipelineBuilder::new(shader)
            .material_layout(&[
                render_uniform(0),
                render_texture(1),
                sampler(2),
                render_texture(3),
                sampler(4),
                render_uniform(5),
            ])
            .vertex_layout(render_common::QuadVertex::buffer_layout())
            .vertex_layout(FurnitureInstance::buffer_layout())
            .depth_format(TextureFormat::Depth24Plus)
            .blend_mode(BlendState::REPLACE)
            .build(gpu);
        let texture = gpu.load_texture_from_file(
            include_bytes!("../../../assets/furniture/furniture_with_power.png").to_vec(),
        );
        let texture_sampler = SamplerBuilder::new()
            .filter_mode(FilterMode::Nearest)
            .build(gpu);
        let material = MaterialBuilder::new(pipeline)
            .buffer(0, camera_buffer)
            .texture(1, texture)
            .sampler(2, texture_sampler)
            .texture(3, lighting.light_texture)
            .sampler(4, lighting.light_sampler)
            .buffer(5, lighting.light_meta_buffer)
            .build(gpu);
        let laser_pipeline =
            RenderPipelineBuilder::new(gpu.load_shader(include_str!("../laser_bore.wgsl")))
                .material_layout(&[render_uniform(0), render_texture(1), sampler(2)])
                .vertex_layout(render_common::QuadVertex::buffer_layout())
                .vertex_layout(LaserBeamInstance::buffer_layout())
                .depth_format(TextureFormat::Depth24Plus)
                .blend_mode(BlendState::ALPHA_BLENDING)
                .build(gpu);
        let laser_texture = gpu.load_texture_from_file(
            include_bytes!("../../../assets/effects/laser_beams.png").to_vec(),
        );
        let laser_sampler = SamplerBuilder::new()
            .filter_mode(FilterMode::Linear)
            .build(gpu);
        let laser_material = MaterialBuilder::new(laser_pipeline)
            .buffer(0, camera_buffer)
            .texture(1, laser_texture)
            .sampler(2, laser_sampler)
            .build(gpu);
        let cable_pipeline =
            RenderPipelineBuilder::new(gpu.load_shader(include_str!("../power_cable.wgsl")))
                .material_layout(&[
                    render_uniform(0),
                    render_texture(1),
                    sampler(2),
                    render_uniform(3),
                ])
                .vertex_layout(render_common::QuadVertex::buffer_layout())
                .vertex_layout(CableSegmentInstance::buffer_layout())
                .depth_format(TextureFormat::Depth24Plus)
                .blend_mode(BlendState::REPLACE)
                .build(gpu);
        let cable_material = MaterialBuilder::new(cable_pipeline)
            .buffer(0, camera_buffer)
            .texture(1, lighting.light_texture)
            .sampler(2, lighting.light_sampler)
            .buffer(3, lighting.light_meta_buffer)
            .build(gpu);
        Self {
            material,
            laser_material,
            cable_material,
            quad,
            instances: Vec::new(),
            laser_instances: Vec::new(),
            cable_instances: Vec::new(),
            laser_light_specs: Vec::new(),
            power_light_specs: Vec::new(),
            flickering_lights: Vec::new(),
            laser_particle_emitters: Vec::new(),
            seen_objects: HashSet::new(),
            seen_connections: HashSet::new(),
        }
    }

    pub(super) fn sync(
        &mut self,
        world: &World,
        power: &PowerSystem,
        loaded_chunks: impl Iterator<Item = ChunkPos>,
    ) {
        self.instances.clear();
        self.laser_instances.clear();
        self.cable_instances.clear();
        self.laser_light_specs.clear();
        self.power_light_specs.clear();
        self.laser_particle_emitters.clear();
        self.seen_objects.clear();
        self.seen_connections.clear();
        let loaded_chunks: Vec<_> = loaded_chunks.collect();
        for &chunk in &loaded_chunks {
            for object in world.objects_in_chunk(chunk) {
                if furniture_definition(object.object_type()).is_some()
                    && self.seen_objects.insert(object.id())
                {
                    append_furniture_instance(world, object, &mut self.instances);
                    append_laser_bore(
                        world,
                        power,
                        object,
                        &mut self.laser_instances,
                        &mut self.laser_light_specs,
                        &mut self.laser_particle_emitters,
                    );
                    append_power_indicator_light(power, object, &mut self.power_light_specs);
                }
            }
        }
        for chunk in loaded_chunks {
            for connection in power.connections_in_chunk(chunk) {
                if self.seen_connections.insert(connection.endpoints()) {
                    append_power_cable(*connection, &mut self.cable_instances);
                }
            }
        }
    }

    pub(super) fn sync_laser_beams(
        &mut self,
        world: &World,
        power: &PowerSystem,
        loaded_chunks: impl Iterator<Item = ChunkPos>,
    ) {
        self.laser_instances.clear();
        self.laser_light_specs.clear();
        self.laser_particle_emitters.clear();
        self.seen_objects.clear();
        for chunk in loaded_chunks {
            for object in world.objects_in_chunk(chunk) {
                if self.seen_objects.insert(object.id()) {
                    append_laser_bore(
                        world,
                        power,
                        object,
                        &mut self.laser_instances,
                        &mut self.laser_light_specs,
                        &mut self.laser_particle_emitters,
                    );
                }
            }
        }
    }

    pub(super) fn update_flickering_lights(&mut self, time_seconds: f32) {
        self.flickering_lights.clear();
        self.flickering_lights
            .reserve(self.power_light_specs.len() + self.laser_light_specs.len());
        self.flickering_lights.extend(
            self.power_light_specs
                .iter()
                .map(|spec| light_source(*spec, time_seconds)),
        );
        self.flickering_lights.extend(
            self.laser_light_specs
                .iter()
                .map(|spec| light_source(*spec, time_seconds)),
        );
    }

    pub(super) fn flickering_lights(&self) -> &[LightSource] {
        &self.flickering_lights
    }

    pub(super) fn laser_particle_emitters(&self) -> &[LaserParticleEmitter] {
        &self.laser_particle_emitters
    }

    pub(super) fn draw(&self, frame: &mut Frame) {
        if !self.cable_instances.is_empty() {
            frame.draw_batch(&self.cable_instances, self.cable_material, self.quad);
        }
        if !self.laser_instances.is_empty() {
            frame.draw_batch(&self.laser_instances, self.laser_material, self.quad);
        }
        if !self.instances.is_empty() {
            frame.draw_batch(&self.instances, self.material, self.quad);
        }
    }
}

fn append_laser_bore(
    world: &World,
    power: &PowerSystem,
    object: &WorldObject,
    instances: &mut Vec<LaserBeamInstance>,
    lights: &mut Vec<FurnitureLightSpec>,
    particle_emitters: &mut Vec<LaserParticleEmitter>,
) {
    if let Some(beam) = world.laser_bore_beam(object, power.is_powered(object.id())) {
        let start = beam.first_y as f32 - 0.5 + FURNITURE_GROUND_INSET;
        let end = beam.first_y as f32 - 0.5 + beam.length_tiles as f32;
        let length = end - start;
        particle_emitters.push(LaserParticleEmitter::cyan(
            beam.target
                .map(|target| [beam.x as f32, target.y as f32 - 0.5]),
        ));
        if length > 0.0 {
            append_tiled_laser_beam(
                instances,
                [beam.x as f32, start],
                [beam.x as f32, end],
                LASER_BEAM_WIDTH,
                0,
            );
            lights.reserve(beam.length_tiles as usize);
            let phase = flicker_phase(object.anchor());
            lights.extend((0..beam.length_tiles).map(|offset| FurnitureLightSpec {
                position: [beam.x as f32, (beam.first_y + offset) as f32],
                phase,
                kind: FurnitureLightKind::Laser,
            }));
        }
        return;
    }
    if let Some(beam) = world.laser_drill_beam(object, power.is_powered(object.id())) {
        particle_emitters.push(LaserParticleEmitter::cyan(
            beam.target
                .map(|target| [target.x as f32, target.y as f32 - 0.5]),
        ));
        if beam.origin != beam.endpoint {
            let [direction_x, direction_y] = beam.aim.direction().map(|value| value as f32);
            let direction_length = direction_x.hypot(direction_y);
            let origin = [
                beam.origin[0] + direction_x / direction_length * FURNITURE_GROUND_INSET,
                beam.origin[1] + direction_y / direction_length * FURNITURE_GROUND_INSET,
            ];
            append_tiled_laser_beam(instances, origin, beam.endpoint, LASER_BEAM_WIDTH, 0);
            let phase = flicker_phase(object.anchor());
            lights.reserve(beam.steps as usize);
            lights.extend((0..beam.steps).filter_map(|step| {
                let offset = beam.aim.tile_offset(step);
                let x = i64::from(beam.first_tile.x) + i64::from(offset[0]);
                let y = i64::from(beam.first_tile.y) + i64::from(offset[1]);
                (x >= 0 && y >= 0).then_some(FurnitureLightSpec {
                    position: [x as f32, y as f32],
                    phase,
                    kind: FurnitureLightKind::Laser,
                })
            }));
        }
        return;
    }
    let Some(beam) = world.red_shaft_bore_beam(object, power.is_powered(object.id())) else {
        return;
    };
    let start = beam.first_y as f32 - 0.5 + FURNITURE_GROUND_INSET;
    let end = beam.first_y as f32 - 0.5 + beam.length_tiles as f32;
    let length = end - start;
    let centre_x = beam.first_x as f32 + (beam.width as f32 - 1.0) * 0.5;
    particle_emitters.push(LaserParticleEmitter::red(
        beam.target_y.map(|y| [centre_x, y as f32 - 0.5]),
        beam.width as f32,
    ));
    if length <= 0.0 {
        return;
    }
    append_tiled_laser_beam(
        instances,
        [centre_x, start],
        [centre_x, end],
        beam.width as f32,
        1,
    );
    lights.reserve(beam.length_tiles as usize);
    let phase = flicker_phase(object.anchor());
    lights.extend((0..beam.length_tiles).map(|offset| FurnitureLightSpec {
        position: [centre_x, (beam.first_y + offset) as f32],
        phase,
        kind: FurnitureLightKind::RedLaser,
    }));
}

fn laser_beam_instance(
    origin: [f32; 2],
    endpoint: [f32; 2],
    width: f32,
    beam_kind: u32,
) -> LaserBeamInstance {
    let render_delta = [endpoint[0] - origin[0], origin[1] - endpoint[1]];
    let length = render_delta[0].hypot(render_delta[1]);
    let direction = if length > f32::EPSILON {
        [render_delta[0] / length, render_delta[1] / length]
    } else {
        [0.0, -1.0]
    };
    LaserBeamInstance {
        position: [
            (origin[0] + endpoint[0]) * 0.5,
            -(origin[1] + endpoint[1]) * 0.5,
            0.30,
        ],
        size: [width, length],
        direction,
        beam_kind,
    }
}

fn append_tiled_laser_beam(
    instances: &mut Vec<LaserBeamInstance>,
    origin: [f32; 2],
    endpoint: [f32; 2],
    width: f32,
    beam_kind: u32,
) {
    let delta = [endpoint[0] - origin[0], endpoint[1] - origin[1]];
    let length = delta[0].hypot(delta[1]);
    if length <= f32::EPSILON {
        return;
    }
    let direction = [delta[0] / length, delta[1] / length];
    instances.reserve(length.ceil() as usize);
    let mut distance = 0.0;
    while distance < length {
        let next_distance = (distance + 1.0).min(length);
        let segment_origin = [
            origin[0] + direction[0] * distance,
            origin[1] + direction[1] * distance,
        ];
        let segment_endpoint = [
            origin[0] + direction[0] * next_distance,
            origin[1] + direction[1] * next_distance,
        ];
        instances.push(laser_beam_instance(
            segment_origin,
            segment_endpoint,
            width,
            beam_kind,
        ));
        distance = next_distance;
    }
}

fn append_power_indicator_light(
    power: &PowerSystem,
    object: &WorldObject,
    lights: &mut Vec<FurnitureLightSpec>,
) {
    if !power.is_powered(object.id()) {
        return;
    }
    let anchor = object.anchor();
    let (position, kind) = match object.object_type() {
        FurnitureObject::PYLON | FurnitureObject::POWER_CONNECTOR => (
            [anchor.x as f32, anchor.y as f32],
            FurnitureLightKind::Pylon,
        ),
        FurnitureObject::BATTERY => (
            [anchor.x as f32 + 0.5, anchor.y as f32 + 0.5],
            FurnitureLightKind::Battery,
        ),
        _ => return,
    };
    lights.push(FurnitureLightSpec {
        position,
        phase: flicker_phase(anchor),
        kind,
    });
}

fn light_source(spec: FurnitureLightSpec, time_seconds: f32) -> LightSource {
    let intensity = match spec.kind {
        FurnitureLightKind::Laser | FurnitureLightKind::RedLaser => {
            laser_light_intensity(time_seconds, spec.phase)
        }
        FurnitureLightKind::Pylon => pylon_light_intensity(time_seconds, spec.phase),
        FurnitureLightKind::Battery => battery_light_intensity(time_seconds, spec.phase),
    };
    let base = match spec.kind {
        FurnitureLightKind::Laser => LASER_LIGHT_COLOUR,
        FurnitureLightKind::RedLaser => RED_LASER_LIGHT_COLOUR,
        FurnitureLightKind::Pylon => PYLON_LIGHT_COLOUR,
        FurnitureLightKind::Battery => BATTERY_LIGHT_COLOUR,
    };
    LightSource::new(spec.position, base.map(|channel| channel * intensity))
}

fn laser_light_intensity(time_seconds: f32, phase: f32) -> f32 {
    let slow = (time_seconds * 6.1 + phase).sin() * 0.055;
    let shimmer = (time_seconds * 13.7 + phase * 1.83).sin() * 0.018;
    (0.92 + slow + shimmer).clamp(0.82, 1.0)
}

fn pylon_light_intensity(time_seconds: f32, phase: f32) -> f32 {
    let drift = (time_seconds * 1.15 + phase).sin() * 0.035;
    let occasional = ((time_seconds * 0.43 + phase * 0.71).sin() * 0.5 + 0.5).powi(10);
    (0.86 + drift - occasional * 0.09).clamp(0.72, 0.91)
}

fn battery_light_intensity(time_seconds: f32, phase: f32) -> f32 {
    (0.82 + (time_seconds * 0.7 + phase).sin() * 0.025).clamp(0.79, 0.85)
}

fn flicker_phase(anchor: TilePos) -> f32 {
    let hash =
        anchor.x.wrapping_mul(0x9E37_79B9).rotate_left(13) ^ anchor.y.wrapping_mul(0x85EB_CA6B);
    hash as f32 * (std::f32::consts::TAU / u32::MAX as f32)
}

fn append_power_cable(connection: PowerConnection, output: &mut Vec<CableSegmentInstance>) {
    let start = connection.start();
    let end = connection.end();
    let delta = [end[0] - start[0], end[1] - start[1]];
    let distance = (delta[0] * delta[0] + delta[1] * delta[1]).sqrt();
    if distance <= f32::EPSILON {
        return;
    }
    let sag = (distance * 0.04).clamp(CABLE_MIN_SAG, CABLE_MAX_SAG);
    let point = |t: f32| {
        let sag_offset = 4.0 * sag * t * (1.0 - t);
        [
            start[0] + delta[0] * t,
            start[1] + delta[1] * t + sag_offset,
        ]
    };
    output.reserve(CABLE_SEGMENTS);
    for segment in 0..CABLE_SEGMENTS {
        let first = point(segment as f32 / CABLE_SEGMENTS as f32);
        let second = point((segment + 1) as f32 / CABLE_SEGMENTS as f32);
        let render_delta = [second[0] - first[0], -(second[1] - first[1])];
        let length = (render_delta[0] * render_delta[0] + render_delta[1] * render_delta[1]).sqrt();
        if length <= f32::EPSILON {
            continue;
        }
        output.push(CableSegmentInstance {
            position: [
                (first[0] + second[0]) * 0.5,
                -(first[1] + second[1]) * 0.5,
                0.28,
            ],
            size: [length + CABLE_WIDTH * 0.35, CABLE_WIDTH],
            direction: [render_delta[0] / length, render_delta[1] / length],
        });
    }
}

fn append_furniture_instance(
    world: &World,
    object: &WorldObject,
    output: &mut Vec<FurnitureInstance>,
) {
    let Some(definition) = furniture_definition(object.object_type()) else {
        return;
    };
    let anchor = object.anchor();
    let [width, height] = object.size();
    if object.object_type() == FurnitureObject::POWERED_CABLE_ANCHOR {
        output.push(FurnitureInstance {
            position: [anchor.x as f32, -(anchor.y as f32), 0.25],
            size: [1.0, 1.0],
            uv_rect: [0.0; 4],
            visual_kind: 1,
        });
        return;
    }
    if object.object_type() == FurnitureObject::POWER_CONNECTOR {
        output.push(FurnitureInstance {
            position: [anchor.x as f32, -(anchor.y as f32), 0.25],
            size: [1.0, 1.0],
            uv_rect: [0.0; 4],
            visual_kind: 6,
        });
        return;
    }
    if object.object_type() == FurnitureObject::COMPOSITE_ASSEMBLER {
        output.push(FurnitureInstance {
            position: [
                anchor.x as f32 + 1.0,
                -(anchor.y as f32 + 0.5 + FURNITURE_GROUND_INSET),
                0.25,
            ],
            size: [3.0, 2.0],
            uv_rect: [0.0; 4],
            visual_kind: 7,
        });
        return;
    }
    if object.object_type() == FurnitureObject::RED_SHAFT_BORE {
        output.push(FurnitureInstance {
            position: [
                anchor.x as f32 + 2.5,
                -(anchor.y as f32 + 1.0 + FURNITURE_GROUND_INSET),
                0.25,
            ],
            size: [6.0, 3.0],
            uv_rect: [0.0; 4],
            visual_kind: 8,
        });
        return;
    }
    if object.object_type() == FurnitureObject::PROCUREMENT_TERMINAL {
        output.push(FurnitureInstance {
            position: [
                anchor.x as f32 + 0.5,
                -(anchor.y as f32 + 0.5 + FURNITURE_GROUND_INSET),
                0.25,
            ],
            size: [2.0, 2.0],
            uv_rect: [0.0; 4],
            visual_kind: 9,
        });
        return;
    }
    if object.object_type() == FurnitureObject::LASER_DRILL {
        let aim = world.laser_drill_aim(object.id()).unwrap_or_default();
        output.push(FurnitureInstance {
            position: [
                anchor.x as f32 + 1.0,
                -(anchor.y as f32 + 0.5 + FURNITURE_GROUND_INSET),
                0.25,
            ],
            size: [3.0, 2.0],
            uv_rect: [0.0; 4],
            visual_kind: 10 + u32::from(aim.raw()),
        });
        return;
    }
    if object.object_type() == FurnitureObject::AMMO_TURRET {
        let facing = world.furniture_facing(object.id()).unwrap_or_default();
        output.push(FurnitureInstance {
            position: [
                anchor.x as f32 + 0.5,
                -(anchor.y as f32 + 0.5 + FURNITURE_GROUND_INSET),
                0.25,
            ],
            size: [2.0, 2.0],
            uv_rect: [0.0; 4],
            visual_kind: if facing == crate::FurnitureFacing::Right {
                17
            } else {
                18
            },
        });
        return;
    }
    if object.object_type() == FurnitureObject::DIRECTIONAL_SENTRY {
        let facing = world.furniture_facing(object.id()).unwrap_or_default();
        output.push(FurnitureInstance {
            position: [anchor.x as f32, -(anchor.y as f32), 0.25],
            size: [1.0, 1.0],
            uv_rect: [0.0; 4],
            visual_kind: if facing == crate::FurnitureFacing::Right {
                19
            } else {
                20
            },
        });
        return;
    }
    if object.object_type() == FurnitureObject::SPIKES {
        output.push(FurnitureInstance {
            position: [anchor.x as f32, -(anchor.y as f32), 0.25],
            size: [1.0, 1.0],
            uv_rect: [0.0; 4],
            visual_kind: 21,
        });
        return;
    }
    if object.object_type() == FurnitureObject::DOOR {
        output.push(FurnitureInstance {
            position: [
                anchor.x as f32,
                -(anchor.y as f32 + 1.0 + FURNITURE_GROUND_INSET),
                0.25,
            ],
            size: [1.0, 3.0],
            uv_rect: [0.0; 4],
            visual_kind: if object.is_active() { 23 } else { 22 },
        });
        return;
    }
    if object.object_type() == FurnitureObject::BED {
        output.push(FurnitureInstance {
            position: [
                anchor.x as f32 + 0.5,
                -(anchor.y as f32 + FURNITURE_GROUND_INSET),
                0.25,
            ],
            size: [2.0, 1.0],
            uv_rect: [0.0; 4],
            visual_kind: 24,
        });
        return;
    }
    if object.object_type() == FurnitureObject::SUBSURFACE_SURVEYOR {
        output.push(FurnitureInstance {
            position: [
                anchor.x as f32 + 1.0,
                -(anchor.y as f32 + 0.5 + FURNITURE_GROUND_INSET),
                0.25,
            ],
            size: [3.0, 2.0],
            uv_rect: [0.0; 4],
            visual_kind: 25,
        });
        return;
    }
    if object.object_type() == FurnitureObject::CARGO_LIFT {
        let cable_x = object
            .linked_object()
            .and_then(|cable| world.object(cable))
            .map_or(anchor.x, |cable| cable.anchor().x);
        output.push(FurnitureInstance {
            position: [
                anchor.x as f32 + 0.5,
                -(object.motion_position_tiles() + 0.5),
                0.24,
            ],
            size: [2.0, 2.0],
            uv_rect: [0.0; 4],
            visual_kind: if cable_x < anchor.x { 2 } else { 3 },
        });
        return;
    }
    if object.object_type() == FurnitureObject::LIFT_STATION {
        let cable_x = object
            .linked_object()
            .and_then(|cable| world.object(cable))
            .map_or(anchor.x, |cable| cable.anchor().x);
        output.push(FurnitureInstance {
            position: [
                anchor.x as f32 + 0.5,
                -(anchor.y as f32 + 0.5 + FURNITURE_GROUND_INSET),
                0.25,
            ],
            size: [2.0, 2.0],
            uv_rect: [0.0; 4],
            visual_kind: if cable_x < anchor.x { 4 } else { 5 },
        });
        return;
    }
    let ground_inset = match definition.support() {
        FurnitureSupport::Floor | FurnitureSupport::FloorEdges => FURNITURE_GROUND_INSET,
        FurnitureSupport::Side | FurnitureSupport::Free => 0.0,
    };
    let sprite_frame =
        item_transport_shape(world, object.id()).map_or(definition.sprite_frame(), |shape| {
            match shape {
                ItemTransportShape::Horizontal => 4,
                ItemTransportShape::Vertical => 5,
                ItemTransportShape::NorthEast => 6,
                ItemTransportShape::SouthEast => 7,
                ItemTransportShape::SouthWest => 8,
                ItemTransportShape::NorthWest => 9,
            }
        });
    let Some(mut uv_rect) =
        atlas_frame_uv(sprite_frame, FURNITURE_ATLAS_COLUMNS, FURNITURE_ATLAS_ROWS)
    else {
        return;
    };
    if definition.supports_facing()
        && world.furniture_facing(object.id()) == Some(crate::FurnitureFacing::Left)
    {
        uv_rect[0] += uv_rect[2];
        uv_rect[2] = -uv_rect[2];
    }
    output.push(FurnitureInstance {
        position: [
            anchor.x as f32 + (f32::from(width) - 1.0) * 0.5,
            -(anchor.y as f32 + (f32::from(height) - 1.0) * 0.5 + ground_inset),
            0.25,
        ],
        size: [f32::from(width), f32::from(height)],
        uv_rect,
        visual_kind: 0,
    });
}

fn atlas_frame_uv(frame: u16, columns: u16, rows: u16) -> Option<[f32; 4]> {
    if columns == 0 || rows == 0 || frame >= columns.checked_mul(rows)? {
        return None;
    }
    let column = frame % columns;
    let row = frame / columns;
    let uv_size = [1.0 / f32::from(columns), 1.0 / f32::from(rows)];
    Some([
        f32::from(column) * uv_size[0],
        f32::from(row) * uv_size[1],
        uv_size[0],
        uv_size[1],
    ])
}

#[cfg(test)]
mod tests;
