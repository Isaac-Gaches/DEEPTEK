use super::*;
use crate::{ForegroundTile, FurnitureObject, TilePos};

#[test]
fn map_layout_fills_the_available_viewport() {
    let viewport = [800.0, 600.0];
    let layout = MapLayout::new(viewport);
    assert_eq!(layout.size, [720.0, 474.0]);
    assert!(layout.centre[0] - layout.size[0] * 0.5 >= 0.0);
    assert!(layout.centre[1] - layout.size[1] * 0.5 >= 0.0);
    assert!(layout.centre[1] + layout.size[1] * 0.5 <= viewport[1]);
}

#[test]
fn zoomed_map_fills_the_available_viewport_without_stretching_world_units() {
    let viewport = [800.0, 600.0];
    let world_size = [2_000, 8_000];
    let layout = MapLayout::new(viewport);
    let mut view = MapView::default();
    view.zoom_by(MAP_ZOOM_STEP);
    let (minimum, maximum) = view.bounds(world_size, layout.size);
    let visible_world_aspect = (maximum[0] - minimum[0]) * world_size[0] as f32
        / ((maximum[1] - minimum[1]) * world_size[1] as f32);

    assert_eq!(layout.size, [720.0, 474.0]);
    assert!((visible_world_aspect - layout.size[0] / layout.size[1]).abs() < 0.0001);
}

#[test]
fn map_view_zoom_and_pan_remain_inside_the_world() {
    let mut view = MapView::default();
    view.zoom_by(2.0);
    view.pan([1.0, -1.0], 1.0);
    let (minimum, maximum) = view.bounds([2_000, 8_000], [720.0, 474.0]);
    assert!(minimum.into_iter().all(|value| value >= 0.0));
    assert!(maximum.into_iter().all(|value| value <= 1.0));

    for _ in 0..100 {
        view.zoom_by(MAP_ZOOM_STEP);
    }
    assert_eq!(view.zoom, MAP_MAX_ZOOM);

    for _ in 0..100 {
        view.zoom_by(1.0 / MAP_ZOOM_STEP);
    }
    assert_eq!(view.zoom, MAP_MIN_ZOOM);
}

#[test]
fn map_layout_projects_the_view_centre_to_the_panel_centre() {
    let layout = MapLayout::new([800.0, 600.0]);
    let mut view = MapView::default();
    view.zoom_by(4.0);
    view.centre = [0.25, 0.75];
    let projected = layout.normalized_to_screen(view.centre, [2_000, 8_000], view);
    assert!((projected[0] - layout.centre[0]).abs() < 0.01);
    assert!((projected[1] - layout.centre[1]).abs() < 0.01);
}

#[test]
fn opening_focus_centres_the_map_on_the_player_at_a_local_zoom() {
    let mut map = WorldMapGui {
        world_size: [2_000, 8_000],
        ..WorldMapGui::default()
    };
    map.focus_on_player([500.0, 6_000.0]);
    assert_eq!(map.view.centre, [0.25, 0.75]);
    assert_eq!(map.view.zoom, MAP_INITIAL_ZOOM);
}

#[test]
fn progressive_build_has_a_small_fixed_row_budget() {
    let world = World::empty(64, 64, 0).unwrap();
    let mut map = WorldMapGui::default();
    map.reset(&world);
    map.advance(&world);
    assert_eq!(map.next_row, MAP_ROWS_PER_FRAME);
    assert!(!map.ready);
}

#[test]
fn map_pixel_range_covers_small_and_large_worlds() {
    assert_eq!(map_pixel_range(0, 1), 0..MAP_SIZE);
    assert_eq!(map_pixel_range(1_000, 2_000), 256..257);
}

#[test]
fn completed_map_resamples_from_the_foreground_change_journal() {
    let mut world = World::empty(64, 64, 0).unwrap();
    let mut map = WorldMapGui::default();
    map.reset(&world);
    map.ready = true;
    map.next_row = MAP_SIZE;
    world
        .set_tile(10, 10, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();

    map.advance(&world);

    assert!(map.pending_upload);
    assert_eq!(map.foreground_revision, world.foreground_revision());
}

#[test]
fn terrain_palette_distinguishes_air_and_built_in_blocks() {
    let mut world = World::empty(4, 4, 0).unwrap();
    let air = tile_colour(&world, 1, 1);
    world
        .set_tile(1, 1, Layer::Foreground, ForegroundTile::GRASS)
        .unwrap();
    let grass = tile_colour(&world, 1, 1);
    world
        .set_tile(1, 1, Layer::Foreground, ForegroundTile::STONE)
        .unwrap();
    let stone = tile_colour(&world, 1, 1);
    world
        .set_tile(1, 1, Layer::Foreground, ForegroundTile::IRON_ORE)
        .unwrap();
    let iron = tile_colour(&world, 1, 1);
    assert_ne!(air, grass);
    assert_ne!(grass, stone);
    assert_ne!(stone, iron);
}

#[test]
fn map_pixels_stay_black_until_a_survey_reveals_them() {
    let mut world = World::empty(80, 80, 0).unwrap();
    world
        .set_tile(38, 30, Layer::Foreground, ForegroundTile::IRON_ORE)
        .unwrap();
    let mut map = WorldMapGui::default();
    map.reset(&world);
    let pixel_x = 38 * MAP_SIZE / world.width();
    let pixel_y = 30 * MAP_SIZE / world.height();
    map.resample_pixel(&world, pixel_x, pixel_y);
    let offset = ((pixel_y * MAP_SIZE + pixel_x) * 4) as usize;
    assert_eq!(&map.pixels[offset..offset + 4], &[0, 0, 0, 255]);

    for x in 0..80 {
        world
            .set_tile(x, 20, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    let surveyor = world
        .place_furniture(FurnitureObject::SUBSURFACE_SURVEYOR, TilePos::new(38, 18))
        .unwrap();
    world.subsurface_survey(surveyor).unwrap();
    map.resample_pixel(&world, pixel_x, pixel_y);
    assert_ne!(&map.pixels[offset..offset + 4], &[0, 0, 0, 255]);
}

#[test]
fn furniture_cache_contains_placed_furniture_item_icons() {
    let registry = ItemRegistry::with_built_ins();
    let mut world = World::empty(8, 8, 0).unwrap();
    for x in 2..=3 {
        world
            .set_tile(x, 5, Layer::Foreground, ForegroundTile::STONE)
            .unwrap();
    }
    world
        .place_furniture(FurnitureObject::CHEST, TilePos::new(2, 3))
        .unwrap();
    let mut map = WorldMapGui::default();
    map.synchronize_furniture(&world, &registry);

    assert_eq!(map.furniture.len(), 1);
    assert_eq!(
        map.furniture[0].icon,
        registry
            .get(registry.item_for_furniture(FurnitureObject::CHEST).unwrap())
            .unwrap()
            .icon
    );
}
