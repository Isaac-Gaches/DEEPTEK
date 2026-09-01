use super::GuiRenderer;
use super::renderer::WORLD_MAP_TEXTURE_SIZE;
use crate::{
    BackgroundTile, ForegroundTile, FurnitureBehavior, ItemRegistry, Layer, TileId, TilePos, World,
    furniture_definition,
};

const MAP_SIZE: u32 = WORLD_MAP_TEXTURE_SIZE;
const MAP_ROWS_PER_FRAME: u32 = 16;
const SAMPLES_PER_PIXEL: u32 = 5;
const MAP_PIXEL_BYTES: usize = 4;
const MAP_PAN_SPEED: f32 = 0.7;
const MAP_ZOOM_STEP: f32 = 1.5;
const MAP_MIN_ZOOM: f32 = 4.0;
const MAP_INITIAL_ZOOM: f32 = 8.0;
const MAP_MAX_ZOOM: f32 = 32.0;
const MAX_VISIBLE_FURNITURE: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq)]
struct MapView {
    centre: [f32; 2],
    zoom: f32,
}

impl Default for MapView {
    fn default() -> Self {
        Self {
            centre: [0.5; 2],
            zoom: MAP_INITIAL_ZOOM,
        }
    }
}

impl MapView {
    fn bounds(self, world_size: [u32; 2], panel_size: [f32; 2]) -> ([f32; 2], [f32; 2]) {
        // Fit a world-space rectangle to the full panel without stretching tiles.
        let mut extent = [
            1.0,
            world_size[0].max(1) as f32 / world_size[1].max(1) as f32 * panel_size[1]
                / panel_size[0].max(1.0),
        ];
        if extent[1] > 1.0 {
            extent = [1.0 / extent[1], 1.0];
        }
        extent[0] /= self.zoom;
        extent[1] /= self.zoom;
        let minimum = std::array::from_fn(|axis| {
            (self.centre[axis] - extent[axis] * 0.5).clamp(0.0, 1.0 - extent[axis])
        });
        (minimum, [minimum[0] + extent[0], minimum[1] + extent[1]])
    }

    fn pan(&mut self, movement: [f32; 2], elapsed: f32) {
        let distance = MAP_PAN_SPEED * elapsed.clamp(0.0, 0.05) / self.zoom;
        self.centre[0] += movement[0] * distance;
        self.centre[1] += movement[1] * distance;
        self.clamp_centre();
    }

    fn zoom_by(&mut self, factor: f32) {
        self.zoom = (self.zoom * factor).clamp(MAP_MIN_ZOOM, MAP_MAX_ZOOM);
        self.clamp_centre();
    }

