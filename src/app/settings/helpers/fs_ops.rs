// Filesystem operations: move and copy directories recursively.

use std::path::Path;

/// Move directory tree from src to dst. Falls back to copy+delete if rename fails.
pub fn move_directory(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src == dst {
        return Ok(());
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::rename(src, dst) {
        Ok(_) => Ok(()),
        Err(e) => {
            log::warn!(
                "rename from {} to {} failed: {}. Falling back to copy+delete",
                src.to_string_lossy(),
                dst.to_string_lossy(),
                e
            );
            copy_dir_all(src, dst)?;
            std::fs::remove_dir_all(src)
        }
    }
}

/// Recursively copy directory tree (or single file).
pub fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_file() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
        return Ok(());
    }
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&from, &to)?;
        } else if file_type.is_file() {
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&from, &to)?;
        } else {
            let _ = std::fs::copy(&from, &to);
        }
    }
    Ok(())
}
