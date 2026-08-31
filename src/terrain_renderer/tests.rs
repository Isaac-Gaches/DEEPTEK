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
