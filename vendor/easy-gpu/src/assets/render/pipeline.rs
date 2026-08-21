use crate::Renderer;
use crate::assets::vertex_layout::BufferLayout;
use crate::assets_manager::handle::Handle;
use wgpu::ShaderModule;
use wgpu::{BlendState, BufferBindingType};

pub struct RenderPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub material_layout: wgpu::BindGroupLayout,
}

pub struct RenderPipelineBuilder<'a> {
    shader: Handle<ShaderModule>,
    vertex_layouts: Vec<wgpu::VertexBufferLayout<'a>>,
    pub(crate) depth_format: Option<wgpu::TextureFormat>,
    depth_writes_enabled: bool,
    material_entries: Vec<wgpu::BindGroupLayoutEntry>,
    blend: BlendState,
    target_format: Option<wgpu::TextureFormat>,
    vs_entry: &'a str,
    fs_entry: &'a str,
}

impl<'a> RenderPipelineBuilder<'a> {
    pub fn new(shader: Handle<ShaderModule>) -> Self {
        Self {
            shader,
            vertex_layouts: Vec::new(),
            depth_format: None,
            depth_writes_enabled: true,
            material_entries: vec![],
            blend: BlendState::ALPHA_BLENDING,
            target_format: None,
            vs_entry: "vs_main",
            fs_entry: "fs_main",
        }
    }
    pub fn material_layout(mut self, entries: &[wgpu::BindGroupLayoutEntry]) -> Self {
        self.material_entries = entries.to_vec();
        self
    }

    pub fn vertex_layout(mut self, layout: BufferLayout) -> Self {
        self.vertex_layouts.push(layout.to_wgpu_layout());
        self
    }

    pub fn depth_format(mut self, format: wgpu::TextureFormat) -> Self {
        self.depth_format = Some(format);
        self
    }

    pub fn target_format(mut self, format: wgpu::TextureFormat) -> Self {
        self.target_format = Some(format);
        self
    }

    pub fn depth_writes_enabled(mut self, enabled: bool) -> Self {
        self.depth_writes_enabled = enabled;
        self
    }

    pub fn vs_entry_point(mut self, entry: &'a str) -> Self {
        self.vs_entry = entry;
        self
    }

    pub fn fs_entry_point(mut self, entry: &'a str) -> Self {
        self.fs_entry = entry;
        self
    }

    pub fn additive_alpha_blending(mut self) -> Self {
        self.blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        self
    }

    pub fn blend_mode(mut self, blend_state: BlendState) -> Self {
        self.blend = blend_state;
        self
    }

    pub fn build(&self, renderer: &mut Renderer) -> Handle<RenderPipeline> {
        if self.depth_format.is_some() && renderer.depth_texture.is_none() {
            renderer.create_depth_texture(
                renderer.surface_config.width,
                renderer.surface_config.height,
            );
        }

        let shader = renderer.asset_manager.shaders.get(self.shader).unwrap();

        let material_layout =
            renderer
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("material layout"),
                    entries: &self.material_entries,
                });

        let pipeline_layout =
            renderer
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("pipeline layout"),
                    bind_group_layouts: &[Some(&material_layout)],
                    immediate_size: 0,
                });

        let pipeline = renderer
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Option::from(self.vs_entry),
                    compilation_options: Default::default(),
                    buffers: &self.vertex_layouts,
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Option::from(self.fs_entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.target_format.unwrap_or(renderer.surface_config.format),
                        blend: Some(self.blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: self.depth_format.map(|format| wgpu::DepthStencilState {
                    format,
                    depth_write_enabled: Option::from(self.depth_writes_enabled),
                    depth_compare: Option::from(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                cache: None,
            });

        renderer
            .asset_manager
            .render_pipelines
            .insert(RenderPipeline {
                pipeline,
                material_layout,
            })
    }
}

pub fn render_texture(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            multisampled: false,
            view_dimension: wgpu::TextureViewDimension::D2,
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
        },
        count: None,
    }
}

pub fn sampler(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

pub fn render_storage(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
pub fn render_uniform(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
