// Migration helper for moving installed games when extract_dir changes.

use std::path::{Path, PathBuf};

use crate::app::settings::helpers::move_directory;

/// Move installed games from old_extract to new_extract,
/// returning updated triples (thread_id, new_folder, new_exe_path).
pub fn migrate_installed_games(
    old_extract: &Path,
    new_extract: &Path,
    entries: Vec<(u64, PathBuf, Option<PathBuf>)>,
) -> Vec<(u64, PathBuf, Option<PathBuf>)> {
    if let Err(e) = std::fs::create_dir_all(new_extract) {
        log::error!(
            "Failed to create new extract dir {}: {}",
            new_extract.to_string_lossy(),
            e
        );
    }

    let mut moved: Vec<(u64, PathBuf, Option<PathBuf>)> = Vec::new();

    for (tid, old_folder, exe) in entries {
        // Skip missing source folder (user may have deleted it manually)
        if !old_folder.exists() {
            log::warn!(
                "Skip moving game {}: source folder not found: {}",
                tid,
                old_folder.to_string_lossy()
            );
            continue;
        }
        // Skip if already inside the new extract dir
        if old_folder.starts_with(new_extract) {
            moved.push((tid, old_folder.clone(), exe.clone()));
            continue;
        }

        // Compute new folder destination
        let mut new_folder = if let Ok(rel) = old_folder.strip_prefix(old_extract) {
            new_extract.join(rel)
        } else {
            match old_folder.file_name() {
                Some(name) => new_extract.join(name),
                None => new_extract.to_path_buf(),
            }
        };

        // Ensure parent exists
        if let Some(parent) = new_folder.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                log::error!(
                    "Failed to create parent {}: {}",
                    parent.to_string_lossy(),
                    e
                );
                continue;
            }
        }

        // If destination exists, pick a non-colliding name by appending _movedN
        if new_folder.exists() {
            let base_name = new_folder
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("game");
            let mut n = 1;
            loop {
                let candidate = new_extract.join(format!("{}_moved{}", base_name, n));
                if !candidate.exists() {
                    new_folder = candidate;
                    break;
                }
                n += 1;
                if n > 1000 {
                    break;
                }
            }
        }

        match move_directory(&old_folder, &new_folder) {
            Ok(_) => {
                // Adjust exe path if it was under old folder or under old_extract
                let new_exe = match exe {
                    Some(ref p) if p.starts_with(&old_folder) => {
                        match p.strip_prefix(&old_folder) {
                            Ok(rel) => Some(new_folder.join(rel)),
                            Err(_) => Some(new_folder.join(p.file_name().unwrap_or_default())),
                        }
                    }
                    Some(ref p) if p.starts_with(old_extract) => {
                        match p.strip_prefix(old_extract) {
                            Ok(rel) => Some(new_extract.join(rel)),
                            Err(_) => Some(p.clone()),
                        }
                    }
                    Some(p) => Some(p),
                    None => None,
                };
                moved.push((tid, new_folder.clone(), new_exe));
            }
            Err(e) => {
                log::error!(
                    "Failed to move game {} from {} to {}: {}",
                    tid,
                    old_folder.to_string_lossy(),
                    new_folder.to_string_lossy(),
                    e
                );
            }
        }
    }

    moved
}
