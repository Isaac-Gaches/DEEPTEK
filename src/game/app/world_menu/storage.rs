use deep_tek::World;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorldEntry {
    pub(super) label: String,
    pub(super) path: PathBuf,
}

pub(super) fn discover_worlds(directory: &Path) -> io::Result<Vec<WorldEntry>> {
    let read_directory = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut worlds = Vec::new();
    for entry in read_directory {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type()?.is_file()
            || path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_none_or(|extension| !extension.eq_ignore_ascii_case("world"))
        {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let fallback = stem.replace(['_', '-'], " ").to_ascii_uppercase();
        let label = World::read_name(&path)
            .ok()
            .flatten()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(fallback);
        worlds.push(WorldEntry { label, path });
    }
    worlds.sort_by_cached_key(|entry| entry.label.to_ascii_uppercase());
    Ok(worlds)
}
