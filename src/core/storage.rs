use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn available_space_for_path(path: &Path) -> Result<u64> {
    let lookup_path = existing_ancestor(path)
        .with_context(|| format!("No existing parent found for {}", path.display()))?;
    fs2::available_space(&lookup_path)
        .with_context(|| format!("Failed to read free space for {}", lookup_path.display()))
}

pub fn system_drive_root() -> Option<PathBuf> {
    std::env::var("SystemDrive")
        .ok()
        .map(|drive| PathBuf::from(format!("{drive}\\")))
        .filter(|path| path.exists())
}

fn existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut candidate = if path.is_dir() {
        path.to_path_buf()
    } else if let Some(parent) = path.parent() {
        parent.to_path_buf()
    } else {
        path.to_path_buf()
    };

    loop {
        if candidate.exists() {
            return Some(candidate);
        }
        if !candidate.pop() {
            return None;
        }
    }
}
