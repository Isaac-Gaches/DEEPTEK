use crate::{CHUNK_SIZE, Layer, TileId, World, block_definition};
use easy_gpu::assets::{
    Buffer, BufferUsages, ComputeBindGroup, ComputeBindGroupBuilder, ComputePipeline,
    ComputePipelineBuilder, Sampler, SamplerBuilder, Texture, TextureBuilder, compute_storage,
    compute_texture_float, compute_texture_uint, compute_uniform, storage_texture,
};
use easy_gpu::assets_manager::Handle;
use easy_gpu::frame::Frame;
use easy_gpu::wgpu::{Extent3d, FilterMode, TextureFormat, TextureUsages};

const DIFFUSION_ITERATIONS: usize = 12;
const TILE_SMOOTH_ITERATIONS: usize = 1;
const UPSCALED_SMOOTH_ITERATIONS: usize = 1;
const MAX_LIGHTS: usize = 8_192;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LightingUpdateStats {
    pub dynamic_lights: usize,
    pub tile_lights: usize,
    pub truncated_lights: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightSource {
    position: [f32; 2],
    _padding0: [f32; 2],
    pub colour: [f32; 3],
    _padding1: f32,
}

impl LightSource {
    pub fn new(position: [f32; 2], colour: [f32; 3]) -> Self {
        Self {
            position,
            _padding0: [0.0; 2],
            colour,
            _padding1: 0.0,
        }
    }

    pub const fn position(&self) -> [f32; 2] {
        self.position
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LightSourceMeta {
    anchor: [f32; 2],
    light_count: u32,
    _padding: u32,
    midpoint: [i32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct LightMapMeta {
    anchor: [f32; 2],
    vertical_render_distance: f32,
    horizontal_render_distance: f32,
    chunk_size: f32,
    _padding: f32,
}

pub(super) struct LightingEngine {
    pub(super) light_texture: Handle<Texture>,
    diffuse_texture_a: Handle<Texture>,
    diffuse_texture_b: Handle<Texture>,
    pub(super) occlusion_texture: Handle<Texture>,
    tile_texture: Handle<Texture>,

    smooth_pipeline: Handle<ComputePipeline>,
    diffuse_pipeline: Handle<ComputePipeline>,
    sky_pipeline: Handle<ComputePipeline>,
    sources_pipeline: Handle<ComputePipeline>,
    occlusion_pipeline: Handle<ComputePipeline>,
    upscale_pipeline: Handle<ComputePipeline>,

    smooth_a_to_b: Handle<ComputeBindGroup>,
    smooth_b_to_a: Handle<ComputeBindGroup>,
    tile_smooth_a_to_b: Handle<ComputeBindGroup>,
    tile_smooth_b_to_a: Handle<ComputeBindGroup>,
    diffuse_a_to_b: Handle<ComputeBindGroup>,
    diffuse_b_to_a: Handle<ComputeBindGroup>,
    sky_bind_group: Handle<ComputeBindGroup>,
    sources_bind_group: Handle<ComputeBindGroup>,
    occlusion_bind_group: Handle<ComputeBindGroup>,
    upscale_bind_group: Handle<ComputeBindGroup>,

    pub(super) light_sampler: Handle<Sampler>,
    pub(super) light_meta_buffer: Handle<Buffer>,
    light_meta: LightMapMeta,
    sky_light_buffer: Handle<Buffer>,
    source_meta_buffer: Handle<Buffer>,
    lights_buffer: Handle<Buffer>,
    light_count: u32,
    horizontal_radius: u32,
    vertical_radius: u32,
    tile_dimensions: [u32; 2],
    smooth_dimensions: [u32; 2],
    last_occupancy_anchor: Option<[i32; 2]>,
    occupancy_buffer: Vec<u8>,
    tile_lights: Vec<LightSource>,
    combined_lights: Vec<LightSource>,
}

impl LightingEngine {
    pub(super) fn new(
        gpu: &mut easy_gpu::Renderer,
        horizontal_radius: u32,
        vertical_radius: u32,
    ) -> Self {
        let tile_dimensions = [
            (horizontal_radius * 2 + 1) * CHUNK_SIZE as u32,
            (vertical_radius * 2 + 1) * CHUNK_SIZE as u32,
        ];
        let smooth_dimensions = [tile_dimensions[0] * 2, tile_dimensions[1] * 2];
        let texture_usage = TextureUsages::STORAGE_BINDING
            | TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_SRC
            | TextureUsages::COPY_DST
            | TextureUsages::RENDER_ATTACHMENT;
        let make_rgba_texture = |gpu: &mut easy_gpu::Renderer, dimensions: [u32; 2]| {
            TextureBuilder::new()
                .size(extent(dimensions))
                .format(TextureFormat::Rgba8Unorm)
                .usage(texture_usage)
                .build(gpu)
        };
        let diffuse_texture_a = make_rgba_texture(gpu, tile_dimensions);
        let diffuse_texture_b = make_rgba_texture(gpu, tile_dimensions);
        let occlusion_texture = make_rgba_texture(gpu, tile_dimensions);
        let light_texture = make_rgba_texture(gpu, smooth_dimensions);
        let smooth_texture_b = make_rgba_texture(gpu, smooth_dimensions);
        let tile_texture = TextureBuilder::new()
            .size(extent(tile_dimensions))
            .format(TextureFormat::R8Uint)
            .usage(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST)
            .build(gpu);

        let diffuse_pipeline = ComputePipelineBuilder::new(
            gpu.load_shader(include_str!("lighting/diffuse_light.wgsl")),
        )
        .bind_group_layout(&[
            compute_texture_float(0),
            storage_texture(1, TextureFormat::Rgba8Unorm),
            compute_texture_uint(2),
        ])
        .entry_point("diffuse_light")
        .build(gpu);
        let diffuse_a_to_b = ComputeBindGroupBuilder::new(diffuse_pipeline)
            .texture(0, diffuse_texture_a)
            .texture(1, diffuse_texture_b)
            .texture(2, tile_texture)
            .build(gpu);
        let diffuse_b_to_a = ComputeBindGroupBuilder::new(diffuse_pipeline)
            .texture(0, diffuse_texture_b)
            .texture(1, diffuse_texture_a)
            .texture(2, tile_texture)
            .build(gpu);

        let sky_light_buffer = gpu.create_buffer(
            BufferUsages::COPY_DST | BufferUsages::UNIFORM,
            4 * size_of::<f32>() as u64,
        );
        let sky_pipeline =
            ComputePipelineBuilder::new(gpu.load_shader(include_str!("lighting/sky_light.wgsl")))
                .bind_group_layout(&[
                    compute_texture_float(0),
                    storage_texture(1, TextureFormat::Rgba8Unorm),
                    compute_texture_uint(2),
                    compute_uniform(3),
                ])
                .entry_point("set_sky_light")
                .build(gpu);
        let sky_bind_group = ComputeBindGroupBuilder::new(sky_pipeline)
            .texture(0, diffuse_texture_b)
            .texture(1, diffuse_texture_a)
            .texture(2, tile_texture)
            .buffer(3, sky_light_buffer)
            .build(gpu);

        let source_meta_buffer = gpu.create_buffer(
            BufferUsages::COPY_DST | BufferUsages::UNIFORM,
            size_of::<LightSourceMeta>() as u64,
        );
        let lights_buffer = gpu.create_buffer(
            BufferUsages::COPY_DST | BufferUsages::STORAGE,
            (MAX_LIGHTS * size_of::<LightSource>()) as u64,
        );
        let sources_pipeline = ComputePipelineBuilder::new(
            gpu.load_shader(include_str!("lighting/light_sources.wgsl")),
        )
        .bind_group_layout(&[
            storage_texture(0, TextureFormat::Rgba8Unorm),
            compute_storage(1, true),
            compute_uniform(2),
        ])
        .entry_point("set_light_sources")
        .build(gpu);
        let sources_bind_group = ComputeBindGroupBuilder::new(sources_pipeline)
            .texture(0, diffuse_texture_b)
            .buffer(1, lights_buffer)
            .buffer(2, source_meta_buffer)
            .build(gpu);

        let smooth_pipeline = ComputePipelineBuilder::new(
            gpu.load_shader(include_str!("lighting/smooth_light.wgsl")),
        )
        .bind_group_layout(&[
            compute_texture_float(0),
            storage_texture(1, TextureFormat::Rgba8Unorm),
        ])
        .entry_point("smooth_light")
        .build(gpu);
        let smooth_a_to_b = ComputeBindGroupBuilder::new(smooth_pipeline)
            .texture(0, light_texture)
            .texture(1, smooth_texture_b)
            .build(gpu);
        let smooth_b_to_a = ComputeBindGroupBuilder::new(smooth_pipeline)
            .texture(0, smooth_texture_b)
            .texture(1, light_texture)
            .build(gpu);
        let tile_smooth_a_to_b = ComputeBindGroupBuilder::new(smooth_pipeline)
            .texture(0, diffuse_texture_a)
            .texture(1, diffuse_texture_b)
            .build(gpu);
        let tile_smooth_b_to_a = ComputeBindGroupBuilder::new(smooth_pipeline)
            .texture(0, diffuse_texture_b)
            .texture(1, diffuse_texture_a)
            .build(gpu);

        let upscale_pipeline = ComputePipelineBuilder::new(
            gpu.load_shader(include_str!("lighting/upscale_lightmap.wgsl")),
        )
        .bind_group_layout(&[
            compute_texture_float(0),
            storage_texture(1, TextureFormat::Rgba8Unorm),
        ])
        .entry_point("upscale_lightmap")
        .build(gpu);
        let upscale_bind_group = ComputeBindGroupBuilder::new(upscale_pipeline)
            .texture(0, diffuse_texture_a)
            .texture(1, light_texture)
            .build(gpu);

        let occlusion_pipeline = ComputePipelineBuilder::new(
            gpu.load_shader(include_str!("lighting/ambient_occlusion.wgsl")),
        )
        .bind_group_layout(&[
            compute_texture_uint(0),
            storage_texture(1, TextureFormat::Rgba8Unorm),
        ])
        .entry_point("set_occlusion_map")
        .build(gpu);
        let occlusion_bind_group = ComputeBindGroupBuilder::new(occlusion_pipeline)
            .texture(0, tile_texture)
            .texture(1, occlusion_texture)
            .build(gpu);

        let light_sampler = SamplerBuilder::new()
            .filter_mode(FilterMode::Linear)
            .build(gpu);
        let light_meta_buffer = gpu.create_buffer(
            BufferUsages::COPY_DST | BufferUsages::UNIFORM,
            size_of::<LightMapMeta>() as u64,
        );
        let light_meta = LightMapMeta {
            anchor: [0.0; 2],
            vertical_render_distance: (vertical_radius * CHUNK_SIZE as u32) as f32,
            horizontal_render_distance: (horizontal_radius * CHUNK_SIZE as u32) as f32,
            chunk_size: CHUNK_SIZE as f32,
            _padding: 0.0,
        };

        let mut engine = Self {
            light_texture,
            diffuse_texture_a,
            diffuse_texture_b,
            occlusion_texture,
            tile_texture,
            smooth_pipeline,
            diffuse_pipeline,
            sky_pipeline,
            sources_pipeline,
            occlusion_pipeline,
            upscale_pipeline,
            smooth_a_to_b,
            smooth_b_to_a,
            tile_smooth_a_to_b,
            tile_smooth_b_to_a,
            diffuse_a_to_b,
            diffuse_b_to_a,
            sky_bind_group,
            sources_bind_group,
            occlusion_bind_group,
            upscale_bind_group,
            light_sampler,
            light_meta_buffer,
            light_meta,
            sky_light_buffer,
            source_meta_buffer,
            lights_buffer,
            light_count: 0,
            horizontal_radius,
            vertical_radius,
            tile_dimensions,
            smooth_dimensions,
            last_occupancy_anchor: None,
            occupancy_buffer: vec![1; (tile_dimensions[0] * tile_dimensions[1]) as usize],
            tile_lights: Vec::new(),
            combined_lights: Vec::with_capacity(MAX_LIGHTS),
        };
        engine.set_sky_light(gpu, [0.8, 0.85, 1.0]);
        gpu.write_buffer(engine.light_meta_buffer, engine.light_meta);
        engine
    }

    pub(super) fn set_sky_light(&mut self, gpu: &easy_gpu::Renderer, colour: [f32; 3]) {
        gpu.write_buffer(
            self.sky_light_buffer,
            [
                colour[0].clamp(0.0, 1.0),
                colour[1].clamp(0.0, 1.0),
                colour[2].clamp(0.0, 1.0),
                0.0,
            ],
        );
    }

    pub(super) fn update_inputs(
        &mut self,
        gpu: &easy_gpu::Renderer,
        world: &World,
        player_position: [f32; 2],
        dynamic_lights: &[LightSource],
        furniture_lights: &[LightSource],
        occupancy_dirty: bool,
    ) -> LightingUpdateStats {
        let anchor = [
            (player_position[0] / CHUNK_SIZE as f32).floor() * CHUNK_SIZE as f32,
            (player_position[1] / CHUNK_SIZE as f32).floor() * CHUNK_SIZE as f32,
        ];
        self.light_meta.anchor = anchor;
        gpu.write_buffer(self.light_meta_buffer, self.light_meta);

        let integer_anchor = [anchor[0] as i32, anchor[1] as i32];
        if occupancy_dirty || self.last_occupancy_anchor != Some(integer_anchor) {
            fill_lighting_window(
                world,
                anchor,
                self.horizontal_radius,
                self.vertical_radius,
                &mut self.occupancy_buffer,
                &mut self.tile_lights,
            );
            gpu.write_texture(
                self.tile_texture,
                &self.occupancy_buffer,
                1,
                extent(self.tile_dimensions),
            );
            self.last_occupancy_anchor = Some(integer_anchor);
        }

        let requested = dynamic_lights.len() + furniture_lights.len() + self.tile_lights.len();
        self.combined_lights.clear();
        self.combined_lights
            .extend(dynamic_lights.iter().take(MAX_LIGHTS).copied());
        self.combined_lights.extend(
            furniture_lights
                .iter()
                .take(MAX_LIGHTS - self.combined_lights.len())
                .copied(),
        );
        let dynamic_count = self.combined_lights.len();
        self.combined_lights.extend(
            self.tile_lights
                .iter()
                .take(MAX_LIGHTS - self.combined_lights.len())
                .copied(),
        );
        if !self.combined_lights.is_empty() {
            gpu.write_array_buffer(self.lights_buffer, &self.combined_lights);
        }
        self.light_count = self.combined_lights.len() as u32;
        gpu.write_buffer(
            self.source_meta_buffer,
            LightSourceMeta {
                anchor,
                light_count: self.light_count,
                _padding: 0,
                midpoint: [
                    (self.horizontal_radius * CHUNK_SIZE as u32) as i32,
                    (self.vertical_radius * CHUNK_SIZE as u32) as i32,
                ],
            },
        );
        LightingUpdateStats {
            dynamic_lights: dynamic_count,
            tile_lights: self.combined_lights.len() - dynamic_count,
            truncated_lights: requested.saturating_sub(self.combined_lights.len()),
        }
    }

    pub(super) fn compute(&self, frame: &mut Frame) {
        frame.request_texture_clear(self.diffuse_texture_a);
        frame.request_texture_clear(self.diffuse_texture_b);
        if self.light_count > 0 {
            frame.compute(
                self.sources_bind_group,
                self.sources_pipeline,
                (self.light_count.div_ceil(64), 1, 1),
            );
        }
        let tile_dispatch = dispatch_2d(self.tile_dimensions);
        frame.compute(self.sky_bind_group, self.sky_pipeline, tile_dispatch);
        frame.compute(
            self.occlusion_bind_group,
            self.occlusion_pipeline,
            tile_dispatch,
        );
        for _ in 0..DIFFUSION_ITERATIONS {
            frame.compute(self.diffuse_a_to_b, self.diffuse_pipeline, tile_dispatch);
            frame.compute(self.diffuse_b_to_a, self.diffuse_pipeline, tile_dispatch);
        }
        for _ in 0..TILE_SMOOTH_ITERATIONS {
            frame.compute(self.tile_smooth_a_to_b, self.smooth_pipeline, tile_dispatch);
            frame.compute(self.tile_smooth_b_to_a, self.smooth_pipeline, tile_dispatch);
        }
        let smooth_dispatch = dispatch_2d(self.smooth_dimensions);
        frame.compute(
            self.upscale_bind_group,
            self.upscale_pipeline,
            smooth_dispatch,
        );
        for _ in 0..UPSCALED_SMOOTH_ITERATIONS {
            frame.compute(self.smooth_a_to_b, self.smooth_pipeline, smooth_dispatch);
            frame.compute(self.smooth_b_to_a, self.smooth_pipeline, smooth_dispatch);
        }
    }
}

fn fill_lighting_window(
    world: &World,
    anchor: [f32; 2],
    horizontal_radius: u32,
    vertical_radius: u32,
    tiles: &mut Vec<u8>,
    lights: &mut Vec<LightSource>,
) {
    let width = (horizontal_radius * 2 + 1) * CHUNK_SIZE as u32;
    let height = (vertical_radius * 2 + 1) * CHUNK_SIZE as u32;
    let origin_x = anchor[0] as i64 - (horizontal_radius * CHUNK_SIZE as u32) as i64;
    let origin_y = anchor[1] as i64 - (vertical_radius * CHUNK_SIZE as u32) as i64;
    tiles.resize(width as usize * height as usize, 1);
    tiles.fill(1);
    lights.clear();
    for local_y in 0..height {
        let world_y = origin_y + i64::from(local_y);
        for local_x in 0..width {
            let world_x = origin_x + i64::from(local_x);
            if world_x < 0
                || world_y < 0
                || world_x >= i64::from(world.width())
                || world_y >= i64::from(world.height())
            {
                continue;
            }
            let x = world_x as u32;
            let y = world_y as u32;
            let foreground = world.tile_in_bounds(x, y, Layer::Foreground);
            let background = world.tile_in_bounds(x, y, Layer::Background);
            tiles[(local_y * width + local_x) as usize] = if foreground != TileId::EMPTY {
                1
            } else if background != TileId::EMPTY {
                2
            } else {
                0
            };
            if let Some(colour) = tile_light_colour(foreground) {
                lights.push(LightSource::new([x as f32, y as f32], colour));
            }
        }
    }
}

fn tile_light_colour(tile: TileId) -> Option<[f32; 3]> {
    block_definition(tile).and_then(crate::BlockDefinition::emitted_light)
}

fn extent(dimensions: [u32; 2]) -> Extent3d {
    Extent3d {
        width: dimensions[0],
        height: dimensions[1],
        depth_or_array_layers: 1,
    }
}

fn dispatch_2d(dimensions: [u32; 2]) -> (u32, u32, u32) {
    (dimensions[0].div_ceil(16), dimensions[1].div_ceil(16), 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BackgroundTile, ForegroundTile};

    #[test]
    fn occupancy_matches_original_air_foreground_background_encoding() {
        let mut world = World::empty(64, 64, 0).unwrap();
        world
            .set_tile(1, 1, Layer::Foreground, ForegroundTile::DIRT)
            .unwrap();
        world
            .set_tile(2, 1, Layer::Background, BackgroundTile::DIRT_WALL)
            .unwrap();
        let mut tiles = Vec::new();
        let mut lights = Vec::new();
        fill_lighting_window(&world, [0.0, 0.0], 0, 0, &mut tiles, &mut lights);
        assert_eq!(tiles[0], 0);
        assert_eq!(tiles[65], 1);
        assert_eq!(tiles[66], 2);
    }

    #[test]
    fn original_tile_light_colours_are_preserved() {
        assert_eq!(tile_light_colour(TileId::new(4)), Some([1.0, 0.2, 0.2]));
        assert_eq!(tile_light_colour(TileId::new(6)), Some([0.1, 0.4, 0.7]));
        assert_eq!(tile_light_colour(TileId::new(3)), None);
    }

    #[test]
    fn light_source_layout_matches_wgsl_storage_layout() {
        assert_eq!(size_of::<LightSource>(), 32);
        assert_eq!(size_of::<LightSourceMeta>(), 24);
        assert_eq!(size_of::<LightMapMeta>(), 24);
    }
}
