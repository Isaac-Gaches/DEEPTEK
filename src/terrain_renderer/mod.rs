mod decorations;
mod furniture;
mod lighting;
mod lut;
mod mesh_builder;

pub(crate) use furniture::LaserParticleEmitter;
pub use lighting::{LightSource, LightingUpdateStats};
pub use mesh_builder::{ChunkMeshData, TerrainVertex, build_chunk_mesh};

use crate::{CHUNK_SIZE, ChunkPos, Layer, PowerSystem, TileId, World, WorldError};
use easy_gpu::assets::{
    Buffer, BufferUsages, GpuVertex, Material, MaterialBuilder, Mesh, RenderPipelineBuilder,
    Sampler, SamplerBuilder, Texture, render_texture, render_uniform, sampler,
};
use easy_gpu::assets_manager::Handle;
use easy_gpu::frame::Frame;
use easy_gpu::wgpu::{BlendState, FilterMode, TextureFormat};
use mesh_builder::append_chunk_mesh;
use rayon::prelude::*;
use std::collections::HashMap;

pub struct TerrainRenderConfig {
    pub horizontal_chunk_radius: u32,
    pub vertical_chunk_radius: u32,
    pub unload_margin: u32,
    pub mesh_layer_budget_per_frame: usize,
    pub worker_threads: usize,
}

