use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_NAME_CHARACTERS: usize = 24;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum WorldSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl WorldSize {
    pub(super) const ALL: [Self; 3] = [Self::Small, Self::Medium, Self::Large];

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Small => "SMALL",
            Self::Medium => "MEDIUM",
            Self::Large => "LARGE",
        }
    }

    pub(super) const fn dimensions(self) -> [u32; 2] {
        match self {
            Self::Small => [2_000, 8_000],
            Self::Medium => [3_000, 12_000],
            Self::Large => [4_000, 16_000],
        }
    }

    fn offset(self, offset: i32) -> Self {
        let index = Self::ALL.iter().position(|&size| size == self).unwrap() as i32;
        Self::ALL[(index + offset).rem_euclid(Self::ALL.len() as i32) as usize]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WorldCreationRequest {
    pub(crate) path: PathBuf,
    pub(crate) name: String,
    pub(crate) seed: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CreationField {
    Name,
    Seed,
    Size,
}

impl CreationField {
    pub(super) const ALL: [Self; 3] = [Self::Name, Self::Seed, Self::Size];

    fn offset(self, offset: i32) -> Self {
        let index = Self::ALL.iter().position(|&field| field == self).unwrap() as i32;
        Self::ALL[(index + offset).rem_euclid(Self::ALL.len() as i32) as usize]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CreationForm {
    pub(super) name: String,
    pub(super) seed: String,
    pub(super) size: WorldSize,
    pub(super) active: CreationField,
    pub(super) error: Option<String>,
    replace_on_input: bool,
}

impl CreationForm {
    pub(super) fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0xD33F_7E57);
        Self {
            name: String::new(),
            seed: seed.to_string(),
            size: WorldSize::default(),
            active: CreationField::Name,
            error: None,
            replace_on_input: false,
        }
    }

    pub(super) fn select(&mut self, index: usize) {
        if let Some(&field) = CreationField::ALL.get(index) {
            self.active = field;
            self.replace_on_input = true;
        }
    }

    pub(super) fn select_size(&mut self, size: WorldSize) {
        self.size = size;
        self.active = CreationField::Size;
        self.replace_on_input = false;
        self.error = None;
    }

    pub(super) fn move_selection(&mut self, offset: i32) {
        self.active = self.active.offset(offset);
        self.replace_on_input = true;
    }

    pub(super) fn change_size(&mut self, offset: i32) {
        if self.active == CreationField::Size {
            self.size = self.size.offset(offset);
            self.error = None;
        }
    }

    pub(super) fn backspace(&mut self) {
        self.replace_on_input = false;
        if let Some(value) = self.active_value_mut() {
            value.pop();
        }
        self.error = None;
    }

    pub(super) fn clear_active(&mut self) {
        if let Some(value) = self.active_value_mut() {
            value.clear();
        }
        self.replace_on_input = false;
        self.error = None;
    }

    fn active_value_mut(&mut self) -> Option<&mut String> {
        match self.active {
            CreationField::Name => Some(&mut self.name),
            CreationField::Seed => Some(&mut self.seed),
            CreationField::Size => None,
        }
    }

    pub(super) fn append_text(&mut self, text: &str) {
        let field = self.active;
        if field == CreationField::Size {
            return;
        }
        if self.replace_on_input {
            self.active_value_mut().unwrap().clear();
            self.replace_on_input = false;
        }
        let value = self.active_value_mut().unwrap();
        for character in text.chars() {
            match field {
                CreationField::Name
                    if value.chars().count() < MAX_NAME_CHARACTERS
                        && (character.is_ascii_alphanumeric()
                            || matches!(character, ' ' | '-')) =>
                {
                    value.push(character.to_ascii_uppercase());
                }
                CreationField::Seed if value.len() < 20 && character.is_ascii_digit() => {
                    value.push(character);
                }
                _ => {}
            }
        }
        self.error = None;
    }

    pub(super) fn request(&mut self, path: PathBuf) -> Option<WorldCreationRequest> {
        let name = self.name.trim().to_owned();
        if name.is_empty() {
            self.error = Some("ENTER A WORLD NAME".into());
            self.active = CreationField::Name;
            return None;
        }
        let Some(seed) = self.seed.parse::<u64>().ok() else {
            self.error = Some("SEED MUST BE 0 TO 18446744073709551615".into());
            self.active = CreationField::Seed;
            return None;
        };
        let [width, height] = self.size.dimensions();
        self.error = None;
        Some(WorldCreationRequest {
            path,
            name,
            seed,
            width,
            height,
        })
    }
}
