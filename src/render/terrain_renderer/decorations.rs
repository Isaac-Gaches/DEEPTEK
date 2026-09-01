use super::lighting::LightingEngine;
use crate::{
    ChunkPos, DecorationVisual, ObjectId, World, WorldObject, decoration_definition, render_common,
};
use easy_gpu::assets::{
    Buffer, BufferLayout, GpuInstance, GpuVertex, Material, MaterialBuilder, Mesh, RenderPipeline,
    RenderPipelineBuilder, Sampler, SamplerBuilder, Texture, render_texture, render_uniform,
    sampler,
};
use easy_gpu::assets_manager::Handle;
use easy_gpu::frame::Frame;
use easy_gpu::wgpu::{BlendState, FilterMode, TextureFormat, VertexFormat, VertexStepMode};
use std::collections::HashSet;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct DecorationInstance {
    position: [f32; 3],
    frame: u32,
    visual_kind: u32,
}

impl GpuInstance for DecorationInstance {
    fn buffer_layout() -> BufferLayout {
        BufferLayout::new()
            .stride(size_of::<Self>() as u64)
            .step_mode(VertexStepMode::Instance)
            .attribute(1, 0, VertexFormat::Float32x3)
            .attribute(2, 12, VertexFormat::Uint32)
            .attribute(3, 16, VertexFormat::Uint32)
    }
}

pub(super) struct DecorationRenderer {
    material: Handle<Material>,
    pipeline: Handle<RenderPipeline>,
    texture: Handle<Texture>,
    texture_sampler: Handle<Sampler>,
    camera_buffer: Handle<Buffer>,
    quad: Handle<Mesh>,
    instances: Vec<DecorationInstance>,
    seen_objects: HashSet<ObjectId>,
}

