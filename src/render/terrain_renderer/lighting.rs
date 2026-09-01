use crate::{CHUNK_SIZE, Layer, TileId, TilePos, WORLD_REGION_SIZE, World, block_definition};
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
const INCREMENTAL_TILE_LIMIT: usize = 512;

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
    smooth_texture_b: Handle<Texture>,
    pub(super) occlusion_texture: Handle<Texture>,
    tile_texture: Handle<Texture>,

    smooth_pipeline: Handle<ComputePipeline>,
    diffuse_pipeline: Handle<ComputePipeline>,
    sky_pipeline: Handle<ComputePipeline>,
    sources_pipeline: Handle<ComputePipeline>,
    upscale_pipeline: Handle<ComputePipeline>,

    smooth_a_to_b: Handle<ComputeBindGroup>,
    smooth_b_to_a: Handle<ComputeBindGroup>,
    tile_smooth_a_to_b: Handle<ComputeBindGroup>,
    tile_smooth_b_to_a: Handle<ComputeBindGroup>,
    diffuse_a_to_b: Handle<ComputeBindGroup>,
    diffuse_b_to_a: Handle<ComputeBindGroup>,
    sky_bind_group: Handle<ComputeBindGroup>,
    sources_bind_group: Handle<ComputeBindGroup>,
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
    dirty_tiles: Vec<TilePos>,
}

