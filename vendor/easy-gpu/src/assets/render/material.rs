use crate::Renderer;
use crate::assets::buffer::Buffer;
use crate::assets::render::pipeline::RenderPipeline;
use crate::assets::texture::{Sampler, Texture};
use crate::assets_manager::handle::Handle;

pub struct Material {
    pub bind_group: wgpu::BindGroup,
    pub pipeline: Handle<RenderPipeline>,
}

pub struct MaterialBuilder {
    textures: Vec<(u32, Handle<Texture>)>,
    samplers: Vec<(u32, Handle<Sampler>)>,
    buffers: Vec<(u32, Handle<Buffer>)>,
    pipeline: Handle<RenderPipeline>,
}

impl MaterialBuilder {
    pub fn new(pipeline: Handle<RenderPipeline>) -> Self {
        Self {
            textures: Vec::new(),
            samplers: Vec::new(),
            buffers: Vec::new(),
            pipeline,
        }
    }

    pub fn texture(mut self, binding: u32, texture: Handle<Texture>) -> Self {
        self.textures.push((binding, texture));
        self
    }

    pub fn sampler(mut self, binding: u32, sampler: Handle<Sampler>) -> Self {
        self.samplers.push((binding, sampler));
        self
    }

    pub fn buffer(mut self, binding: u32, buffer: Handle<Buffer>) -> Self {
        self.buffers.push((binding, buffer));
        self
    }

    pub fn build(&self, renderer: &mut Renderer) -> Handle<Material> {
        let pipeline = renderer
            .asset_manager
            .render_pipelines
            .get(self.pipeline)
            .unwrap();

        let mut entries = Vec::new();

        for (binding, handle) in &self.buffers {
            let buffer = renderer.asset_manager.buffers.get(*handle).unwrap();
            entries.push(wgpu::BindGroupEntry {
                binding: *binding,
                resource: buffer.buffer.as_entire_binding(),
            });
        }

        for (binding, handle) in &self.textures {
            let texture = renderer.asset_manager.textures.get(*handle).unwrap();
            entries.push(wgpu::BindGroupEntry {
                binding: *binding,
                resource: wgpu::BindingResource::TextureView(&texture.view),
            });
        }
        for (binding, handle) in &self.samplers {
            let sampler = renderer.asset_manager.samplers.get(*handle).unwrap();
            entries.push(wgpu::BindGroupEntry {
                binding: *binding,
                resource: wgpu::BindingResource::Sampler(&sampler.sampler),
            });
        }

        let bind_group = renderer
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("material bind group"),
                layout: &pipeline.material_layout,
                entries: &entries,
            });

        renderer.asset_manager.materials.insert(Material {
            bind_group,
            pipeline: self.pipeline,
        })
    }
}