impl DecorationRenderer {
    pub(super) fn new(
        gpu: &mut easy_gpu::Renderer,
        camera_buffer: Handle<Buffer>,
        lighting: &LightingEngine,
    ) -> Self {
        let quad = render_common::create_unit_quad(gpu);
        let shader = gpu.load_shader(include_str!("decorations.wgsl"));
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
            .vertex_layout(DecorationInstance::buffer_layout())
            .depth_format(TextureFormat::Depth24Plus)
            .blend_mode(BlendState::REPLACE)
            .build(gpu);
        let texture = gpu.load_texture_from_file(
            include_bytes!("../../../assets/decorations/deco.png").to_vec(),
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
        Self {
            material,
            pipeline,
            texture,
            texture_sampler,
            camera_buffer,
            quad,
            instances: Vec::new(),
            seen_objects: HashSet::new(),
        }
    }

    pub(super) fn rebind_lighting(&self, gpu: &mut easy_gpu::Renderer, lighting: &LightingEngine) {
        render_common::replace_material(
            gpu,
            self.material,
            MaterialBuilder::new(self.pipeline)
                .buffer(0, self.camera_buffer)
                .texture(1, self.texture)
                .sampler(2, self.texture_sampler)
                .texture(3, lighting.light_texture)
                .sampler(4, lighting.light_sampler)
                .buffer(5, lighting.light_meta_buffer),
        );
    }

    pub(super) fn sync(&mut self, world: &World, loaded_chunks: impl Iterator<Item = ChunkPos>) {
        self.instances.clear();
        self.seen_objects.clear();
        for chunk in loaded_chunks {
            for object in world.objects_in_chunk(chunk) {
                if self.seen_objects.insert(object.id()) {
                    append_object_instances(object, &mut self.instances);
                }
            }
        }
    }

    pub(super) fn draw(&self, frame: &mut Frame) {
        if !self.instances.is_empty() {
            frame.draw_batch(&self.instances, self.material, self.quad);
        }
    }
}

fn append_object_instances(object: &WorldObject, output: &mut Vec<DecorationInstance>) {
    let anchor = object.anchor();
    let Some(definition) = decoration_definition(object.object_type()) else {
        return;
    };
    match definition.visual() {
        DecorationVisual::Variants {
            first_frame,
            variants,
        } => {
            let variant = object.variant() % variants.max(1);
            output.push(instance(
                anchor.x,
                anchor.y as f32 + 0.15,
                u32::from(first_frame) + u32::from(variant),
                0,
            ));
        }
        DecorationVisual::Static { frame } => {
            output.push(instance(
                anchor.x,
                anchor.y as f32 + 0.15,
                u32::from(frame),
                0,
            ));
        }
        DecorationVisual::Segmented {
            body_frame,
            tip_frame,
        } => {
            let height = object.size()[1];
            output.reserve(usize::from(height));
            for segment in 0..height {
                let frame = if segment + 1 == height {
                    tip_frame
                } else {
                    body_frame
                };
                output.push(instance(
                    anchor.x,
                    anchor.y as f32 + f32::from(segment) - 0.15,
                    u32::from(frame),
                    0,
                ));
            }
        }
        DecorationVisual::Rope => {
            let height = object.size()[1];
            output.reserve(usize::from(height));
            for segment in 0..height {
                output.push(instance(
                    anchor.x,
                    anchor.y as f32 + f32::from(segment),
                    0,
                    if segment + 1 == height { 2 } else { 1 },
                ));
            }
        }
        DecorationVisual::PoweredCable => {
            let height = object.size()[1];
            output.reserve(usize::from(height));
            for segment in 0..height {
                let visual_kind = match (segment == 0, segment + 1 == height) {
                    (true, true) => 6,
                    (true, false) => 5,
                    (false, true) => 4,
                    (false, false) => 3,
                };
                output.push(instance(
                    anchor.x,
                    anchor.y as f32 + f32::from(segment),
                    0,
                    visual_kind,
                ));
            }
        }
    }
}

fn instance(x: u32, logical_y: f32, frame: u32, visual_kind: u32) -> DecorationInstance {
    DecorationInstance {
        // WebGPU clip-space depth is 0.0..=1.0. Keep decorations between
        // foreground terrain (0.05) and background walls (0.5) so they remain
        // visible in air while foreground tiles naturally occlude them.
        position: [x as f32, -logical_y, 0.25],
        frame,
        visual_kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ForegroundTile, Layer, NaturalObject, TilePos};

    #[test]
    fn growing_vine_uses_body_segments_and_one_tip() {
        let mut world = World::empty(8, 8, 0).unwrap();
        world
            .set_tile(2, 1, Layer::Foreground, ForegroundTile::GRASS)
            .unwrap();
        let id = world
            .place_natural_object(NaturalObject::VINE, TilePos::new(2, 2), TilePos::new(2, 1))
            .unwrap();
        let mut instances = Vec::new();
        append_object_instances(world.object(id).unwrap(), &mut instances);
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].frame, 2);
    }

    #[test]
    fn decoration_frames_match_the_original_atlas() {
        let mut world = World::empty(8, 8, 0).unwrap();
        world
            .set_tile(2, 3, Layer::Foreground, ForegroundTile::GRASS)
            .unwrap();
        let grass = world
            .place_natural_object(NaturalObject::GRASS, TilePos::new(2, 2), TilePos::new(2, 3))
            .unwrap();
        let mut instances = Vec::new();
        append_object_instances(world.object(grass).unwrap(), &mut instances);
        assert_eq!(
            instances[0].frame,
            3 + u32::from(world.object(grass).unwrap().variant() % 2)
        );
        assert!((0.0..=1.0).contains(&instances[0].position[2]));
    }

    #[test]
    fn hanging_stone_uses_the_first_atlas_frame() {
        let mut world = World::empty(8, 8, 0).unwrap();
        world
            .set_tile(2, 1, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        let id = world
            .place_natural_object(
                NaturalObject::HANGING_STONE,
                TilePos::new(2, 2),
                TilePos::new(2, 1),
            )
            .unwrap();

        let mut instances = Vec::new();
        append_object_instances(world.object(id).unwrap(), &mut instances);

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].frame, 0);
    }

    #[test]
    fn rope_uses_body_segments_and_one_wider_tip() {
        let mut world = World::empty(8, 8, 0).unwrap();
        world
            .set_tile(2, 1, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        world.place_or_extend_rope(TilePos::new(2, 2)).unwrap();
        world.place_or_extend_rope(TilePos::new(2, 2)).unwrap();
        world.place_or_extend_rope(TilePos::new(2, 2)).unwrap();

        let mut instances = Vec::new();
        append_object_instances(world.object_at(TilePos::new(2, 2)).unwrap(), &mut instances);
        assert_eq!(instances.len(), 3);
        assert_eq!(
            instances
                .iter()
                .map(|instance| instance.visual_kind)
                .collect::<Vec<_>>(),
            vec![1, 1, 2]
        );
    }

    #[test]
    fn powered_cable_renders_built_in_top_and_bottom_terminals() {
        let mut world = World::empty(8, 8, 0).unwrap();
        world
            .set_tile(2, 1, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        for _ in 0..3 {
            world
                .place_or_extend_powered_cable(TilePos::new(2, 2))
                .unwrap();
        }

        let mut instances = Vec::new();
        append_object_instances(world.object_at(TilePos::new(2, 2)).unwrap(), &mut instances);
        assert_eq!(
            instances
                .iter()
                .map(|instance| instance.visual_kind)
                .collect::<Vec<_>>(),
            vec![5, 3, 4]
        );
    }
}