impl Default for TerrainRenderConfig {
    fn default() -> Self {
        Self {
            horizontal_chunk_radius: 5,
            vertical_chunk_radius: 3,
            unload_margin: 1,
            mesh_layer_budget_per_frame: 12,
            worker_threads: std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MeshSyncStats {
    pub spawned_chunks: usize,
    pub despawned_chunks: usize,
    pub rebuilt_layers: usize,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    position: [f32; 2],
    scale: [f32; 2],
}

#[derive(Default)]
struct RenderChunk {
    meshes: [ChunkMeshData; 2],
    dirty: [bool; 2],
}

impl RenderChunk {
    fn newly_spawned() -> Self {
        Self {
            meshes: std::array::from_fn(|_| ChunkMeshData::default()),
            dirty: [true, true],
        }
    }
}

#[derive(Default)]
struct RenderRegion {
    meshes: [Option<Handle<Mesh>>; 2],
    dirty: [bool; 2],
}

pub struct TerrainRenderer {
    config: TerrainRenderConfig,
    chunks: HashMap<ChunkPos, RenderChunk>,
    regions: HashMap<ChunkPos, RenderRegion>,
    despawn_scratch: Vec<ChunkPos>,
    mesh_job_scratch: Vec<(ChunkPos, Layer)>,
    region_job_scratch: Vec<(ChunkPos, Layer)>,
    region_mesh_scratch: ChunkMeshData,
    materials: [Handle<Material>; 2],
    camera_buffer: Handle<Buffer>,
    decorations: decorations::DecorationRenderer,
    furniture: furniture::FurnitureRenderer,
    lighting: lighting::LightingEngine,
    lighting_dirty: bool,
    lighting_occupancy_dirty: bool,
    furniture_beams_dirty: bool,
    mesh_pool: rayon::ThreadPool,
    last_object_revision: u64,
    last_power_revision: u64,
    visual_time_seconds: f32,
}

impl TerrainRenderer {
    pub fn new(gpu: &mut easy_gpu::Renderer, config: TerrainRenderConfig) -> Self {
        let camera_buffer = gpu.create_buffer(
            BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            size_of::<CameraUniform>() as u64,
        );
        let lighting = lighting::LightingEngine::new(
            gpu,
            config.horizontal_chunk_radius,
            config.vertical_chunk_radius,
        );
        let decorations = decorations::DecorationRenderer::new(gpu, camera_buffer, &lighting);
        let furniture = furniture::FurnitureRenderer::new(gpu, camera_buffer, &lighting);
        let shader = gpu.load_shader(include_str!("shader.wgsl"));
        let foreground_pipeline = RenderPipelineBuilder::new(shader)
            .material_layout(&[
                render_uniform(0),
                render_texture(1),
                sampler(2),
                render_texture(3),
                sampler(4),
                render_uniform(5),
            ])
            .vertex_layout(TerrainVertex::buffer_layout())
            .depth_format(TextureFormat::Depth24Plus)
            .blend_mode(BlendState::REPLACE)
            .fs_entry_point("fs_foreground")
            .build(gpu);
        let background_pipeline = RenderPipelineBuilder::new(shader)
            .material_layout(&[
                render_uniform(0),
                render_texture(1),
                sampler(2),
                render_texture(3),
                sampler(4),
                render_uniform(5),
                render_texture(6),
            ])
            .vertex_layout(TerrainVertex::buffer_layout())
            .depth_format(TextureFormat::Depth24Plus)
            .blend_mode(BlendState::REPLACE)
            .fs_entry_point("fs_background")
            .build(gpu);
        let foreground_texture = gpu
            .load_texture_from_file(include_bytes!("../../assets/terrain/fg_tiles.png").to_vec());
        let background_texture = gpu
            .load_texture_from_file(include_bytes!("../../assets/terrain/bg_tiles.png").to_vec());
        let texture_sampler = SamplerBuilder::new()
            .filter_mode(FilterMode::Nearest)
            .build(gpu);
        let foreground_material = MaterialBuilder::new(foreground_pipeline)
            .buffer(0, camera_buffer)
            .texture(1, foreground_texture)
            .sampler(2, texture_sampler)
            .texture(3, lighting.light_texture)
            .sampler(4, lighting.light_sampler)
            .buffer(5, lighting.light_meta_buffer)
            .build(gpu);
        let background_material = MaterialBuilder::new(background_pipeline)
            .buffer(0, camera_buffer)
            .texture(1, background_texture)
            .sampler(2, texture_sampler)
            .texture(3, lighting.light_texture)
            .sampler(4, lighting.light_sampler)
            .buffer(5, lighting.light_meta_buffer)
            .texture(6, lighting.occlusion_texture)
            .build(gpu);
        let mesh_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(config.worker_threads.max(1))
            .thread_name(|index| format!("terrain-mesh-{index}"))
            .build()
            .expect("failed to create terrain mesh worker pool");
        let mut renderer = Self {
            config: TerrainRenderConfig {
                worker_threads: config.worker_threads.max(1),
                mesh_layer_budget_per_frame: config.mesh_layer_budget_per_frame.max(1),
                ..config
            },
            chunks: HashMap::new(),
            regions: HashMap::new(),
            despawn_scratch: Vec::new(),
            mesh_job_scratch: Vec::new(),
            region_job_scratch: Vec::new(),
            region_mesh_scratch: ChunkMeshData::default(),
            materials: [background_material, foreground_material],
            camera_buffer,
            decorations,
            furniture,
            lighting,
            lighting_dirty: true,
            lighting_occupancy_dirty: true,
            furniture_beams_dirty: true,
            mesh_pool,
            last_object_revision: u64::MAX,
            last_power_revision: u64::MAX,
            visual_time_seconds: 0.0,
        };
        renderer.update_camera(gpu, [0.0, 0.0], 55.0);
        renderer
    }

    pub fn update_camera(
        &mut self,
        gpu: &easy_gpu::Renderer,
        player_position: [f32; 2],
        vertical_tiles_visible: f32,
    ) {
        let half_height = (vertical_tiles_visible * 0.5).max(1.0);
        let half_width = half_height * gpu.window_aspect().max(0.01);
        gpu.write_buffer(
            self.camera_buffer,
            CameraUniform {
                position: [player_position[0], -player_position[1]],
                scale: [1.0 / half_width, 1.0 / half_height],
            },
        );
    }

    pub(crate) const fn camera_buffer(&self) -> Handle<Buffer> {
        self.camera_buffer
    }

    pub(crate) const fn light_texture(&self) -> Handle<Texture> {
        self.lighting.light_texture
    }

    pub(crate) const fn light_sampler(&self) -> Handle<Sampler> {
        self.lighting.light_sampler
    }

    pub(crate) const fn light_meta_buffer(&self) -> Handle<Buffer> {
        self.lighting.light_meta_buffer
    }

    pub fn sync(
        &mut self,
        gpu: &mut easy_gpu::Renderer,
        world: &World,
        power: &PowerSystem,
        player_position: [f32; 2],
    ) -> MeshSyncStats {
        let player_chunk = player_chunk(world, player_position);
        let desired = chunk_bounds(
            world,
            player_chunk,
            self.config.horizontal_chunk_radius,
            self.config.vertical_chunk_radius,
        );
        let retained = chunk_bounds(
            world,
            player_chunk,
            self.config.horizontal_chunk_radius + self.config.unload_margin,
            self.config.vertical_chunk_radius + self.config.unload_margin,
        );
        let mut stats = MeshSyncStats::default();

        self.despawn_scratch.clear();
        self.despawn_scratch.extend(
            self.chunks
                .keys()
                .copied()
                .filter(|position| !retained.contains(position)),
        );
        while let Some(position) = self.despawn_scratch.pop() {
            if self.chunks.remove(&position).is_some() {
                self.mark_region_dirty(position, Layer::Background);
                self.mark_region_dirty(position, Layer::Foreground);
                stats.despawned_chunks += 1;
            }
        }
        for y in desired.min_y..=desired.max_y {
            for x in desired.min_x..=desired.max_x {
                let position = ChunkPos { x, y };
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    self.chunks.entry(position)
                {
                    entry.insert(RenderChunk::newly_spawned());
                    stats.spawned_chunks += 1;
                }
            }
        }

        self.mesh_job_scratch.clear();
        for (position, chunk) in &self.chunks {
            for layer in [Layer::Background, Layer::Foreground] {
                if chunk.dirty[layer_index(layer)] {
                    self.mesh_job_scratch.push((*position, layer));
                }
            }
        }
        self.mesh_job_scratch
            .sort_unstable_by_key(|(position, layer)| {
                let dx = position.x.abs_diff(player_chunk.x);
                let dy = position.y.abs_diff(player_chunk.y);
                (dx + dy, layer_index(*layer))
            });
        self.mesh_job_scratch
            .truncate(self.config.mesh_layer_budget_per_frame);

        let generated = build_jobs(&self.mesh_pool, world, &self.mesh_job_scratch);
        for (position, layer, mesh_data) in generated {
            let index = layer_index(layer);
            if let Some(chunk) = self.chunks.get_mut(&position) {
                chunk.meshes[index] = mesh_data;
                chunk.dirty[index] = false;
                self.mark_region_dirty(position, layer);
                stats.rebuilt_layers += 1;
            }
        }
        self.rebuild_dirty_regions(gpu);

        let object_revision = world.object_revision();
        let power_revision = power.revision();
        if stats.spawned_chunks > 0
            || stats.despawned_chunks > 0
            || object_revision != self.last_object_revision
            || power_revision != self.last_power_revision
        {
            self.decorations.sync(world, self.chunks.keys().copied());
            self.furniture
                .sync(world, power, self.chunks.keys().copied());
            self.furniture_beams_dirty = false;
            self.last_object_revision = object_revision;
            self.last_power_revision = power_revision;
            self.lighting_dirty = true;
        } else if self.furniture_beams_dirty {
            self.furniture
                .sync_laser_beams(world, power, self.chunks.keys().copied());
            self.furniture_beams_dirty = false;
            self.lighting_dirty = true;
        }
        stats
    }

    pub fn draw(&self, frame: &mut Frame) {
        for layer in [Layer::Background, Layer::Foreground] {
            let index = layer_index(layer);
            for region in self.regions.values() {
                if let Some(mesh) = region.meshes[index] {
                    frame.draw(self.materials[index], mesh);
                }
            }
        }
        self.decorations.draw(frame);
        self.furniture.draw(frame);
    }

    pub(crate) fn laser_particle_emitters(&self) -> &[furniture::LaserParticleEmitter] {
        self.furniture.laser_particle_emitters()
    }

    pub fn update_lighting(
        &mut self,
        gpu: &easy_gpu::Renderer,
        world: &World,
        player_position: [f32; 2],
        dynamic_lights: &[LightSource],
    ) -> LightingUpdateStats {
        self.furniture
            .update_flickering_lights(self.visual_time_seconds);
        let stats = self.lighting.update_inputs(
            gpu,
            world,
            player_position,
            dynamic_lights,
            self.furniture.flickering_lights(),
            self.lighting_occupancy_dirty,
        );
        self.lighting_dirty = false;
        self.lighting_occupancy_dirty = false;
        stats
    }

    pub fn advance_visual_time(&mut self, elapsed_seconds: f32) {
        if elapsed_seconds.is_finite() && elapsed_seconds > 0.0 {
            self.visual_time_seconds =
                (self.visual_time_seconds + elapsed_seconds.min(0.25)).rem_euclid(4_096.0);
        }
    }

    pub fn compute_lighting(&self, frame: &mut Frame) {
        self.lighting.compute(frame);
    }

    pub fn set_sky_light(&mut self, gpu: &easy_gpu::Renderer, colour: [f32; 3]) {
        self.lighting.set_sky_light(gpu, colour);
        self.lighting_dirty = true;
    }

    pub const fn lighting_needs_refresh(&self) -> bool {
        self.lighting_dirty
    }

    pub fn set_tile(
        &mut self,
        world: &mut World,
        x: u32,
        y: u32,
        layer: Layer,
        tile: TileId,
    ) -> Result<(), WorldError> {
        world.set_tile(x, y, layer, tile)?;
        self.mark_tile_dirty(x, y, layer);
        Ok(())
    }

    /// Call this after modifying the world through another system.
    pub fn mark_tile_dirty(&mut self, x: u32, y: u32, layer: Layer) {
        self.lighting_dirty = true;
        self.lighting_occupancy_dirty = true;
        if layer == Layer::Foreground {
            self.furniture_beams_dirty = true;
        }
        for position in chunks_affected_by_tile(x, y) {
            if let Some(chunk) = self.chunks.get_mut(&position) {
                chunk.dirty[layer_index(layer)] = true;
            }
        }
    }

    pub fn loaded_chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn clear_meshes(&mut self, gpu: &mut easy_gpu::Renderer) {
        self.chunks.clear();
        for (_, region) in self.regions.drain() {
            remove_meshes(gpu, region.meshes);
        }
    }

    fn mark_region_dirty(&mut self, chunk: ChunkPos, layer: Layer) {
        self.regions
            .entry(region_position(chunk))
            .or_default()
            .dirty[layer_index(layer)] = true;
    }

    fn rebuild_dirty_regions(&mut self, gpu: &mut easy_gpu::Renderer) {
        self.region_job_scratch.clear();
        for (&position, region) in &self.regions {
            for layer in [Layer::Background, Layer::Foreground] {
                if region.dirty[layer_index(layer)] {
                    self.region_job_scratch.push((position, layer));
                }
            }
        }

        for (position, layer) in self.region_job_scratch.drain(..) {
            self.region_mesh_scratch.vertices.clear();
            self.region_mesh_scratch.indices.clear();
            let index = layer_index(layer);
            for chunk_position in chunks_in_region(position) {
                if let Some(chunk) = self.chunks.get(&chunk_position) {
                    append_chunk_mesh(&mut self.region_mesh_scratch, &chunk.meshes[index]);
                }
            }
            let region = self
                .regions
                .get_mut(&position)
                .expect("dirty region must still exist");
            if let Some(old_mesh) = region.meshes[index].take() {
                gpu.asset_manager.meshes.remove(old_mesh);
            }
            region.meshes[index] = (!self.region_mesh_scratch.indices.is_empty()).then(|| {
                gpu.create_mesh(
                    &self.region_mesh_scratch.vertices,
                    &self.region_mesh_scratch.indices,
                )
            });
            region.dirty[index] = false;
        }

        self.despawn_scratch.clear();
        self.despawn_scratch.extend(
            self.regions
                .keys()
                .copied()
                .filter(|&position| !region_has_chunks(&self.chunks, position)),
        );
        while let Some(position) = self.despawn_scratch.pop() {
            if let Some(region) = self.regions.remove(&position) {
                remove_meshes(gpu, region.meshes);
            }
        }
    }
}

const REGION_CHUNKS: u32 = 2;

fn region_position(chunk: ChunkPos) -> ChunkPos {
    ChunkPos {
        x: chunk.x / REGION_CHUNKS,
        y: chunk.y / REGION_CHUNKS,
    }
}

fn chunks_in_region(region: ChunkPos) -> impl Iterator<Item = ChunkPos> {
    let origin_x = region.x * REGION_CHUNKS;
    let origin_y = region.y * REGION_CHUNKS;
    (0..REGION_CHUNKS).flat_map(move |offset_y| {
        (0..REGION_CHUNKS).map(move |offset_x| ChunkPos {
            x: origin_x + offset_x,
            y: origin_y + offset_y,
        })
    })
}

fn region_has_chunks(chunks: &HashMap<ChunkPos, RenderChunk>, region: ChunkPos) -> bool {
    chunks_in_region(region).any(|position| chunks.contains_key(&position))
}

fn chunks_affected_by_tile(x: u32, y: u32) -> impl Iterator<Item = ChunkPos> {
    let own = ChunkPos {
        x: x / CHUNK_SIZE as u32,
        y: y / CHUNK_SIZE as u32,
    };
    let x_offset = match x as usize % CHUNK_SIZE {
        0 => Some(-1),
        value if value == CHUNK_SIZE - 1 => Some(1),
        _ => None,
    };
    let y_offset = match y as usize % CHUNK_SIZE {
        0 => Some(-1),
        value if value == CHUNK_SIZE - 1 => Some(1),
        _ => None,
    };
    let adjacent = |dx: i32, dy: i32| {
        own.x
            .checked_add_signed(dx)
            .zip(own.y.checked_add_signed(dy))
            .map(|(x, y)| ChunkPos { x, y })
    };
    [
        Some(own),
        y_offset.and_then(|dy| adjacent(0, dy)),
        x_offset.and_then(|dx| adjacent(dx, 0)),
        x_offset.zip(y_offset).and_then(|(dx, dy)| adjacent(dx, dy)),
    ]
    .into_iter()
    .flatten()
}

fn build_jobs(
    pool: &rayon::ThreadPool,
    world: &World,
    jobs: &[(ChunkPos, Layer)],
) -> Vec<(ChunkPos, Layer, ChunkMeshData)> {
    pool.install(|| {
        jobs.par_iter()
            .map(|&(position, layer)| (position, layer, build_chunk_mesh(world, position, layer)))
            .collect()
    })
}

fn player_chunk(world: &World, player: [f32; 2]) -> ChunkPos {
    let x = player[0].floor().max(0.0) as u32;
    let y = player[1].floor().max(0.0) as u32;
    ChunkPos {
        x: (x / CHUNK_SIZE as u32).min(world.chunks_wide() - 1),
        y: (y / CHUNK_SIZE as u32).min(world.chunks_high() - 1),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ChunkBounds {
    min_x: u32,
    max_x: u32,
    min_y: u32,
    max_y: u32,
}

impl ChunkBounds {
    fn contains(self, position: &ChunkPos) -> bool {
        (self.min_x..=self.max_x).contains(&position.x)
            && (self.min_y..=self.max_y).contains(&position.y)
    }

    #[cfg(test)]
    fn len(self) -> usize {
        ((self.max_x - self.min_x + 1) * (self.max_y - self.min_y + 1)) as usize
    }
}

fn chunk_bounds(
    world: &World,
    centre: ChunkPos,
    horizontal_radius: u32,
    vertical_radius: u32,
) -> ChunkBounds {
    let min_x = centre.x.saturating_sub(horizontal_radius);
    let max_x = centre
        .x
        .saturating_add(horizontal_radius)
        .min(world.chunks_wide() - 1);
    let min_y = centre.y.saturating_sub(vertical_radius);
    let max_y = centre
        .y
        .saturating_add(vertical_radius)
        .min(world.chunks_high() - 1);
    ChunkBounds {
        min_x,
        max_x,
        min_y,
        max_y,
    }
}

fn layer_index(layer: Layer) -> usize {
    match layer {
        Layer::Background => 0,
        Layer::Foreground => 1,
    }
}

fn remove_meshes(gpu: &mut easy_gpu::Renderer, meshes: [Option<Handle<Mesh>>; 2]) {
    for mesh in meshes.into_iter().flatten() {
        gpu.asset_manager.meshes.remove(mesh);
    }
}

#[cfg(test)]
mod tests {
    use super::lut::MARCHING_SQUARES_LUT;
    use super::*;
    use crate::{BackgroundTile, ForegroundTile};
    use std::collections::HashSet;

    #[test]
    fn one_solid_tile_produces_one_quad() {
        let mut world = World::empty(64, 64, 0).unwrap();
        world
            .set_tile(10, 10, Layer::Foreground, ForegroundTile::DIRT)
            .unwrap();
        let mesh = build_chunk_mesh(&world, ChunkPos { x: 0, y: 0 }, Layer::Foreground);
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.indices.len(), 6);
    }

    #[test]
    fn edge_chunks_mesh_only_valid_world_tiles() {
        let mut world = World::empty(65, 65, 0).unwrap();
        world
            .set_tile(64, 64, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        let mesh = build_chunk_mesh(&world, ChunkPos { x: 1, y: 1 }, Layer::Foreground);
        assert_eq!(mesh.vertices.len(), 4);
    }

    #[test]
    fn each_layer_uses_its_actual_atlas_height() {
        let mut world = World::empty(1, 1, 0).unwrap();
        world
            .set_tile(0, 0, Layer::Foreground, ForegroundTile::DIRT)
            .unwrap();
        world
            .set_tile(0, 0, Layer::Background, BackgroundTile::DIRT_WALL)
            .unwrap();
        let foreground = build_chunk_mesh(&world, ChunkPos { x: 0, y: 0 }, Layer::Foreground);
        let background = build_chunk_mesh(&world, ChunkPos { x: 0, y: 0 }, Layer::Background);
        let foreground_height = foreground.vertices[0].uv[1] - foreground.vertices[3].uv[1];
        let background_height = background.vertices[0].uv[1] - background.vertices[3].uv[1];
        assert!((foreground_height - 1.0 / 21.0).abs() < f32::EPSILON);
        assert!((background_height - 1.0 / 14.0).abs() < f32::EPSILON);
    }

    #[test]
    fn visible_chunk_set_is_clipped_to_world() {
        let world = World::empty(130, 130, 0).unwrap();
        let visible = chunk_bounds(&world, ChunkPos { x: 0, y: 0 }, 1, 1);
        assert_eq!(visible.len(), 4);
        assert!(visible.contains(&ChunkPos { x: 1, y: 1 }));
    }

    #[test]
    fn old_lut_values_are_retained() {
        assert_eq!(MARCHING_SQUARES_LUT[0], [0.0, 0.0]);
        assert_eq!(MARCHING_SQUARES_LUT[26], [6.0, 0.0]);
        assert_eq!(MARCHING_SQUARES_LUT[255], [1.0, 2.0]);
    }

    #[test]
    fn edits_dirty_only_chunks_whose_neighbour_masks_can_change() {
        assert_eq!(
            chunks_affected_by_tile(10, 10).collect::<Vec<_>>(),
            vec![ChunkPos { x: 0, y: 0 }]
        );
        let affected: HashSet<_> = chunks_affected_by_tile(64, 64).collect();
        assert_eq!(
            affected,
            HashSet::from([
                ChunkPos { x: 1, y: 1 },
                ChunkPos { x: 0, y: 1 },
                ChunkPos { x: 1, y: 0 },
                ChunkPos { x: 0, y: 0 },
            ])
        );
    }

    #[test]
    fn regions_group_four_adjacent_chunks() {
        let region = ChunkPos { x: 3, y: 2 };
        let chunks: HashSet<_> = chunks_in_region(region).collect();
        assert_eq!(chunks.len(), 4);
        assert!(chunks.contains(&ChunkPos { x: 6, y: 4 }));
        assert!(chunks.contains(&ChunkPos { x: 7, y: 5 }));
        assert_eq!(region_position(ChunkPos { x: 7, y: 5 }), region);
    }
}
