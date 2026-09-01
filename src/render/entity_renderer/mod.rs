use crate::{Sprite, TerrainRenderer, Transform, render_common};
use easy_gpu::assets::{
    BufferLayout, BufferUsages, GpuInstance, GpuVertex, Material, MaterialBuilder, Mesh,
    RenderPipeline, RenderPipelineBuilder, Sampler, SamplerBuilder, Texture, render_storage,
    render_texture, render_uniform, sampler,
};
use easy_gpu::assets_manager::Handle;
use easy_gpu::frame::Frame;
use easy_gpu::wgpu::{BlendState, FilterMode, TextureFormat, VertexFormat, VertexStepMode};
use hecs::World;
use std::collections::HashMap;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SpriteAtlasFrame {
    pub min_uv: [f32; 2],
    pub max_uv: [f32; 2],
}

impl SpriteAtlasFrame {
    pub const FULL: Self = Self {
        min_uv: [0.0, 0.0],
        max_uv: [1.0, 1.0],
    };

    pub const fn new(min_uv: [f32; 2], max_uv: [f32; 2]) -> Self {
        Self { min_uv, max_uv }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SpriteInstance {
    position: [f32; 3],
    rotation: f32,
    scale: [f32; 2],
    frame: u32,
    tint: [f32; 4],
    emissive: f32,
}

impl GpuInstance for SpriteInstance {
    fn buffer_layout() -> BufferLayout {
        BufferLayout::new()
            .stride(size_of::<Self>() as u64)
            .step_mode(VertexStepMode::Instance)
            .attribute(1, 0, VertexFormat::Float32x3)
            .attribute(2, 12, VertexFormat::Float32)
            .attribute(3, 16, VertexFormat::Float32x2)
            .attribute(4, 24, VertexFormat::Uint32)
            .attribute(5, 28, VertexFormat::Float32x4)
            .attribute(6, 44, VertexFormat::Float32)
    }
}

/// Collects all ECS sprites into one retained instance batch per material.
pub struct SpriteRenderer {
    pipeline: Handle<RenderPipeline>,
    quad: Handle<Mesh>,
    sampler: Handle<Sampler>,
    batches: HashMap<Handle<Material>, Vec<SpriteInstance>>,
    material_bindings: HashMap<Handle<Material>, SpriteMaterialBindings>,
}

#[derive(Clone, Copy)]
struct SpriteMaterialBindings {
    texture: Handle<Texture>,
    atlas: Handle<easy_gpu::assets::Buffer>,
}

impl SpriteRenderer {
    pub fn new(gpu: &mut easy_gpu::Renderer) -> Self {
        let quad = render_common::create_unit_quad(gpu);
        let shader = gpu.load_shader(include_str!("shader.wgsl"));
        let pipeline = RenderPipelineBuilder::new(shader)
            .material_layout(&[
                render_uniform(0),
                render_texture(1),
                sampler(2),
                render_texture(3),
                sampler(4),
                render_uniform(5),
                render_storage(6, true),
            ])
            .vertex_layout(render_common::QuadVertex::buffer_layout())
            .vertex_layout(SpriteInstance::buffer_layout())
            .depth_format(TextureFormat::Depth24Plus)
            .blend_mode(BlendState::ALPHA_BLENDING)
            .build(gpu);
        let sampler = SamplerBuilder::new()
            .filter_mode(FilterMode::Nearest)
            .build(gpu);
        Self {
            pipeline,
            quad,
            sampler,
            batches: HashMap::new(),
            material_bindings: HashMap::new(),
        }
    }

    pub fn create_material(
        &mut self,
        gpu: &mut easy_gpu::Renderer,
        terrain: &TerrainRenderer,
        texture: Handle<Texture>,
        frames: &[SpriteAtlasFrame],
    ) -> Handle<Material> {
        let frames = if frames.is_empty() {
            std::slice::from_ref(&SpriteAtlasFrame::FULL)
        } else {
            frames
        };
        let atlas =
            gpu.create_buffer_with_contents(BufferUsages::STORAGE, bytemuck::cast_slice(frames));
        let material = MaterialBuilder::new(self.pipeline)
            .buffer(0, terrain.camera_buffer())
            .texture(1, texture)
            .sampler(2, self.sampler)
            .texture(3, terrain.light_texture())
            .sampler(4, terrain.light_sampler())
            .buffer(5, terrain.light_meta_buffer())
            .buffer(6, atlas)
            .build(gpu);
        self.material_bindings
            .insert(material, SpriteMaterialBindings { texture, atlas });
        material
    }

    pub fn rebind_lighting(&self, gpu: &mut easy_gpu::Renderer, terrain: &TerrainRenderer) {
        for (&material, bindings) in &self.material_bindings {
            render_common::replace_material(
                gpu,
                material,
                MaterialBuilder::new(self.pipeline)
                    .buffer(0, terrain.camera_buffer())
                    .texture(1, bindings.texture)
                    .sampler(2, self.sampler)
                    .texture(3, terrain.light_texture())
                    .sampler(4, terrain.light_sampler())
                    .buffer(5, terrain.light_meta_buffer())
                    .buffer(6, bindings.atlas),
            );
        }
    }

    pub fn draw(&mut self, frame: &mut Frame, entities: &World) {
        for instances in self.batches.values_mut() {
            instances.clear();
        }

        for (_, (sprite, transform)) in entities.query::<(&Sprite, &Transform)>().iter() {
            self.batches
                .entry(sprite.material)
                .or_default()
                .push(SpriteInstance {
                    position: [transform.position[0], -transform.position[1], sprite.depth],
                    rotation: transform.rotation,
                    scale: transform.scale,
                    frame: sprite.frame,
                    tint: sprite.tint,
                    emissive: sprite.emissive,
                });
        }

        for (&material, instances) in &self.batches {
            if !instances.is_empty() {
                frame.draw_batch(instances, material, self.quad);
            }
        }
    }
}