    fn clamp_centre(&mut self) {
        self.centre[0] = self.centre[0].clamp(0.0, 1.0);
        self.centre[1] = self.centre[1].clamp(0.0, 1.0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FurnitureMarker {
    world_position: [f32; 2],
    icon: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MapLayout {
    centre: [f32; 2],
    size: [f32; 2],
}

impl MapLayout {
    fn new(viewport: [f32; 2]) -> Self {
        let available_width = (viewport[0] - 80.0).max(1.0);
        let available_height = (viewport[1] - 126.0).max(1.0);
        Self {
            centre: [viewport[0] * 0.5, 62.0 + available_height * 0.5],
            size: [available_width, available_height],
        }
    }

    fn world_to_screen(self, position: [f32; 2], world_size: [u32; 2], view: MapView) -> [f32; 2] {
        let normalized = [
            position[0] / world_size[0].max(1) as f32,
            position[1] / world_size[1].max(1) as f32,
        ];
        self.normalized_to_screen(normalized, world_size, view)
    }

    fn normalized_to_screen(
        self,
        normalized: [f32; 2],
        world_size: [u32; 2],
        view: MapView,
    ) -> [f32; 2] {
        let (minimum, maximum) = view.bounds(world_size, self.size);
        [
            self.centre[0] - self.size[0] * 0.5
                + (normalized[0] - minimum[0]) / (maximum[0] - minimum[0]) * self.size[0],
            self.centre[1] - self.size[1] * 0.5
                + (normalized[1] - minimum[1]) / (maximum[1] - minimum[1]) * self.size[1],
        ]
    }
}

/// Bounded CPU overview cache. It is built over several normal frames and only
/// uploads its fixed-size texture when the map is actually shown.
pub struct WorldMapGui {
    pixels: Vec<u8>,
    world_size: [u32; 2],
    next_row: u32,
    foreground_revision: u64,
    survey_revision: u64,
    furniture_revision: u64,
    furniture: Vec<FurnitureMarker>,
    view: MapView,
    ready: bool,
    pending_upload: bool,
}

impl Default for WorldMapGui {
    fn default() -> Self {
        Self {
            pixels: vec![0; (MAP_SIZE * MAP_SIZE) as usize * MAP_PIXEL_BYTES],
            world_size: [0; 2],
            next_row: 0,
            foreground_revision: 0,
            survey_revision: 0,
            furniture_revision: u64::MAX,
            furniture: Vec::new(),
            view: MapView::default(),
            ready: false,
            pending_upload: false,
        }
    }
}

impl WorldMapGui {
    pub const fn is_ready(&self) -> bool {
        self.ready
    }

    pub fn reset(&mut self, world: &World) {
        self.world_size = [world.width(), world.height()];
        self.next_row = 0;
        self.foreground_revision = world.foreground_revision();
        self.survey_revision = world.survey_revision();
        self.furniture_revision = u64::MAX;
        self.furniture.clear();
        self.view = MapView::default();
        self.ready = false;
        self.pending_upload = false;
        self.pixels.fill(0);
    }

    /// Advances a small, fixed amount of map work. Once built, this only checks
    /// the world's bounded foreground change journal and resamples dirty pixels.
    pub fn advance(&mut self, world: &World) {
        if self.world_size != [world.width(), world.height()] {
            self.reset(world);
        }
        if self.survey_revision != world.survey_revision() {
            self.next_row = 0;
            self.foreground_revision = world.foreground_revision();
            self.survey_revision = world.survey_revision();
            self.ready = false;
            self.pending_upload = false;
            self.pixels.fill(0);
        }
        if !self.ready {
            let end_row = (self.next_row + MAP_ROWS_PER_FRAME).min(MAP_SIZE);
            for pixel_y in self.next_row..end_row {
                for pixel_x in 0..MAP_SIZE {
                    self.resample_pixel(world, pixel_x, pixel_y);
                }
            }
            self.next_row = end_row;
            if self.next_row == MAP_SIZE {
                self.ready = true;
                self.pending_upload = true;
                self.synchronize_changes(world);
            }
            return;
        }
        self.synchronize_changes(world);
    }

    pub fn navigate(&mut self, movement: [f32; 2], elapsed: f32) {
        self.view.pan(movement, elapsed);
    }

    pub fn focus_on_player(&mut self, player_position: [f32; 2]) {
        self.view.centre = [
            player_position[0] / self.world_size[0].max(1) as f32,
            player_position[1] / self.world_size[1].max(1) as f32,
        ];
        self.view.zoom = MAP_INITIAL_ZOOM;
        self.view.clamp_centre();
    }

    pub fn zoom_in(&mut self) {
        self.view.zoom_by(MAP_ZOOM_STEP);
    }

    pub fn zoom_out(&mut self) {
        self.view.zoom_by(1.0 / MAP_ZOOM_STEP);
    }

    pub fn invalidate_furniture(&mut self) {
        self.furniture_revision = u64::MAX;
    }

    pub fn upload_if_needed(
        &mut self,
        gpu: &easy_gpu::Renderer,
        renderer: &GuiRenderer,
        viewport: [f32; 2],
    ) {
        if self.ready && self.pending_upload {
            renderer.update_world_map_texture(gpu, &self.pixels);
            self.pending_upload = false;
        }
        let layout = MapLayout::new(viewport);
        let (minimum, maximum) = self.view.bounds(self.world_size, layout.size);
        renderer.update_world_map_view(gpu, minimum, maximum);
    }

    pub fn queue(
        &mut self,
        renderer: &mut GuiRenderer,
        world: &World,
        registry: &ItemRegistry,
        player_position: [f32; 2],
        camera: ([f32; 2], f32),
        viewport: [f32; 2],
    ) {
        let (camera_position, camera_vertical_tiles) = camera;
        renderer.queue_rect(
            [viewport[0] * 0.5, viewport[1] * 0.5],
            viewport,
            [0.008, 0.015, 0.028, 0.985],
        );
        queue_centred_text(
            renderer,
            "WORLD MAP",
            [viewport[0] * 0.5, 20.0],
            2.0,
            [0.38, 0.9, 1.0, 1.0],
        );

        if !self.ready {
            let progress = self.progress();
            let label = format!("SCANNING TERRAIN {:>3}%", (progress * 100.0).round() as u32);
            queue_centred_text(
                renderer,
                &label,
                [viewport[0] * 0.5, viewport[1] * 0.5 - 18.0],
                1.5,
                [0.72, 0.86, 0.94, 1.0],
            );
            let width = (viewport[0] - 80.0).clamp(120.0, 420.0);
            renderer.queue_rect(
                [viewport[0] * 0.5, viewport[1] * 0.5 + 16.0],
                [width, 12.0],
                [0.04, 0.08, 0.12, 1.0],
            );
            renderer.queue_rect(
                [
                    viewport[0] * 0.5 - width * 0.5 + width * progress * 0.5,
                    viewport[1] * 0.5 + 16.0,
                ],
                [width * progress, 12.0],
                [0.05, 0.62, 0.78, 1.0],
            );
            return;
        }

        self.synchronize_furniture(world, registry);

        let world_size = [world.width(), world.height()];
        let layout = MapLayout::new(viewport);
        renderer.queue_rect(
            layout.centre,
            [layout.size[0] + 8.0, layout.size[1] + 8.0],
            [0.12, 0.26, 0.34, 1.0],
        );
        renderer.queue_world_map(layout.centre, layout.size);

        let (view_minimum, view_maximum) = self.view.bounds(world_size, layout.size);
        let icon_size = (5.0 + self.view.zoom.sqrt() * 1.4).clamp(6.0, 13.0);
        for marker in self
            .furniture
            .iter()
            .filter(|marker| {
                let normalized = [
                    marker.world_position[0] / world.width().max(1) as f32,
                    marker.world_position[1] / world.height().max(1) as f32,
                ];
                (view_minimum[0]..=view_maximum[0]).contains(&normalized[0])
                    && (view_minimum[1]..=view_maximum[1]).contains(&normalized[1])
                    && world.is_surveyed(TilePos::new(
                        marker.world_position[0].max(0.0) as u32,
                        marker.world_position[1].max(0.0) as u32,
                    ))
            })
            .take(MAX_VISIBLE_FURNITURE)
        {
            renderer.queue_icon(
                marker.icon,
                layout.world_to_screen(marker.world_position, world_size, self.view),
                icon_size,
                [1.0; 4],
            );
        }

        let camera_normalized = [
            camera_position[0] / world.width().max(1) as f32,
            camera_position[1] / world.height().max(1) as f32,
        ];
        if (view_minimum[0]..=view_maximum[0]).contains(&camera_normalized[0])
            && (view_minimum[1]..=view_maximum[1]).contains(&camera_normalized[1])
        {
            let camera_centre = layout.world_to_screen(camera_position, world_size, self.view);
            let camera_height = (camera_vertical_tiles
                / world.height().max(1) as f32
                / (view_maximum[1] - view_minimum[1])
                * layout.size[1])
                .max(4.0);
            let camera_width = (camera_vertical_tiles * viewport[0]
                / viewport[1].max(1.0)
                / world.width().max(1) as f32
                / (view_maximum[0] - view_minimum[0])
                * layout.size[0])
                .max(4.0);
            queue_outline(
                renderer,
                camera_centre,
                [camera_width, camera_height],
                [1.0, 0.9, 0.32, 0.85],
            );
        }

        let player_normalized = [
            player_position[0] / world.width().max(1) as f32,
            player_position[1] / world.height().max(1) as f32,
        ];
        if (view_minimum[0]..=view_maximum[0]).contains(&player_normalized[0])
            && (view_minimum[1]..=view_maximum[1]).contains(&player_normalized[1])
        {
            let player = layout.world_to_screen(player_position, world_size, self.view);
            renderer.queue_world_map_overlay(player, [11.0, 3.0], [0.2, 1.0, 0.88, 1.0]);
            renderer.queue_world_map_overlay(player, [3.0, 11.0], [0.2, 1.0, 0.88, 1.0]);
        }

        let position_label = format!(
            "X {}   DEPTH {:+.1}M",
            player_position[0].round() as i32,
            world.elevation_decimetres(player_position[1]) as f32 / 10.0
        );
        queue_centred_text(
            renderer,
            &position_label,
            [viewport[0] * 0.5, viewport[1] - 42.0],
            1.5,
            [0.78, 0.9, 0.96, 1.0],
        );
        queue_centred_text(
            renderer,
            &format!(
                "WASD PAN   -/= ZOOM {:.1}X   TAB OR ESC CLOSE",
                self.view.zoom
            ),
            [viewport[0] * 0.5, viewport[1] - 22.0],
            1.0,
            [0.48, 0.64, 0.72, 1.0],
        );
    }

    fn progress(&self) -> f32 {
        self.next_row as f32 / MAP_SIZE as f32
    }

    fn synchronize_furniture(&mut self, world: &World, registry: &ItemRegistry) {
        let revision = world.object_revision();
        if revision == self.furniture_revision {
            return;
        }
        self.furniture.clear();
        self.furniture.extend(world.objects().filter_map(|object| {
            let definition = furniture_definition(object.object_type())?;
            let item = registry.item_for_furniture(object.object_type())?;
            let icon = registry.get(item)?.icon;
            let size = object.size();
            let x = object.anchor().x as f32 + f32::from(size[0]) * 0.5;
            let y = if definition.behavior() == FurnitureBehavior::CargoLift {
                object.motion_position_tiles() + f32::from(size[1]) * 0.5
            } else {
                object.anchor().y as f32 + f32::from(size[1]) * 0.5
            };
            Some(FurnitureMarker {
                world_position: [x, y],
                icon,
            })
        }));
        self.furniture_revision = revision;
    }

    fn synchronize_changes(&mut self, world: &World) {
        let current_revision = world.foreground_revision();
        if current_revision == self.foreground_revision {
            return;
        }
        let Some(changes) = world.foreground_changes_since(self.foreground_revision) else {
            self.reset(world);
            return;
        };
        let changes: Vec<_> = changes.collect();
        self.foreground_revision = current_revision;

        let mut dirty_pixels = Vec::with_capacity(changes.len());
        for position in changes {
            let x_range = map_pixel_range(position.x, world.width());
            let y_range = map_pixel_range(position.y, world.height());
            for pixel_y in y_range.clone() {
                for pixel_x in x_range.clone() {
                    dirty_pixels.push((pixel_y * MAP_SIZE + pixel_x) as usize);
                }
            }
        }
        dirty_pixels.sort_unstable();
        dirty_pixels.dedup();
        if dirty_pixels.len() > (MAP_SIZE * MAP_SIZE / 2) as usize {
            self.reset(world);
            return;
        }
        for index in dirty_pixels {
            self.resample_pixel(world, index as u32 % MAP_SIZE, index as u32 / MAP_SIZE);
        }
        self.pending_upload = true;
    }

    fn resample_pixel(&mut self, world: &World, pixel_x: u32, pixel_y: u32) {
        let x0 = sample_edge(pixel_x, world.width()).min(world.width() - 1);
        let x1 = sample_edge(pixel_x + 1, world.width())
            .saturating_sub(1)
            .max(x0);
        let y0 = sample_edge(pixel_y, world.height()).min(world.height() - 1);
        let y1 = sample_edge(pixel_y + 1, world.height())
            .saturating_sub(1)
            .max(y0);
        let pixel = ((pixel_y * MAP_SIZE + pixel_x) * 4) as usize;
        if !world.surveyed_area_intersects(
            TilePos::new(x0, y0),
            TilePos::new(x1.saturating_add(1), y1.saturating_add(1)),
        ) {
            self.pixels[pixel..pixel + 4].copy_from_slice(&[0, 0, 0, 255]);
            return;
        }
        let centre = [(x0 + x1) / 2, (y0 + y1) / 2];
        let samples = [[x0, y0], [x1, y0], [x0, y1], [x1, y1], centre];
        let mut colour = [0_u32; 3];
        for [x, y] in samples {
            let sample = tile_colour(world, x, y);
            for channel in 0..3 {
                colour[channel] += u32::from(sample[channel]);
            }
        }
        self.pixels[pixel..pixel + 4].copy_from_slice(&[
            (colour[0] / SAMPLES_PER_PIXEL) as u8,
            (colour[1] / SAMPLES_PER_PIXEL) as u8,
            (colour[2] / SAMPLES_PER_PIXEL) as u8,
            255,
        ]);
    }
}

fn sample_edge(pixel_edge: u32, world_extent: u32) -> u32 {
    ((u64::from(pixel_edge) * u64::from(world_extent) / u64::from(MAP_SIZE)) as u32)
        .min(world_extent)
}

fn map_pixel_range(tile: u32, world_extent: u32) -> std::ops::Range<u32> {
    let extent = u64::from(world_extent.max(1));
    let start = (u64::from(tile) * u64::from(MAP_SIZE) / extent) as u32;
    let end = ((u64::from(tile + 1) * u64::from(MAP_SIZE)).div_ceil(extent) as u32)
        .max(start + 1)
        .min(MAP_SIZE);
    start.min(MAP_SIZE - 1)..end
}

fn tile_colour(world: &World, x: u32, y: u32) -> [u8; 3] {
    let foreground = world.tile_in_bounds(x, y, Layer::Foreground);
    match foreground {
        ForegroundTile::GRASS => [55, 137, 73],
        ForegroundTile::DIRT => [126, 82, 48],
        ForegroundTile::STONE => [89, 99, 113],
        ForegroundTile::IRON_ORE => [142, 75, 39],
        ForegroundTile::ASTERITE => [48, 164, 225],
        tile if tile == TileId::new(4) => [235, 66, 56],
        tile if tile == TileId::new(6) => [44, 145, 214],
        TileId::EMPTY => match world.tile_in_bounds(x, y, Layer::Background) {
            BackgroundTile::DIRT_WALL => [61, 42, 34],
            BackgroundTile::STONE_WALL => [48, 52, 61],
            TileId::EMPTY if y < world.sea_level_y() => {
                let daylight =
                    18_u32.saturating_sub(y.saturating_mul(12) / world.sea_level_y().max(1));
                [5, (10 + daylight) as u8, (20 + daylight) as u8]
            }
            TileId::EMPTY => [7, 10, 16],
            _ => [52, 44, 58],
        },
        _ => [112, 94, 124],
    }
}

fn queue_outline(renderer: &mut GuiRenderer, centre: [f32; 2], size: [f32; 2], tint: [f32; 4]) {
    let thickness = 1.5;
    renderer.queue_world_map_overlay(
        [centre[0], centre[1] - size[1] * 0.5],
        [size[0], thickness],
        tint,
    );
    renderer.queue_world_map_overlay(
        [centre[0], centre[1] + size[1] * 0.5],
        [size[0], thickness],
        tint,
    );
    renderer.queue_world_map_overlay(
        [centre[0] - size[0] * 0.5, centre[1]],
        [thickness, size[1]],
        tint,
    );
    renderer.queue_world_map_overlay(
        [centre[0] + size[0] * 0.5, centre[1]],
        [thickness, size[1]],
        tint,
    );
}

fn queue_centred_text(
    renderer: &mut GuiRenderer,
    text: &str,
    centre: [f32; 2],
    scale: f32,
    tint: [f32; 4],
) {
    renderer.queue_text(
        text,
        [
            centre[0] - GuiRenderer::text_width(text, scale) * 0.5,
            centre[1],
        ],
        scale,
        tint,
    );
}

#[cfg(test)]
mod tests;
