use deep_tek::FollowCamera;
use std::collections::HashSet;
use std::time::Instant;
use winit::keyboard::KeyCode;

pub(crate) const fn is_jump_key(key: KeyCode) -> bool {
    matches!(key, KeyCode::Space | KeyCode::ArrowUp)
}

#[derive(Clone, Copy)]
pub(super) struct PointerClick {
    pub(super) pixel: [f32; 2],
    pub(super) world: [f32; 2],
}

pub(crate) struct InputState {
    pub(super) pressed: HashSet<KeyCode>,
    pub(super) jump_queued: bool,
    pub(super) cursor_position: [f32; 2],
    pub(super) primary_click_queued: Option<PointerClick>,
    pub(super) secondary_click_queued: Option<PointerClick>,
    pub(super) primary_down: bool,
    pub(super) primary_world_use_active: bool,
    pub(super) last_continuous_item_use: Instant,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            pressed: HashSet::new(),
            jump_queued: false,
            cursor_position: [0.0, 0.0],
            primary_click_queued: None,
            secondary_click_queued: None,
            primary_down: false,
            primary_world_use_active: false,
            last_continuous_item_use: Instant::now(),
        }
    }
}

impl InputState {
    pub(crate) fn horizontal_movement(&self) -> f32 {
        let left =
            self.pressed.contains(&KeyCode::KeyA) || self.pressed.contains(&KeyCode::ArrowLeft);
        let right =
            self.pressed.contains(&KeyCode::KeyD) || self.pressed.contains(&KeyCode::ArrowRight);
        (i32::from(right) - i32::from(left)) as f32
    }

    pub(crate) fn vertical_movement(&self) -> f32 {
        let up = self.pressed.contains(&KeyCode::KeyW) || self.pressed.contains(&KeyCode::ArrowUp);
        let down =
            self.pressed.contains(&KeyCode::KeyS) || self.pressed.contains(&KeyCode::ArrowDown);
        (i32::from(down) - i32::from(up)) as f32
    }

    pub(crate) fn take_jump(&mut self) -> bool {
        std::mem::take(&mut self.jump_queued)
    }

    pub(crate) fn press_key(&mut self, key: KeyCode) -> bool {
        self.pressed.insert(key)
    }

    pub(crate) fn release_key(&mut self, key: KeyCode) {
        self.pressed.remove(&key);
    }

    pub(crate) fn queue_jump(&mut self) {
        self.jump_queued = true;
    }

    pub(crate) fn move_cursor(&mut self, position: [f32; 2]) {
        self.cursor_position = position;
    }

    pub(crate) fn cursor_position(&self) -> [f32; 2] {
        self.cursor_position
    }

    pub(crate) fn press_primary(&mut self, camera: &FollowCamera, viewport: [f32; 2]) {
        self.primary_down = true;
        self.primary_click_queued = Some(PointerClick {
            pixel: self.cursor_position,
            world: camera.screen_to_world(self.cursor_position, viewport),
        });
    }

    pub(crate) fn release_primary(&mut self) {
        self.primary_down = false;
        self.primary_world_use_active = false;
    }

    pub(crate) fn queue_secondary(&mut self, camera: &FollowCamera, viewport: [f32; 2]) {
        self.secondary_click_queued = Some(PointerClick {
            pixel: self.cursor_position,
            world: camera.screen_to_world(self.cursor_position, viewport),
        });
    }

    pub(crate) fn clear_focus(&mut self) {
        self.pressed.clear();
        self.jump_queued = false;
        self.primary_click_queued = None;
        self.secondary_click_queued = None;
        self.primary_down = false;
        self.primary_world_use_active = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w_is_reserved_for_vertical_climbing() {
        assert!(!is_jump_key(KeyCode::KeyW));
        assert!(is_jump_key(KeyCode::Space));
        assert!(is_jump_key(KeyCode::ArrowUp));
    }
}
