//! Full-frame colour-selective bloom.

use crate::render_common::{QuadVertex, create_unit_quad};
use easy_gpu::Renderer;
use easy_gpu::assets::{
    GpuVertex, Material, MaterialBuilder, Mesh, RenderPipeline, RenderPipelineBuilder, Sampler,
    SamplerBuilder, Texture, TextureBuilder, render_texture, sampler,
};
use easy_gpu::assets_manager::Handle;
use easy_gpu::frame::Frame;
use easy_gpu::wgpu::{BlendState, Extent3d, FilterMode, TextureFormat, TextureUsages};

const BLOOM_DOWNSAMPLE: u32 = 4;
const BLUR_ITERATIONS: usize = 2;
const BLOOM_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

#[derive(Clone, Copy)]
struct BloomPipelines {
    extract: Handle<RenderPipeline>,
    blur_horizontal: Handle<RenderPipeline>,
    blur_vertical: Handle<RenderPipeline>,
    composite: Handle<RenderPipeline>,
}

struct BloomTargets {
    full_size: [u32; 2],
    scene: Handle<Texture>,
    bloom_a: Handle<Texture>,
    bloom_b: Handle<Texture>,
    extract_material: Handle<Material>,
    blur_horizontal_material: Handle<Material>,
    blur_vertical_material: Handle<Material>,
    composite_material: Handle<Material>,
}

/// Records a colour-selective bloom chain after the scene's draw calls.
///
/// Extraction accepts only the six endpoint RGB colours requested by the game:
/// green, yellow, cyan, magenta, red, and white. All work remains on the GPU.
pub struct BloomRenderer {
    quad: Handle<Mesh>,
    sampler: Handle<Sampler>,
    pipelines: BloomPipelines,
    targets: BloomTargets,
}

impl BloomRenderer {
    pub fn new(gpu: &mut Renderer) -> Self {
        let shader = gpu.load_shader(include_str!("bloom.wgsl"));
        let resource_layout = [render_texture(0), render_texture(1), sampler(2)];
        let bloom_pipeline = |gpu: &mut Renderer, entry_point| {
            RenderPipelineBuilder::new(shader)
                .vertex_layout(QuadVertex::buffer_layout())
                .material_layout(&resource_layout)
                .fs_entry_point(entry_point)
                .target_format(BLOOM_FORMAT)
                .blend_mode(BlendState::REPLACE)
                .build(gpu)
        };
        let extract = bloom_pipeline(gpu, "fs_extract");
        let blur_horizontal = bloom_pipeline(gpu, "fs_blur_horizontal");
        let blur_vertical = bloom_pipeline(gpu, "fs_blur_vertical");
        let composite = RenderPipelineBuilder::new(shader)
            .vertex_layout(QuadVertex::buffer_layout())
            .material_layout(&resource_layout)
            .fs_entry_point("fs_composite")
            .depth_format(TextureFormat::Depth24Plus)
            .depth_writes_enabled(false)
            .blend_mode(BlendState::REPLACE)
            .build(gpu);
        let pipelines = BloomPipelines {
            extract,
            blur_horizontal,
            blur_vertical,
            composite,
        };
        let sampler = SamplerBuilder::new()
            .filter_mode(FilterMode::Linear)
            .min_filter_mode(FilterMode::Linear)
            .build(gpu);
        let targets = BloomTargets::new(gpu, gpu.width(), gpu.height(), pipelines, sampler);

        Self {
            quad: create_unit_quad(gpu),
            sampler,
            pipelines,
            targets,
        }
    }

    /// Reallocates only the size-dependent textures and bind groups.
    pub fn resize(&mut self, gpu: &mut Renderer, width: u32, height: u32) {
        let size = [width.max(1), height.max(1)];
        if self.targets.full_size == size {
            return;
        }

        let replacement = BloomTargets::new(gpu, size[0], size[1], self.pipelines, self.sampler);
        let old = std::mem::replace(&mut self.targets, replacement);
        old.remove(gpu);
    }

