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
const LASER_BEAM_WIDTH: f32 = 0.12;
const LASER_LIGHT_COLOUR: [f32; 3] = [0.0, 1.0, 1.0];
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FurnitureLightKind {
    Laser,
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
        let shader = gpu.load_shader(include_str!("furniture.wgsl"));
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
            include_bytes!("../../assets/furniture/furniture_with_power.png").to_vec(),
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
            RenderPipelineBuilder::new(gpu.load_shader(include_str!("laser_bore.wgsl")))
                .material_layout(&[render_uniform(0)])
                .vertex_layout(render_common::QuadVertex::buffer_layout())
                .vertex_layout(LaserBeamInstance::buffer_layout())
                .depth_format(TextureFormat::Depth24Plus)
                .blend_mode(BlendState::REPLACE)
                .build(gpu);
        let laser_material = MaterialBuilder::new(laser_pipeline)
            .buffer(0, camera_buffer)
            .build(gpu);
        let cable_pipeline =
            RenderPipelineBuilder::new(gpu.load_shader(include_str!("power_cable.wgsl")))
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
    let Some(beam) = world.laser_bore_beam(object, power.is_powered(object.id())) else {
        return;
    };
    let start = beam.first_y as f32 - 0.5 + FURNITURE_GROUND_INSET;
    let end = beam.first_y as f32 - 0.5 + beam.length_tiles as f32;
    let length = end - start;
    particle_emitters.push(LaserParticleEmitter {
        impact: beam
            .target
            .map(|target| [beam.x as f32, target.y as f32 - 0.5]),
    });
    if length <= 0.0 {
        return;
    }
    instances.push(LaserBeamInstance {
        position: [beam.x as f32, -(start + end) * 0.5, 0.30],
        size: [LASER_BEAM_WIDTH, length],
    });
    lights.reserve(beam.length_tiles as usize);
    let phase = flicker_phase(object.anchor());
    lights.extend((0..beam.length_tiles).map(|offset| FurnitureLightSpec {
        position: [beam.x as f32, (beam.first_y + offset) as f32],
        phase,
        kind: FurnitureLightKind::Laser,
    }));
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
        FurnitureLightKind::Laser => laser_light_intensity(time_seconds, spec.phase),
        FurnitureLightKind::Pylon => pylon_light_intensity(time_seconds, spec.phase),
        FurnitureLightKind::Battery => battery_light_intensity(time_seconds, spec.phase),
    };
    let base = match spec.kind {
        FurnitureLightKind::Laser => LASER_LIGHT_COLOUR,
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
    let Some(uv_rect) = atlas_frame_uv(sprite_frame, FURNITURE_ATLAS_COLUMNS, FURNITURE_ATLAS_ROWS)
    else {
        return;
    };
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
mod tests {
    use super::*;
    use crate::{ForegroundTile, FurnitureObject, Layer, TilePos};

    #[test]
    fn chest_instance_covers_its_two_by_two_world_footprint() {
        let mut world = World::empty(8, 8, 0).unwrap();
        for x in 2..=3 {
            world
                .set_tile(x, 5, Layer::Foreground, ForegroundTile::DIRT)
                .unwrap();
        }
        let id = world
            .place_furniture(FurnitureObject::CHEST, TilePos::new(2, 3))
            .unwrap();
        let mut instances = Vec::new();
        append_furniture_instance(&world, world.object(id).unwrap(), &mut instances);

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].position, [2.5, -3.7, 0.25]);
        assert_eq!(instances[0].size, [2.0, 2.0]);
        assert_eq!(
            instances[0].uv_rect,
            atlas_frame_uv(0, FURNITURE_ATLAS_COLUMNS, FURNITURE_ATLAS_ROWS).unwrap()
        );
    }

    #[test]
    fn cable_anchor_lift_and_station_use_procedural_furniture_visuals() {
        let mut world = World::empty(16, 20, 0).unwrap();
        world
            .set_tile(6, 2, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        let anchor = world
            .place_furniture(FurnitureObject::POWERED_CABLE_ANCHOR, TilePos::new(6, 3))
            .unwrap();
        for _ in 0..6 {
            world
                .place_or_extend_powered_cable(TilePos::new(6, 3))
                .unwrap();
        }
        let lift = world.place_cargo_lift(TilePos::new(6, 4)).unwrap();
        for x in 4..=5 {
            world
                .set_tile(x, 9, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        let station = world.place_lift_station(TilePos::new(4, 7)).unwrap();
        let mut instances = Vec::new();
        append_furniture_instance(&world, world.object(anchor).unwrap(), &mut instances);
        append_furniture_instance(&world, world.object(lift).unwrap(), &mut instances);
        append_furniture_instance(&world, world.object(station).unwrap(), &mut instances);

        assert_eq!(instances[0].visual_kind, 1);
        assert_eq!(instances[0].size, [1.0, 1.0]);
        assert_eq!(instances[1].visual_kind, 2);
        assert_eq!(instances[1].size, [2.0, 2.0]);
        assert_eq!(instances[1].position, [7.5, -4.5, 0.24]);
        assert_eq!(instances[2].visual_kind, 5);
        assert_eq!(instances[2].size, [2.0, 2.0]);
        assert_eq!(instances[2].position, [4.5, -7.7, 0.25]);
    }

    #[test]
    fn laser_bore_uses_the_second_atlas_frame_and_emits_to_its_target() {
        let mut world = World::empty(12, 20, 0).unwrap();
        for x in [2, 4] {
            world
                .set_tile(x, 7, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        for x in [7, 9, 10] {
            world
                .set_tile(x, 7, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        world
            .set_tile(3, 10, Layer::Foreground, ForegroundTile::DIRT)
            .unwrap();
        let id = world
            .place_furniture(FurnitureObject::LASER_BORE, TilePos::new(2, 4))
            .unwrap();
        world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(7, 5))
            .unwrap();
        world
            .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(9, 4))
            .unwrap();
        let mut power = PowerSystem::new();
        power.distribute(&mut world, 0.5, std::time::Duration::from_secs(1));
        assert!(world.set_furniture_active(id, true));
        let object = world.object(id).unwrap();

        let mut furniture = Vec::new();
        append_furniture_instance(&world, object, &mut furniture);
        assert_eq!(furniture[0].position, [3.0, -5.2, 0.25]);
        assert_eq!(furniture[0].size, [3.0, 3.0]);
        assert_eq!(
            furniture[0].uv_rect,
            atlas_frame_uv(1, FURNITURE_ATLAS_COLUMNS, FURNITURE_ATLAS_ROWS).unwrap()
        );

        let mut beams = Vec::new();
        let mut lights = Vec::new();
        let mut emitters = Vec::new();
        append_laser_bore(
            &world,
            &power,
            object,
            &mut beams,
            &mut lights,
            &mut emitters,
        );
        assert_eq!(beams.len(), 1);
        assert_eq!(beams[0].position[0], 3.0);
        assert!((beams[0].position[1] + 8.1).abs() < 0.000_01);
        assert_eq!(beams[0].position[2], 0.30);
        assert_eq!(beams[0].size[0], LASER_BEAM_WIDTH);
        assert!((beams[0].size[1] - 2.8).abs() < 0.000_01);
        assert_eq!(lights.len(), 3);
        assert_eq!(lights[0].position, [3.0, 7.0]);
        assert_eq!(lights[2].position, [3.0, 9.0]);
        assert!(
            lights
                .iter()
                .all(|light| light.kind == FurnitureLightKind::Laser)
        );
        assert_eq!(emitters.len(), 1);
        assert_eq!(emitters[0].impact, Some([3.0, 9.5]));
    }

    #[test]
    fn turret_uses_the_third_atlas_frame() {
        let mut world = World::empty(8, 8, 0).unwrap();
        for x in 2..=3 {
            world
                .set_tile(x, 5, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        let id = world
            .place_furniture(FurnitureObject::TURRET, TilePos::new(2, 3))
            .unwrap();
        let mut instances = Vec::new();
        append_furniture_instance(&world, world.object(id).unwrap(), &mut instances);
        assert_eq!(instances[0].size, [2.0, 2.0]);
        assert_eq!(
            instances[0].uv_rect,
            atlas_frame_uv(2, FURNITURE_ATLAS_COLUMNS, FURNITURE_ATLAS_ROWS).unwrap()
        );
    }

    #[test]
    fn orbital_export_launcher_uses_the_fourth_atlas_frame() {
        let mut world = World::empty(10, 10, 0).unwrap();
        for x in 2..=4 {
            world
                .set_tile(x, 7, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        let id = world
            .place_furniture(FurnitureObject::ORBITAL_EXPORT_LAUNCHER, TilePos::new(2, 4))
            .unwrap();
        let mut instances = Vec::new();
        append_furniture_instance(&world, world.object(id).unwrap(), &mut instances);
        assert_eq!(instances[0].size, [3.0, 3.0]);
        assert_eq!(
            instances[0].uv_rect,
            atlas_frame_uv(3, FURNITURE_ATLAS_COLUMNS, FURNITURE_ATLAS_ROWS).unwrap()
        );
    }

    #[test]
    fn cargo_conveyor_uses_a_connected_atlas_frame() {
        let mut world = World::empty(10, 10, 0).unwrap();
        for x in 2..=3 {
            world
                .set_tile(x, 7, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        world
            .place_furniture(FurnitureObject::CHEST, TilePos::new(2, 5))
            .unwrap();
        let id = world
            .place_furniture(FurnitureObject::CARGO_CONVEYOR, TilePos::new(4, 6))
            .unwrap();
        let mut instances = Vec::new();
        append_furniture_instance(&world, world.object(id).unwrap(), &mut instances);
        assert_eq!(instances[0].position, [4.0, -6.0, 0.25]);
        assert_eq!(instances[0].size, [1.0, 1.0]);
        assert_eq!(
            instances[0].uv_rect,
            atlas_frame_uv(4, FURNITURE_ATLAS_COLUMNS, FURNITURE_ATLAS_ROWS).unwrap()
        );
    }

    #[test]
    fn power_furniture_uses_the_appended_atlas_frames() {
        let mut world = World::empty(12, 12, 0).unwrap();
        for x in [2, 3, 6, 8, 9] {
            world
                .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
        }
        let solar = world
            .place_furniture(FurnitureObject::SOLAR_ARRAY, TilePos::new(2, 5))
            .unwrap();
        let pylon = world
            .place_furniture(FurnitureObject::PYLON, TilePos::new(6, 6))
            .unwrap();
        let battery = world
            .place_furniture(FurnitureObject::BATTERY, TilePos::new(8, 6))
            .unwrap();

        let mut power = PowerSystem::new();
        power.distribute(&mut world, 0.5, std::time::Duration::from_secs(1));
        let mut instances = Vec::new();
        append_furniture_instance(&world, world.object(solar).unwrap(), &mut instances);
        append_furniture_instance(&world, world.object(pylon).unwrap(), &mut instances);
        append_furniture_instance(&world, world.object(battery).unwrap(), &mut instances);
        assert_eq!(instances[0].size, [2.0, 3.0]);
        assert_eq!(
            instances[0].uv_rect,
            atlas_frame_uv(10, FURNITURE_ATLAS_COLUMNS, FURNITURE_ATLAS_ROWS).unwrap()
        );
        assert_eq!(instances[1].size, [1.0, 2.0]);
        assert_eq!(
            instances[1].uv_rect,
            atlas_frame_uv(11, FURNITURE_ATLAS_COLUMNS, FURNITURE_ATLAS_ROWS).unwrap()
        );
        assert_eq!(instances[2].size, [2.0, 2.0]);
        assert_eq!(
            instances[2].uv_rect,
            atlas_frame_uv(12, FURNITURE_ATLAS_COLUMNS, FURNITURE_ATLAS_ROWS).unwrap()
        );
        let mut indicator_lights = Vec::new();
        append_power_indicator_light(&power, world.object(pylon).unwrap(), &mut indicator_lights);
        append_power_indicator_light(
            &power,
            world.object(battery).unwrap(),
            &mut indicator_lights,
        );
        assert_eq!(indicator_lights.len(), 2);
        assert_eq!(indicator_lights[0].kind, FurnitureLightKind::Pylon);
        assert_eq!(indicator_lights[1].kind, FurnitureLightKind::Battery);

        assert!(world.remove_object(solar).is_some());
        power.distribute(&mut world, 0.9, std::time::Duration::from_secs(1));
        indicator_lights.clear();
        append_power_indicator_light(&power, world.object(pylon).unwrap(), &mut indicator_lights);
        append_power_indicator_light(
            &power,
            world.object(battery).unwrap(),
            &mut indicator_lights,
        );
        assert_eq!(indicator_lights.len(), 2);

        assert!(world.set_battery_charge_milli(battery, 0));
        power.distribute(&mut world, 0.9, std::time::Duration::from_secs(1));
        indicator_lights.clear();
        append_power_indicator_light(&power, world.object(pylon).unwrap(), &mut indicator_lights);
        append_power_indicator_light(
            &power,
            world.object(battery).unwrap(),
            &mut indicator_lights,
        );
        assert!(indicator_lights.is_empty());
    }

    #[test]
    fn power_connector_uses_a_procedural_one_tile_visual() {
        let mut world = World::empty(8, 8, 0).unwrap();
        world
            .set_tile(
                3,
                3,
                crate::Layer::Background,
                crate::BackgroundTile::STONE_WALL,
            )
            .unwrap();
        let connector = world
            .place_furniture(FurnitureObject::POWER_CONNECTOR, TilePos::new(3, 3))
            .unwrap();
        let mut instances = Vec::new();

        append_furniture_instance(&world, world.object(connector).unwrap(), &mut instances);

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].size, [1.0, 1.0]);
        assert_eq!(instances[0].visual_kind, 6);
    }

    #[test]
    fn cable_is_a_single_segment_batch_with_downward_sag() {
        let mut world = World::empty(32, 16, 0).unwrap();
        for x in [2, 10] {
            world
                .set_tile(x, 8, Layer::Foreground, ForegroundTile::STONE)
                .unwrap();
            world
                .place_furniture(FurnitureObject::PYLON, TilePos::new(x, 6))
                .unwrap();
        }
        let mut power = PowerSystem::new();
        power.update(&world);
        let connection = power.connections()[0];
        let mut segments = Vec::new();
        append_power_cable(connection, &mut segments);

        assert_eq!(segments.len(), CABLE_SEGMENTS);
        let straight_y = (connection.start()[1] + connection.end()[1]) * 0.5;
        assert!(-segments[CABLE_SEGMENTS / 2].position[1] > straight_y);
        assert!(
            segments
                .iter()
                .all(|segment| segment.size[1] == CABLE_WIDTH)
        );
    }

    #[test]
    fn furniture_light_flicker_is_subtle_smooth_and_bounded() {
        let phase = flicker_phase(TilePos::new(12, 34));
        let laser_a = laser_light_intensity(2.0, phase);
        let laser_b = laser_light_intensity(2.01, phase);
        let pylon_a = pylon_light_intensity(2.0, phase);
        let pylon_b = pylon_light_intensity(2.01, phase);
        let battery_a = battery_light_intensity(2.0, phase);
        let battery_b = battery_light_intensity(2.01, phase);

        assert!((0.82..=1.0).contains(&laser_a));
        assert!((0.72..=0.91).contains(&pylon_a));
        assert!((0.79..=0.85).contains(&battery_a));
        assert!((laser_b - laser_a).abs() < 0.02);
        assert!((pylon_b - pylon_a).abs() < 0.02);
        assert!((battery_b - battery_a).abs() < 0.01);
    }
}
