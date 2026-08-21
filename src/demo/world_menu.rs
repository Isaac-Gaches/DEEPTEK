mod creation;
mod layout;
mod storage;
mod view;

pub(crate) use creation::WorldCreationRequest;

use creation::CreationForm;
use deep_tek::GuiRenderer;
use layout::{ConfirmationLayout, CreationLayout, MenuLayout, Rect};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use storage::{WorldEntry, discover_worlds};
use view::{
    queue_button, queue_centred_text, queue_confirmation, queue_creation, queue_title,
    truncate_label,
};
use winit::keyboard::KeyCode;

const DEFAULT_WORLD_DIRECTORY: &str = "worlds";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WorldMenuAction {
    Create(WorldCreationRequest),
    Load(PathBuf),
    Delete(PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum MenuMode {
    List,
    Create(CreationForm),
    ConfirmDelete { label: String, path: PathBuf },
}

pub(crate) struct WorldMenu {
    directory: PathBuf,
    entries: Vec<WorldEntry>,
    scroll: usize,
    status: Option<(String, bool)>,
    mode: MenuMode,
}

impl Default for WorldMenu {
    fn default() -> Self {
        Self::new(DEFAULT_WORLD_DIRECTORY)
    }
}

impl WorldMenu {
    fn new(directory: impl Into<PathBuf>) -> Self {
        let mut menu = Self {
            directory: directory.into(),
            entries: Vec::new(),
            scroll: 0,
            status: None,
            mode: MenuMode::List,
        };
        if let Err(error) = menu.refresh() {
            eprintln!("failed to scan saved worlds: {error}");
            menu.set_status("WORLD LIST ERROR", true);
        }
        menu
    }

    pub(crate) fn refresh(&mut self) -> io::Result<()> {
        self.entries = discover_worlds(&self.directory)?;
        self.scroll = self.scroll.min(self.entries.len().saturating_sub(1));
        Ok(())
    }

    pub(crate) fn ensure_directory(&self) -> io::Result<()> {
        fs::create_dir_all(&self.directory)
    }

    pub(crate) fn next_world_path(&self) -> PathBuf {
        for index in 1_u32.. {
            let candidate = self.directory.join(format!("world_{index}.world"));
            if !candidate.exists() {
                return candidate;
            }
        }
        unreachable!("the world number space cannot be exhausted in practice")
    }

    pub(crate) fn delete_world(&mut self, path: &Path) -> io::Result<()> {
        if !self.entries.iter().any(|entry| entry.path == path) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "world is no longer in the save list",
            ));
        }
        fs::remove_file(path)?;
        self.mode = MenuMode::List;
        self.refresh()
    }

    pub(crate) fn set_status(&mut self, message: impl Into<String>, is_error: bool) {
        self.status = Some((message.into(), is_error));
    }

    pub(crate) fn clear_status(&mut self) {
        self.status = None;
    }

    pub(crate) fn show_root(&mut self) {
        self.mode = MenuMode::List;
    }

    pub(crate) fn is_root(&self) -> bool {
        matches!(self.mode, MenuMode::List)
    }

    pub(crate) fn handle_key(
        &mut self,
        key: KeyCode,
        text: Option<&str>,
    ) -> Option<WorldMenuAction> {
        let path = self.next_world_path();
        let MenuMode::Create(form) = &mut self.mode else {
            if key == KeyCode::Escape && matches!(self.mode, MenuMode::ConfirmDelete { .. }) {
                self.mode = MenuMode::List;
            }
            return None;
        };
        match key {
            KeyCode::Escape => self.mode = MenuMode::List,
            KeyCode::Tab | KeyCode::ArrowDown => form.move_selection(1),
            KeyCode::ArrowUp => form.move_selection(-1),
            KeyCode::ArrowLeft => form.change_size(-1),
            KeyCode::ArrowRight => form.change_size(1),
            KeyCode::Backspace => form.backspace(),
            KeyCode::Delete => form.clear_active(),
            KeyCode::Enter | KeyCode::NumpadEnter => {
                return form.request(path).map(WorldMenuAction::Create);
            }
            _ => {
                if let Some(text) = text {
                    form.append_text(text);
                }
            }
        }
        None
    }

    pub(crate) fn handle_click(
        &mut self,
        cursor: [f32; 2],
        viewport: [f32; 2],
    ) -> Option<WorldMenuAction> {
        let layout = MenuLayout::new(viewport);
        let next_world_path = self.next_world_path();
        match &mut self.mode {
            MenuMode::List => {
                if layout.create_button.contains(cursor) {
                    self.status = None;
                    self.mode = MenuMode::Create(CreationForm::new());
                    return None;
                }
                let clicked = self
                    .visible_entries(layout)
                    .find_map(|(_, entry, load, delete)| {
                        if load.contains(cursor) {
                            Some((false, entry.label.clone(), entry.path.clone()))
                        } else if delete.contains(cursor) {
                            Some((true, entry.label.clone(), entry.path.clone()))
                        } else {
                            None
                        }
                    });
                match clicked {
                    Some((false, _, path)) => Some(WorldMenuAction::Load(path)),
                    Some((true, label, path)) => {
                        self.mode = MenuMode::ConfirmDelete { label, path };
                        None
                    }
                    None => None,
                }
            }
            MenuMode::Create(form) => {
                let creation = CreationLayout::new(layout.panel);
                if let Some(field) = creation
                    .fields
                    .iter()
                    .position(|field| field.contains(cursor))
                {
                    form.select(field);
                    return None;
                }
                if let Some(index) = creation
                    .size_buttons
                    .iter()
                    .position(|button| button.contains(cursor))
                {
                    form.select_size(creation::WorldSize::ALL[index]);
                    return None;
                }
                if creation.back.contains(cursor) {
                    self.mode = MenuMode::List;
                    return None;
                }
                if creation.create.contains(cursor) {
                    return form.request(next_world_path).map(WorldMenuAction::Create);
                }
                None
            }
            MenuMode::ConfirmDelete { path, .. } => {
                let confirmation = ConfirmationLayout::new(layout.panel);
                if confirmation.cancel.contains(cursor) {
                    self.mode = MenuMode::List;
                    None
                } else if confirmation.delete.contains(cursor) {
                    Some(WorldMenuAction::Delete(path.clone()))
                } else {
                    None
                }
            }
        }
    }

    pub(crate) fn scroll(&mut self, direction: f32, viewport: [f32; 2]) {
        if !matches!(self.mode, MenuMode::List) {
            return;
        }
        let visible_rows = MenuLayout::new(viewport).visible_rows;
        let maximum = self.entries.len().saturating_sub(visible_rows);
        if direction > 0.0 {
            self.scroll = self.scroll.saturating_sub(1);
        } else if direction < 0.0 {
            self.scroll = (self.scroll + 1).min(maximum);
        }
    }

    pub(crate) fn queue(&self, renderer: &mut GuiRenderer, viewport: [f32; 2], cursor: [f32; 2]) {
        let layout = MenuLayout::new(viewport);
        renderer.queue_rect(
            layout.panel.centre,
            layout.panel.size,
            [0.025, 0.035, 0.065, 0.96],
        );
        match &self.mode {
            MenuMode::List => self.queue_list(renderer, layout, cursor),
            MenuMode::Create(form) => queue_creation(renderer, layout, form, cursor),
            MenuMode::ConfirmDelete { label, .. } => {
                queue_confirmation(renderer, layout, label, cursor);
            }
        }
    }

    fn queue_list(&self, renderer: &mut GuiRenderer, layout: MenuLayout, cursor: [f32; 2]) {
        queue_title(renderer, layout, "DEEPTEK WORLDS");
        queue_button(
            renderer,
            layout.create_button,
            cursor,
            "CREATE NEW WORLD",
            [0.10, 0.42, 0.25, 1.0],
        );
        renderer.queue_text(
            "SELECT WORLD",
            [layout.panel.left() + 26.0, layout.rows_top - 31.0],
            2.0,
            [0.62, 0.72, 0.88, 1.0],
        );

        let mut visible_count = 0;
        for (_, entry, load, delete) in self.visible_entries(layout) {
            visible_count += 1;
            renderer.queue_rect(
                load.centre,
                load.size,
                if load.contains(cursor) {
                    [0.18, 0.28, 0.46, 1.0]
                } else {
                    [0.10, 0.16, 0.28, 1.0]
                },
            );
            let maximum_characters = ((load.size[0] - 36.0) / 18.0).floor().max(1.0) as usize;
            let label = truncate_label(&entry.label, maximum_characters);
            renderer.queue_text(
                &label,
                [load.left() + 18.0, load.centre[1] - 10.5],
                3.0,
                [0.93, 0.96, 1.0, 1.0],
            );
            queue_button(renderer, delete, cursor, "DELETE", [0.48, 0.11, 0.12, 1.0]);
        }

        if self.entries.is_empty() {
            queue_centred_text(
                renderer,
                "NO SAVED WORLDS",
                layout.panel.centre[0],
                layout.rows_top + 20.0,
                2.0,
                [0.48, 0.56, 0.68, 1.0],
            );
        } else if self.entries.len() > visible_count {
            let first = self.scroll + 1;
            let last = (self.scroll + visible_count).min(self.entries.len());
            let range = format!("SHOWING {first}-{last}/{total}", total = self.entries.len());
            queue_centred_text(
                renderer,
                &range,
                layout.panel.centre[0],
                layout.panel.bottom() - 28.0,
                1.5,
                [0.50, 0.60, 0.75, 1.0],
            );
        }
        if let Some((status, is_error)) = &self.status {
            queue_centred_text(
                renderer,
                status,
                layout.panel.centre[0],
                layout.panel.bottom() - 51.0,
                2.0,
                if *is_error {
                    [1.0, 0.35, 0.30, 1.0]
                } else {
                    [0.45, 1.0, 0.58, 1.0]
                },
            );
        }
    }

    fn visible_entries(
        &self,
        layout: MenuLayout,
    ) -> impl Iterator<Item = (usize, &WorldEntry, Rect, Rect)> {
        self.entries
            .iter()
            .enumerate()
            .skip(self.scroll)
            .take(layout.visible_rows)
            .map(move |(index, entry)| {
                let (load, delete) = layout.entry_buttons(index - self.scroll);
                (index, entry, load, delete)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creation::CreationField;
    use deep_tek::World;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("deep-tek-world-menu-{suffix}"))
    }

    #[test]
    fn discovery_uses_saved_name_and_falls_back_for_legacy_or_invalid_files() {
        let directory = temporary_directory();
        fs::create_dir_all(&directory).unwrap();
        let mut world = World::empty(1, 1, 7).unwrap();
        world.set_name("Copper Hills").unwrap();
        world.save(directory.join("world_1.world")).unwrap();
        fs::write(directory.join("world_2.world"), []).unwrap();
        fs::write(directory.join("notes.txt"), []).unwrap();
        let entries = discover_worlds(&directory).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|entry| entry.label == "Copper Hills"));
        assert!(entries.iter().any(|entry| entry.label == "WORLD 2"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn next_world_path_skips_existing_numbers() {
        let directory = temporary_directory();
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("world_1.world"), []).unwrap();
        let menu = WorldMenu::new(&directory);
        assert_eq!(menu.next_world_path(), directory.join("world_2.world"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn creation_form_validates_and_returns_all_requested_settings() {
        let mut form = CreationForm::new();
        form.name = "MY WORLD".into();
        form.seed = "42".into();
        form.size = creation::WorldSize::Small;
        let request = form.request(PathBuf::from("test.world")).unwrap();
        assert_eq!(request.name, "MY WORLD");
        assert_eq!(
            (request.seed, request.width, request.height),
            (42, 2_000, 8_000)
        );

        for (size, expected) in [
            (creation::WorldSize::Small, (2_000, 8_000)),
            (creation::WorldSize::Medium, (3_000, 12_000)),
            (creation::WorldSize::Large, (4_000, 16_000)),
        ] {
            form.size = size;
            let request = form.request(PathBuf::from("test.world")).unwrap();
            assert_eq!((request.width, request.height), expected);
        }

        form.active = CreationField::Size;
        form.change_size(1);
        assert_eq!(form.size, creation::WorldSize::Small);
    }

    #[test]
    fn deletion_only_removes_a_discovered_world_after_confirmation_action() {
        let directory = temporary_directory();
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("world_1.world");
        fs::write(&path, []).unwrap();
        let mut menu = WorldMenu::new(&directory);
        let viewport = [800.0, 700.0];
        let layout = MenuLayout::new(viewport);
        let delete_button = menu.visible_entries(layout).next().unwrap().3;
        assert_eq!(menu.handle_click(delete_button.centre, viewport), None);
        assert!(matches!(menu.mode, MenuMode::ConfirmDelete { .. }));
        let confirmation = ConfirmationLayout::new(layout.panel);
        assert_eq!(
            menu.handle_click(confirmation.delete.centre, viewport),
            Some(WorldMenuAction::Delete(path.clone()))
        );
        assert!(path.exists());
        menu.delete_world(&path).unwrap();
        assert!(!path.exists());
        assert!(menu.entries.is_empty());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn long_labels_are_truncated_to_the_requested_width() {
        assert_eq!(truncate_label("A VERY LONG WORLD", 8), "A VER...");
    }

    #[test]
    fn show_root_leaves_creation_and_confirmation_screens() {
        let mut menu = WorldMenu::new(temporary_directory());
        menu.mode = MenuMode::Create(CreationForm::new());
        menu.show_root();
        assert!(menu.is_root());
        menu.mode = MenuMode::ConfirmDelete {
            label: "TEST".into(),
            path: PathBuf::from("test.world"),
        };
        menu.show_root();
        assert!(menu.is_root());
    }
}