impl LightingEngine {
    pub(super) fn new(
        gpu: &mut easy_gpu::Renderer,
        horizontal_radius: u32,
        vertical_radius: u32,
    ) -> Self {
        let tile_dimensions = lighting_tile_dimensions(horizontal_radius, vertical_radius);
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
                    storage_texture(4, TextureFormat::Rgba8Unorm),
                ])
                .entry_point("set_sky_light")
                .build(gpu);
        let sky_bind_group = ComputeBindGroupBuilder::new(sky_pipeline)
            .texture(0, diffuse_texture_b)
            .texture(1, diffuse_texture_a)
            .texture(2, tile_texture)
            .buffer(3, sky_light_buffer)
            .texture(4, occlusion_texture)
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
            chunk_size: WORLD_REGION_SIZE as f32,
            _padding: 0.0,
        };

        let mut engine = Self {
            light_texture,
            diffuse_texture_a,
            diffuse_texture_b,
            smooth_texture_b,
            occlusion_texture,
            tile_texture,
            smooth_pipeline,
            diffuse_pipeline,
            sky_pipeline,
            sources_pipeline,
            upscale_pipeline,
            smooth_a_to_b,
            smooth_b_to_a,
            tile_smooth_a_to_b,
            tile_smooth_b_to_a,
            diffuse_a_to_b,
            diffuse_b_to_a,
            sky_bind_group,
            sources_bind_group,
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
            dirty_tiles: Vec::new(),
        };
        engine.set_sky_light(gpu, [0.8, 0.85, 1.0]);
        gpu.write_buffer(engine.light_meta_buffer, engine.light_meta);
        engine
    }

    pub(super) fn resize(
        &mut self,
        gpu: &mut easy_gpu::Renderer,
        horizontal_radius: u32,
        vertical_radius: u32,
    ) -> Option<[Handle<Texture>; 6]> {
        if self.horizontal_radius == horizontal_radius && self.vertical_radius == vertical_radius {
            return None;
        }

        let tile_dimensions = lighting_tile_dimensions(horizontal_radius, vertical_radius);
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

        let diffuse_a_to_b = ComputeBindGroupBuilder::new(self.diffuse_pipeline)
            .texture(0, diffuse_texture_a)
            .texture(1, diffuse_texture_b)
            .texture(2, tile_texture)
            .build(gpu);
        let diffuse_b_to_a = ComputeBindGroupBuilder::new(self.diffuse_pipeline)
            .texture(0, diffuse_texture_b)
            .texture(1, diffuse_texture_a)
            .texture(2, tile_texture)
            .build(gpu);
        let sky_bind_group = ComputeBindGroupBuilder::new(self.sky_pipeline)
            .texture(0, diffuse_texture_b)
            .texture(1, diffuse_texture_a)
            .texture(2, tile_texture)
            .buffer(3, self.sky_light_buffer)
            .texture(4, occlusion_texture)
            .build(gpu);
        let sources_bind_group = ComputeBindGroupBuilder::new(self.sources_pipeline)
            .texture(0, diffuse_texture_b)
            .buffer(1, self.lights_buffer)
            .buffer(2, self.source_meta_buffer)
            .build(gpu);
        let smooth_a_to_b = ComputeBindGroupBuilder::new(self.smooth_pipeline)
            .texture(0, light_texture)
            .texture(1, smooth_texture_b)
            .build(gpu);
        let smooth_b_to_a = ComputeBindGroupBuilder::new(self.smooth_pipeline)
            .texture(0, smooth_texture_b)
            .texture(1, light_texture)
            .build(gpu);
        let tile_smooth_a_to_b = ComputeBindGroupBuilder::new(self.smooth_pipeline)
            .texture(0, diffuse_texture_a)
            .texture(1, diffuse_texture_b)
            .build(gpu);
        let tile_smooth_b_to_a = ComputeBindGroupBuilder::new(self.smooth_pipeline)
            .texture(0, diffuse_texture_b)
            .texture(1, diffuse_texture_a)
            .build(gpu);
        let upscale_bind_group = ComputeBindGroupBuilder::new(self.upscale_pipeline)
            .texture(0, diffuse_texture_a)
            .texture(1, light_texture)
            .build(gpu);

        for bind_group in [
            self.smooth_a_to_b,
            self.smooth_b_to_a,
            self.tile_smooth_a_to_b,
            self.tile_smooth_b_to_a,
            self.diffuse_a_to_b,
            self.diffuse_b_to_a,
            self.sky_bind_group,
            self.sources_bind_group,
            self.upscale_bind_group,
        ] {
            let _ = gpu.asset_manager.compute_bind_groups.remove(bind_group);
        }
        let retired = [
            self.light_texture,
            self.diffuse_texture_a,
            self.diffuse_texture_b,
            self.smooth_texture_b,
            self.occlusion_texture,
            self.tile_texture,
        ];
        self.light_texture = light_texture;
        self.diffuse_texture_a = diffuse_texture_a;
        self.diffuse_texture_b = diffuse_texture_b;
        self.smooth_texture_b = smooth_texture_b;
        self.occlusion_texture = occlusion_texture;
        self.tile_texture = tile_texture;
        self.smooth_a_to_b = smooth_a_to_b;
        self.smooth_b_to_a = smooth_b_to_a;
        self.tile_smooth_a_to_b = tile_smooth_a_to_b;
        self.tile_smooth_b_to_a = tile_smooth_b_to_a;
        self.diffuse_a_to_b = diffuse_a_to_b;
        self.diffuse_b_to_a = diffuse_b_to_a;
        self.sky_bind_group = sky_bind_group;
        self.sources_bind_group = sources_bind_group;
        self.upscale_bind_group = upscale_bind_group;
        self.horizontal_radius = horizontal_radius;
        self.vertical_radius = vertical_radius;
        self.tile_dimensions = tile_dimensions;
        self.smooth_dimensions = smooth_dimensions;
        self.light_meta.vertical_render_distance = (vertical_radius * CHUNK_SIZE as u32) as f32;
        self.light_meta.horizontal_render_distance = (horizontal_radius * CHUNK_SIZE as u32) as f32;
        gpu.write_buffer(self.light_meta_buffer, self.light_meta);
        self.last_occupancy_anchor = None;
        self.occupancy_buffer
            .resize((tile_dimensions[0] * tile_dimensions[1]) as usize, 1);
        self.occupancy_buffer.fill(1);
        self.tile_lights.clear();
        self.combined_lights.clear();
        self.dirty_tiles.clear();
        self.light_count = 0;
        Some(retired)
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
            (player_position[0] / WORLD_REGION_SIZE as f32).floor() * WORLD_REGION_SIZE as f32,
            (player_position[1] / WORLD_REGION_SIZE as f32).floor() * WORLD_REGION_SIZE as f32,
        ];
        self.light_meta.anchor = anchor;
        gpu.write_buffer(self.light_meta_buffer, self.light_meta);

        let integer_anchor = [anchor[0] as i32, anchor[1] as i32];
        if occupancy_dirty {
            self.dirty_tiles.sort_unstable();
            self.dirty_tiles.dedup();
        }
        let anchor_changed = self.last_occupancy_anchor != Some(integer_anchor);
        if anchor_changed
            || (occupancy_dirty
                && (self.dirty_tiles.is_empty() || self.dirty_tiles.len() > INCREMENTAL_TILE_LIMIT))
        {
            prepare_lighting_window(
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
        } else if occupancy_dirty
            && update_lighting_window_cells(
                world,
                anchor,
                self.horizontal_radius,
                self.vertical_radius,
                &self.dirty_tiles,
                &mut self.occupancy_buffer,
                &mut self.tile_lights,
            ) > 0
        {
            gpu.write_texture(
                self.tile_texture,
                &self.occupancy_buffer,
                1,
                extent(self.tile_dimensions),
            );
        }
        self.dirty_tiles.clear();

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

    pub(super) fn mark_tile_dirty(&mut self, position: TilePos) {
        self.dirty_tiles.push(position);
    }

    pub(super) const fn tile_dimensions(&self) -> [u32; 2] {
        self.tile_dimensions
    }

    pub(super) fn compute(&self, frame: &mut Frame) {
        // The fused sky pass overwrites every texel in A before diffusion reads it.
        // Only B must be cleared before sparse light sources are written into it.
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

/// Builds the CPU-side occupancy and emissive-tile inputs consumed by the
/// lighting compute passes. Kept public so the exact production path can be
/// exercised by the headless performance suite.
#[doc(hidden)]
pub fn prepare_lighting_window(
    world: &World,
    anchor: [f32; 2],
    horizontal_radius: u32,
    vertical_radius: u32,
    tiles: &mut Vec<u8>,
    lights: &mut Vec<LightSource>,
) {
    let width = horizontal_radius * 2 * CHUNK_SIZE as u32 + WORLD_REGION_SIZE;
    let height = vertical_radius * 2 * CHUNK_SIZE as u32 + WORLD_REGION_SIZE;
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

/// Applies localized world edits to a previously prepared lighting window.
/// Returns the number of unique edited cells that overlapped the window.
#[doc(hidden)]
pub fn update_lighting_window_cells(
    world: &World,
    anchor: [f32; 2],
    horizontal_radius: u32,
    vertical_radius: u32,
    dirty_tiles: &[TilePos],
    tiles: &mut [u8],
    lights: &mut Vec<LightSource>,
) -> usize {
    let width = horizontal_radius * 2 * CHUNK_SIZE as u32 + WORLD_REGION_SIZE;
    let height = vertical_radius * 2 * CHUNK_SIZE as u32 + WORLD_REGION_SIZE;
    if tiles.len() != (width * height) as usize {
        return 0;
    }
    let origin_x = anchor[0] as i64 - (horizontal_radius * CHUNK_SIZE as u32) as i64;
    let origin_y = anchor[1] as i64 - (vertical_radius * CHUNK_SIZE as u32) as i64;
    let mut visible = dirty_tiles
        .iter()
        .copied()
        .filter(|position| {
            let local_x = i64::from(position.x) - origin_x;
            let local_y = i64::from(position.y) - origin_y;
            local_x >= 0
                && local_y >= 0
                && local_x < i64::from(width)
                && local_y < i64::from(height)
        })
        .collect::<Vec<_>>();
    visible.sort_unstable();
    visible.dedup();
    if visible.is_empty() {
        return 0;
    }

    lights.retain(|light| {
        let position = light.position();
        visible
            .binary_search(&TilePos::new(position[0] as u32, position[1] as u32))
            .is_err()
    });
    for position in &visible {
        let local_x = (i64::from(position.x) - origin_x) as u32;
        let local_y = (i64::from(position.y) - origin_y) as u32;
        let foreground = world.tile_in_bounds(position.x, position.y, Layer::Foreground);
        let background = world.tile_in_bounds(position.x, position.y, Layer::Background);
        tiles[(local_y * width + local_x) as usize] = if foreground != TileId::EMPTY {
            1
        } else if background != TileId::EMPTY {
            2
        } else {
            0
        };
        if let Some(colour) = tile_light_colour(foreground) {
            lights.push(LightSource::new(
                [position.x as f32, position.y as f32],
                colour,
            ));
        }
    }
    lights.sort_unstable_by(|left, right| {
        left.position()[1]
            .total_cmp(&right.position()[1])
            .then_with(|| left.position()[0].total_cmp(&right.position()[0]))
    });
    visible.len()
}

fn tile_light_colour(tile: TileId) -> Option<[f32; 3]> {
    block_definition(tile).and_then(crate::BlockDefinition::emitted_light)
}

pub const fn lighting_tile_dimensions(horizontal_radius: u32, vertical_radius: u32) -> [u32; 2] {
    [
        horizontal_radius * 2 * CHUNK_SIZE as u32 + WORLD_REGION_SIZE,
        vertical_radius * 2 * CHUNK_SIZE as u32 + WORLD_REGION_SIZE,
    ]
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
    fn render_presets_allocate_distinct_light_map_sizes() {
        let medium = lighting_tile_dimensions(2, 1);
        let high = lighting_tile_dimensions(3, 2);
        assert_eq!(medium, [192, 128]);
        assert_eq!(high, [256, 192]);
        assert_eq!(medium[0] * medium[1] * 2, high[0] * high[1]);
    }

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
        prepare_lighting_window(&world, [0.0, 0.0], 0, 0, &mut tiles, &mut lights);
        assert_eq!(tiles[0], 0);
        assert_eq!(tiles[WORLD_REGION_SIZE as usize + 1], 1);
        assert_eq!(tiles[WORLD_REGION_SIZE as usize + 2], 2);
    }

    #[test]
    fn localized_lighting_updates_match_a_complete_rebuild() {
        let mut world = World::empty(192, 192, 0).unwrap();
        world
            .set_tile(60, 70, Layer::Foreground, ForegroundTile::DIRT)
            .unwrap();
        world
            .set_tile(61, 70, Layer::Background, BackgroundTile::DIRT_WALL)
            .unwrap();
        let anchor = [64.0, 64.0];
        let mut incremental_tiles = Vec::new();
        let mut incremental_lights = Vec::new();
        prepare_lighting_window(
            &world,
            anchor,
            1,
            1,
            &mut incremental_tiles,
            &mut incremental_lights,
        );

        world
            .set_tile(60, 70, Layer::Foreground, ForegroundTile::ASTERITE)
            .unwrap();
        world
            .set_tile(61, 70, Layer::Background, TileId::EMPTY)
            .unwrap();
        let changed = [
            TilePos::new(60, 70),
            TilePos::new(61, 70),
            TilePos::new(60, 70),
            TilePos::new(4, 4),
        ];
        assert_eq!(
            update_lighting_window_cells(
                &world,
                anchor,
                1,
                1,
                &changed,
                &mut incremental_tiles,
                &mut incremental_lights,
            ),
            2
        );

        let mut rebuilt_tiles = Vec::new();
        let mut rebuilt_lights = Vec::new();
        prepare_lighting_window(
            &world,
            anchor,
            1,
            1,
            &mut rebuilt_tiles,
            &mut rebuilt_lights,
        );
        assert_eq!(incremental_tiles, rebuilt_tiles);
        assert_eq!(incremental_lights, rebuilt_lights);
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
