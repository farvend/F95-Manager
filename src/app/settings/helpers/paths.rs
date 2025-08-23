// Path utilities and folder existence checks.

use std::path::{Path, PathBuf};
use crate::app::settings::store::APP_SETTINGS;

fn to_abs(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(cd) => cd.join(p),
            Err(_) => p.to_path_buf(),
        }
    }
}

/// Robustly check if a game folder exists, resolving relative paths via extract_dir.
///
/// Strategy:
/// 1) direct check (absolute or relative to CWD)
/// 2) resolve against current extract_dir keeping relative structure (if any)
/// 3) fallback: match by folder name inside current extract_dir
pub fn game_folder_exists(folder: &Path) -> bool {
    // 1) direct check (absolute or relative to CWD)
    let abs_folder = to_abs(folder);
    if abs_folder.is_dir() {
        return true;
    }
    // 2) resolve against current extract_dir keeping relative structure (if any)
    let extract = { APP_SETTINGS.read().unwrap().extract_dir.clone() };
    let abs_extract = to_abs(&extract);
    if let Ok(rel) = folder.strip_prefix(&extract) {
        let candidate = abs_extract.join(rel);
        if candidate.is_dir() {
            return true;
        }
    }
    // 3) fallback: match by folder name inside current extract_dir
    if let Some(name) = folder.file_name() {
        let candidate = abs_extract.join(name);
        if candidate.is_dir() {
            return true;
        }
    }
    false
}
