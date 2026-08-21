use crate::assets::buffer::Buffer;
use crate::assets::compute::bind_group::ComputeBindGroup;
use crate::assets::compute::pipeline::ComputePipeline;
use crate::assets::render::material::Material;
use crate::assets::render::mesh::Mesh;
use crate::assets::render::pipeline::RenderPipeline;
use crate::assets::texture::{Sampler, Texture};
use crate::assets_manager::arena::Arena;
use wgpu::ShaderModule;

pub struct AssetManager {
    pub render_pipelines: Arena<RenderPipeline>,
    pub meshes: Arena<Mesh>,
    pub materials: Arena<Material>,
    pub textures: Arena<Texture>,
    pub samplers: Arena<Sampler>,
    pub shaders: Arena<ShaderModule>,
    pub compute_pipelines: Arena<ComputePipeline>,
    pub compute_bind_groups: Arena<ComputeBindGroup>,
    pub buffers: Arena<Buffer>,
}

impl AssetManager {
    pub fn new() -> Self {
        Self {
            render_pipelines: Arena::new(),
            meshes: Arena::new(),
            materials: Arena::new(),
            textures: Arena::new(),
            samplers: Arena::new(),
            shaders: Arena::new(),
            compute_pipelines: Arena::new(),
            compute_bind_groups: Arena::new(),
            buffers: Arena::new(),
        }
    }
}
