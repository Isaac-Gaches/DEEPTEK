use super::lut::MARCHING_SQUARES_LUT;
use crate::{CHUNK_SIZE, ChunkPos, Layer, TileId, World};
use easy_gpu::assets::{BufferLayout, GpuVertex};
use easy_gpu::wgpu::VertexFormat;

const ATLAS_COLUMNS: f32 = 4.0;
const FOREGROUND_ATLAS_ROWS: f32 = 3.0;
const BACKGROUND_ATLAS_ROWS: f32 = 2.0;
const VARIANTS_PER_AXIS: f32 = 7.0;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TerrainVertex {
    pub position: [f32; 3],
    _padding: f32,
    pub uv: [f32; 2],
}

impl TerrainVertex {
    fn new(position: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            position,
            _padding: 0.0,
            uv,
        }
    }
}

impl GpuVertex for TerrainVertex {
    fn buffer_layout() -> BufferLayout {
        BufferLayout::new()
            .stride(size_of::<Self>() as u64)
            .attribute(0, 0, VertexFormat::Float32x3)
            .attribute(1, 16, VertexFormat::Float32x2)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkMeshData {
    pub vertices: Vec<TerrainVertex>,
    pub indices: Vec<u16>,
}

pub(super) fn append_chunk_mesh(target: &mut ChunkMeshData, source: &ChunkMeshData) {
    let combined_vertices = target.vertices.len() + source.vertices.len();
    assert!(
        combined_vertices <= usize::from(u16::MAX) + 1,
        "a regional terrain mesh exceeded the u16 index limit"
    );
    let base = target.vertices.len() as u32;
    target.vertices.extend_from_slice(&source.vertices);
    target.indices.extend(
        source
            .indices
            .iter()
            .map(|&index| (base + u32::from(index)) as u16),
    );
}

pub fn build_chunk_mesh(world: &World, position: ChunkPos, layer: Layer) -> ChunkMeshData {
    let mut mesh = ChunkMeshData::default();
    build_chunk_mesh_into(world, position, layer, &mut mesh);
    mesh
}

/// Rebuilds a chunk layer while retaining the destination's allocations.
pub fn build_chunk_mesh_into(
    world: &World,
    position: ChunkPos,
    layer: Layer,
    mesh: &mut ChunkMeshData,
) {
    mesh.vertices.clear();
    mesh.indices.clear();
    if position.x >= world.chunks_wide() || position.y >= world.chunks_high() {
        return;
    }
    let origin_x = position.x * CHUNK_SIZE as u32;
    let origin_y = position.y * CHUNK_SIZE as u32;
    let width = (world.width() - origin_x).min(CHUNK_SIZE as u32);
    let height = (world.height() - origin_y).min(CHUNK_SIZE as u32);
    mesh.vertices.reserve(width as usize * height as usize * 4);
    mesh.indices.reserve(width as usize * height as usize * 6);
    // Depth 0.0 is reserved for GUI in easy-gpu's shared render pass.
    let z = if layer == Layer::Foreground {
        0.05
    } else {
        0.5
    };
    for local_y in 0..height {
        let y = origin_y + local_y;
        for local_x in 0..width {
            let x = origin_x + local_x;
            let tile = world.tile_in_bounds(x, y, layer);
            if tile == TileId::EMPTY {
                continue;
            }
            let mask = neighbour_mask(world, x, y, layer);
            let variant = MARCHING_SQUARES_LUT[mask as usize];
            let atlas_rows = atlas_rows(layer);
            let material = tile_material_origin(tile, atlas_rows as u32);
            let u0 = (material[0] + variant[0] / VARIANTS_PER_AXIS) / ATLAS_COLUMNS;
            let v0 = (material[1] + variant[1] / VARIANTS_PER_AXIS) / atlas_rows;
            let u1 = u0 + 1.0 / (VARIANTS_PER_AXIS * ATLAS_COLUMNS);
            let v1 = v0 + 1.0 / (VARIANTS_PER_AXIS * atlas_rows);
            let world_x = x as f32;
            let world_y = -(y as f32);
            let base = mesh.vertices.len() as u16;
            mesh.vertices.extend_from_slice(&[
                TerrainVertex::new([world_x - 0.5, world_y + 0.5, z], [u0, v1]),
                TerrainVertex::new([world_x + 0.5, world_y + 0.5, z], [u1, v1]),
                TerrainVertex::new([world_x + 0.5, world_y - 0.5, z], [u1, v0]),
                TerrainVertex::new([world_x - 0.5, world_y - 0.5, z], [u0, v0]),
            ]);
            mesh.indices.extend_from_slice(&[
                base + 1,
                base + 3,
                base,
                base + 1,
                base + 2,
                base + 3,
            ]);
        }
    }
}

fn neighbour_mask(world: &World, x: u32, y: u32, layer: Layer) -> u8 {
    let mut mask = 0_u8;
    let mut bit = 0;
    for dx in -1..=1 {
        for dy in -1..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let solid = x
                .checked_add_signed(dx)
                .zip(y.checked_add_signed(dy))
                .filter(|&(sample_x, sample_y)| {
                    sample_x < world.width() && sample_y < world.height()
                })
                .is_none_or(|(sample_x, sample_y)| {
                    world.tile_in_bounds(sample_x, sample_y, layer) != TileId::EMPTY
                });
            mask |= u8::from(solid) << bit;
            bit += 1;
        }
    }
    mask
}

fn tile_material_origin(tile: TileId, atlas_rows: u32) -> [f32; 2] {
    let material_count = ATLAS_COLUMNS as u32 * atlas_rows;
    let index = tile.raw().saturating_sub(1) as u32 % material_count;
    [(index % 4) as f32, (index / 4) as f32]
}

fn atlas_rows(layer: Layer) -> f32 {
    match layer {
        Layer::Foreground => FOREGROUND_ATLAS_ROWS,
        Layer::Background => BACKGROUND_ATLAS_ROWS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ForegroundTile;

    #[test]
    fn appended_chunk_indices_are_rebased() {
        let mut world = World::empty(64, 64, 0).unwrap();
        world
            .set_tile(2, 2, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
        let source = build_chunk_mesh(&world, ChunkPos { x: 0, y: 0 }, Layer::Foreground);
        let mut combined = ChunkMeshData::default();
        append_chunk_mesh(&mut combined, &source);
        append_chunk_mesh(&mut combined, &source);
        assert_eq!(combined.vertices.len(), 8);
        assert_eq!(&combined.indices[6..], &[5, 7, 4, 5, 6, 7]);
    }

    #[test]
    fn rebuilding_into_existing_mesh_reuses_its_allocations() {
        let mut world = World::empty(32, 32, 0).unwrap();
        for y in 0..32 {
            for x in 0..32 {
                world
                    .set_tile(x, y, Layer::Foreground, ForegroundTile::STONE)
                    .unwrap();
            }
        }
        let mut mesh = ChunkMeshData::default();
        build_chunk_mesh_into(
            &world,
            ChunkPos { x: 0, y: 0 },
            Layer::Foreground,
            &mut mesh,
        );
        let vertex_allocation = mesh.vertices.as_ptr();
        let index_allocation = mesh.indices.as_ptr();

        build_chunk_mesh_into(
            &world,
            ChunkPos { x: 0, y: 0 },
            Layer::Foreground,
            &mut mesh,
        );

        assert_eq!(mesh.vertices.as_ptr(), vertex_allocation);
        assert_eq!(mesh.indices.as_ptr(), index_allocation);
    }
}