    /// Ends the queued scene pass, records bloom extraction and blur passes,
    /// then queues the final scene-plus-glow composite for the surface.
    pub fn queue(&self, frame: &mut Frame) {
        frame.render_to_texture(self.targets.scene, true);

        frame.draw(self.targets.extract_material, self.quad);
        frame.render_to_texture(self.targets.bloom_a, false);

        for _ in 0..BLUR_ITERATIONS {
            frame.draw(self.targets.blur_horizontal_material, self.quad);
            frame.render_to_texture(self.targets.bloom_b, false);
            frame.draw(self.targets.blur_vertical_material, self.quad);
            frame.render_to_texture(self.targets.bloom_a, false);
        }

        frame.draw(self.targets.composite_material, self.quad);
    }
}

impl BloomTargets {
    fn new(
        gpu: &mut Renderer,
        width: u32,
        height: u32,
        pipelines: BloomPipelines,
        sampler_handle: Handle<Sampler>,
    ) -> Self {
        let full_size = [width.max(1), height.max(1)];
        let bloom_size = [
            downsampled_extent(full_size[0]),
            downsampled_extent(full_size[1]),
        ];
        let usage = TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING;
        let scene = TextureBuilder::new()
            .size(texture_extent(full_size))
            .format(gpu.surface_format())
            .usage(usage)
            .build(gpu);
        let bloom_a = TextureBuilder::new()
            .size(texture_extent(bloom_size))
            .format(BLOOM_FORMAT)
            .usage(usage)
            .build(gpu);
        let bloom_b = TextureBuilder::new()
            .size(texture_extent(bloom_size))
            .format(BLOOM_FORMAT)
            .usage(usage)
            .build(gpu);

        Self {
            full_size,
            scene,
            bloom_a,
            bloom_b,
            extract_material: post_process_material(
                gpu,
                pipelines.extract,
                scene,
                scene,
                sampler_handle,
            ),
            blur_horizontal_material: post_process_material(
                gpu,
                pipelines.blur_horizontal,
                bloom_a,
                bloom_a,
                sampler_handle,
            ),
            blur_vertical_material: post_process_material(
                gpu,
                pipelines.blur_vertical,
                bloom_b,
                bloom_b,
                sampler_handle,
            ),
            composite_material: post_process_material(
                gpu,
                pipelines.composite,
                scene,
                bloom_a,
                sampler_handle,
            ),
        }
    }

    fn remove(self, gpu: &mut Renderer) {
        let _ = gpu.asset_manager.materials.remove(self.extract_material);
        let _ = gpu
            .asset_manager
            .materials
            .remove(self.blur_horizontal_material);
        let _ = gpu
            .asset_manager
            .materials
            .remove(self.blur_vertical_material);
        let _ = gpu.asset_manager.materials.remove(self.composite_material);
        let _ = gpu.asset_manager.textures.remove(self.scene);
        let _ = gpu.asset_manager.textures.remove(self.bloom_a);
        let _ = gpu.asset_manager.textures.remove(self.bloom_b);
    }
}

fn post_process_material(
    gpu: &mut Renderer,
    pipeline: Handle<RenderPipeline>,
    source: Handle<Texture>,
    secondary: Handle<Texture>,
    sampler_handle: Handle<Sampler>,
) -> Handle<Material> {
    MaterialBuilder::new(pipeline)
        .texture(0, source)
        .texture(1, secondary)
        .sampler(2, sampler_handle)
        .build(gpu)
}

const fn downsampled_extent(value: u32) -> u32 {
    let clamped = if value == 0 { 1 } else { value };
    clamped.div_ceil(BLOOM_DOWNSAMPLE)
}

const fn texture_extent(size: [u32; 2]) -> Extent3d {
    Extent3d {
        width: size[0],
        height: size[1],
        depth_or_array_layers: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::downsampled_extent;

    #[test]
    fn bloom_extent_rounds_up_and_never_reaches_zero() {
        assert_eq!(downsampled_extent(0), 1);
        assert_eq!(downsampled_extent(1), 1);
        assert_eq!(downsampled_extent(4), 1);
        assert_eq!(downsampled_extent(5), 2);
        assert_eq!(downsampled_extent(1_921), 481);
    }
}
