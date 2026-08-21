use crate::Renderer;
use crate::assets_manager::Handle;
use wgpu::{
    BindGroupLayout, BufferBindingType, ComputePipelineDescriptor, ShaderModule,
    StorageTextureAccess, TextureFormat, TextureViewDimension,
};

pub struct ComputePipeline {
    pub pipeline: wgpu::ComputePipeline,
    pub layout: BindGroupLayout,
}

pub struct ComputePipelineBuilder<'a> {
    shader: Handle<ShaderModule>,
    entries: Vec<wgpu::BindGroupLayoutEntry>,
    entry_point: &'a str,
}

impl<'a> ComputePipelineBuilder<'a> {
    pub fn new(shader: Handle<ShaderModule>) -> Self {
        Self {
            shader,
            entries: vec![],
            entry_point: "cs_main",
        }
    }
    pub fn entry_point(mut self, entry_point: &'a str) -> Self {
        self.entry_point = entry_point;
        self
    }
    pub fn bind_group_layout(mut self, entries: &[wgpu::BindGroupLayoutEntry]) -> Self {
        self.entries = entries.to_vec();
        self
    }
    pub fn build(&self, renderer: &mut Renderer) -> Handle<ComputePipeline> {
        let bind_group_layout =
            renderer
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("bind group layout"),
                    entries: &self.entries,
                });

        let layout = renderer
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            });

        let pipeline = renderer
            .device
            .create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("Compute Pipeline"),
                layout: Some(&layout),
                module: renderer.asset_manager.shaders.get(self.shader).unwrap(),
                entry_point: Some(self.entry_point),
                compilation_options: Default::default(),
                cache: None,
            });

        renderer
            .asset_manager
            .compute_pipelines
            .insert(ComputePipeline {
                pipeline,
                layout: bind_group_layout,
            })
    }
}

pub fn compute_uniform(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

pub fn compute_storage(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

pub fn storage_texture(binding: u32, format: TextureFormat) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: StorageTextureAccess::WriteOnly,
            format,
            view_dimension: TextureViewDimension::D2,
        },
        count: None,
    }
}

pub fn compute_texture_float(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}
pub fn compute_texture_uint(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Uint,
            view_dimension: TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}
