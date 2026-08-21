use crate::Renderer;
use crate::assets_manager::Handle;
use wgpu::{Extent3d, FilterMode, TextureDimension, TextureFormat, TextureUsages};

pub struct Texture {
    pub(crate) texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
}

impl Texture {
    pub(crate) fn new(texture: wgpu::Texture, view: wgpu::TextureView) -> Self {
        Self { texture, view }
    }
}

pub struct TextureBuilder {
    size: Extent3d,
    dimension: TextureDimension,
    format: TextureFormat,
    usage: TextureUsages,
}
impl Default for TextureBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TextureBuilder {
    pub fn new() -> Self {
        Self {
            size: Extent3d {
                width: 128,
                height: 128,
                depth_or_array_layers: 1,
            },
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        }
    }

    pub fn size(mut self, size: Extent3d) -> Self {
        self.size = size;
        self
    }

    pub fn dimension_3d(mut self) -> Self {
        self.dimension = TextureDimension::D3;
        self
    }

    pub fn usage(mut self, usage: TextureUsages) -> Self {
        self.usage = usage;
        self
    }

    pub fn format(mut self, format: TextureFormat) -> Self {
        self.format = format;
        self
    }

    pub fn build(&self, renderer: &mut Renderer) -> Handle<Texture> {
        let tex = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("texture"),
            size: self.size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: self.dimension,
            format: self.format,
            usage: self.usage,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());

        renderer
            .asset_manager
            .textures
            .insert(Texture::new(tex, view))
    }
}

pub struct SamplerBuilder {
    mag_filter: FilterMode,
    min_filter: FilterMode,
}
impl Default for SamplerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SamplerBuilder {
    pub fn new() -> Self {
        Self {
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Nearest,
        }
    }

    pub fn filter_mode(mut self, filter_mode: FilterMode) -> Self {
        self.mag_filter = filter_mode;
        self
    }

    pub fn min_filter_mode(mut self, filter_mode: FilterMode) -> Self {
        self.min_filter = filter_mode;
        self
    }
    pub fn build(&self, renderer: &mut Renderer) -> Handle<Sampler> {
        let sampler = renderer.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sampler"),
            address_mode_u: Default::default(),
            address_mode_v: Default::default(),
            address_mode_w: Default::default(),
            mag_filter: self.mag_filter,
            min_filter: self.min_filter,
            mipmap_filter: Default::default(),
            lod_min_clamp: 0.0,
            lod_max_clamp: 0.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });
        renderer.asset_manager.samplers.insert(Sampler { sampler })
    }
}

pub struct Sampler {
    pub(crate) sampler: wgpu::Sampler,
}
