#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FollowCamera {
    position: [f32; 2],
    vertical_tiles_visible: f32,
    follow_rate: f32,
}

const MIN_VERTICAL_TILES_VISIBLE: f32 = 12.0;
const MAX_VERTICAL_TILES_VISIBLE: f32 = 120.0;
const ZOOM_STEP: f32 = 1.15;

impl FollowCamera {
    pub fn new(position: [f32; 2], vertical_tiles_visible: f32) -> Self {
        Self {
            position,
            vertical_tiles_visible: vertical_tiles_visible.max(2.0),
            follow_rate: 8.0,
        }
    }

    pub const fn position(&self) -> [f32; 2] {
        self.position
    }

    pub const fn vertical_tiles_visible(&self) -> f32 {
        self.vertical_tiles_visible
    }

    pub fn set_vertical_tiles_visible(&mut self, tiles: f32) {
        self.vertical_tiles_visible = tiles.max(2.0);
    }

    pub fn zoom_in(&mut self) {
        self.vertical_tiles_visible =
            (self.vertical_tiles_visible / ZOOM_STEP).max(MIN_VERTICAL_TILES_VISIBLE);
    }

    pub fn zoom_out(&mut self) {
        self.vertical_tiles_visible =
            (self.vertical_tiles_visible * ZOOM_STEP).min(MAX_VERTICAL_TILES_VISIBLE);
    }

    pub fn set_follow_rate(&mut self, follow_rate: f32) {
        self.follow_rate = follow_rate.max(0.0);
    }

    pub fn snap_to(&mut self, target: [f32; 2]) {
        self.position = target;
    }

    pub fn follow(&mut self, target: [f32; 2], elapsed: f32) {
        let blend = 1.0 - (-self.follow_rate * elapsed.clamp(0.0, 0.1)).exp();
        self.position[0] += (target[0] - self.position[0]) * blend;
        self.position[1] += (target[1] - self.position[1]) * blend;
    }

    pub fn screen_to_world(&self, pixel: [f32; 2], viewport: [f32; 2]) -> [f32; 2] {
        let width = viewport[0].max(1.0);
        let height = viewport[1].max(1.0);
        let half_height = self.vertical_tiles_visible * 0.5;
        let half_width = half_height * (width / height);
        let normalized_x = pixel[0] / width * 2.0 - 1.0;
        let normalized_y = pixel[1] / height * 2.0 - 1.0;
        [
            self.position[0] + normalized_x * half_width,
            self.position[1] + normalized_y * half_height,
        ]
    }

    pub fn world_to_screen(&self, position: [f32; 2], viewport: [f32; 2]) -> [f32; 2] {
        let width = viewport[0].max(1.0);
        let height = viewport[1].max(1.0);
        let half_height = self.vertical_tiles_visible * 0.5;
        let half_width = half_height * (width / height);
        [
            ((position[0] - self.position[0]) / half_width + 1.0) * width * 0.5,
            ((position[1] - self.position[1]) / half_height + 1.0) * height * 0.5,
        ]
    }
}

impl Default for FollowCamera {
    fn default() -> Self {
        Self::new([0.0, 0.0], 55.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_moves_towards_target_without_overshooting() {
        let mut camera = FollowCamera::default();
        camera.follow([10.0, 20.0], 0.1);
        assert!((0.0..10.0).contains(&camera.position()[0]));
        assert!((0.0..20.0).contains(&camera.position()[1]));
    }

    #[test]
    fn screen_centre_maps_to_camera_position() {
        let camera = FollowCamera::new([12.0, 34.0], 50.0);
        assert_eq!(
            camera.screen_to_world([400.0, 300.0], [800.0, 600.0]),
            camera.position()
        );
    }

    #[test]
    fn screen_edges_match_the_rendered_camera_bounds() {
        let camera = FollowCamera::new([100.0, 50.0], 40.0);
        let viewport = [800.0, 400.0];
        assert_eq!(camera.screen_to_world([0.0, 0.0], viewport), [60.0, 30.0]);
        assert_eq!(
            camera.screen_to_world([800.0, 400.0], viewport),
            [140.0, 70.0]
        );
    }

    #[test]
    fn world_and_screen_conversions_are_inverses() {
        let camera = FollowCamera::new([100.0, 50.0], 40.0);
        let viewport = [1280.0, 720.0];
        let world = [83.25, 61.75];
        let screen = camera.world_to_screen(world, viewport);
        let restored = camera.screen_to_world(screen, viewport);
        assert!((restored[0] - world[0]).abs() < 0.0001);
        assert!((restored[1] - world[1]).abs() < 0.0001);
    }

    #[test]
    fn zoom_uses_bounded_multiplicative_steps() {
        let mut camera = FollowCamera::default();
        let initial = camera.vertical_tiles_visible();
        camera.zoom_in();
        assert!(camera.vertical_tiles_visible() < initial);
        camera.zoom_out();
        assert!((camera.vertical_tiles_visible() - initial).abs() < 0.0001);

        for _ in 0..100 {
            camera.zoom_in();
        }
        assert_eq!(camera.vertical_tiles_visible(), MIN_VERTICAL_TILES_VISIBLE);
        for _ in 0..100 {
            camera.zoom_out();
        }
        assert_eq!(camera.vertical_tiles_visible(), MAX_VERTICAL_TILES_VISIBLE);
    }
}
