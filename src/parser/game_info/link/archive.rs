use sevenz_rust;
use std::{
    collections::HashSet,
    fs,
    fs::File as StdFile,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tokio::sync::mpsc::UnboundedSender;
use unrar;
use zip::ZipArchive;

use crate::game_download::{GameDownloadStatus, Progress};

fn sanitize_relative_path(name: &str, strip_prefix: Option<&str>) -> Option<PathBuf> {
    // Nested helpers are kept local to avoid polluting the module namespace.
    fn is_windows_reserved(stem_upper: &str) -> bool {
        matches!(
            stem_upper,
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
    }
    fn sanitize_component_str(s: &str) -> Option<String> {
        // Trim trailing spaces/dots which are invalid on Windows
        let mut out = s.to_string();
        while out.ends_with(' ') || out.ends_with('.') {
            out.pop();
        }
        if out.is_empty() {
            return None;
        }
        // Split into stem/ext to check reserved names against stem
        let (stem, ext) = match out.rsplit_once('.') {
            Some((st, ex)) if !st.is_empty() => (st.to_string(), Some(ex.to_string())),
            _ => (out.clone(), None),
        };
        let mut stem_fixed = stem.clone();
        if is_windows_reserved(&stem.to_ascii_uppercase()) {
            stem_fixed.push('_');
        }
        let mut name = stem_fixed;
        if let Some(ex) = ext {
            name.push('.');
            name.push_str(&ex);
        }
        Some(name)
    }

    let mut s = name.replace('\\', "/");
    if let Some(prefix) = strip_prefix {
        if s.starts_with(prefix) {
            s = s[prefix.len()..].to_string();
        }
    }
    while s.starts_with('/') {
        s.remove(0);
    }
    if s.is_empty() {
        return None;
    }
    let mut out = PathBuf::new();
    for comp in Path::new(&s).components() {
        use std::path::Component::*;
        match comp {
            Normal(os) => {
                let piece = os.to_string_lossy();
                if let Some(clean) = sanitize_component_str(&piece) {
                    out.push(clean);
                } else {
                    return None;
                }
            }
            CurDir => {}
            RootDir | Prefix(_) | ParentDir => return None,
        }
    }
    Some(out)
}

fn find_first_exe(dir: &Path) -> Option<PathBuf> {
    fn rec(cur: &Path) -> Option<PathBuf> {
        let entries = std::fs::read_dir(cur).ok()?;
        for e in entries {
            let e = e.ok()?;
            let p = e.path();
            if p.is_dir() {
                if let Some(found) = rec(&p) {
                    return Some(found);
                }
            } else if p
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("exe"))
                == Some(true)
            {
                // Skip blacklisted executables commonly not the game launcher
                let name_lc = p
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_ascii_lowercase());
                let is_blacklisted = name_lc
                    .as_deref()
                    .map(|n| {
                        n.contains("unitycrashhandler")
                            || n.contains("unitycrash")
                            || n.contains("python")
                            || n.contains("WindowsIconUpdater")
                    })
                    .unwrap_or(false);
                if is_blacklisted {
                    continue;
                }
                return Some(p);
            }
        }
        None
    }
    rec(dir)
}

fn unzip_streaming(
    zip_path: &Path,
    dest_base: &Path,
    sd: &UnboundedSender<GameDownloadStatus>,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    // Only process .zip files
    if zip_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.eq_ignore_ascii_case("zip"))
        != Some(true)
    {
        return Ok((dest_base.to_path_buf(), None));
    }

    let file = StdFile::open(zip_path).map_err(|e| format!("Open zip failed: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("Read zip failed: {e}"))?;

    // Detect single top-level folder and whether there are root files
    let mut top_levels: HashSet<String> = HashSet::new();
    let mut root_files = false;
    for i in 0..archive.len() {
        let name_owned = archive
            .by_index(i)
            .map_err(|e| format!("Zip idx {i} err: {e}"))?
            .name()
            .to_string();
        let n = name_owned.replace('\\', "/");
        if let Some(pos) = n.find('/') {
            let top = &n[..pos];
            if !top.is_empty() {
                top_levels.insert(top.to_string());
            }
        } else {
            root_files = true;
            if !n.is_empty() {
                top_levels.insert(n.clone());
            }
        }
    }
    let strip_prefix = if !root_files && top_levels.len() == 1 {
        Some(format!("{}/", top_levels.iter().next().unwrap()))
    } else {
        None
    };

    // Destination: use archive_dest_dir(...) and ensure uniqueness to avoid mixing previous runs
    let base_dest = archive_dest_dir(zip_path, dest_base);
    let mut dest_dir = base_dest.clone();
    if dest_dir.exists() {
        let orig_name = dest_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("extracted")
            .to_string();
        let mut idx = 2usize;
        loop {
            let candidate = dest_dir.with_file_name(format!("{orig_name}-{idx}"));
            if !candidate.exists() {
                dest_dir = candidate;
                break;
            }
            idx += 1;
        }
    }
    fs::create_dir_all(&dest_dir).map_err(|e| format!("Create dest dir failed: {e}"))?;

    // Count total bytes to extract (exclude dirs, after sanitize)
    let mut total_bytes: u64 = 0;
    for i in 0..archive.len() {
        let f = archive
            .by_index(i)
            .map_err(|e| format!("Zip idx {i} err: {e}"))?;
        if f.is_dir() {
            continue;
        }
        if let Some(rel) = sanitize_relative_path(f.name(), strip_prefix.as_deref()) {
            if !rel.as_os_str().is_empty() {
                total_bytes = total_bytes.saturating_add(f.size());
            }
        }
    }

    // Extract sequentially with streaming I/O
    let mut extracted_bytes: u64 = 0;
    // Track case-insensitive created file paths to avoid collisions on Windows
    let mut used_rel_lower: HashSet<String> = HashSet::new();

    for i in 0..archive.len() {
        let mut f = archive
            .by_index(i)
            .map_err(|e| format!("Zip idx {i} err: {e}"))?;
        let name = f.name().to_string();
        let is_dir = f.is_dir();

        let rel = match sanitize_relative_path(&name, strip_prefix.as_deref()) {
            Some(p) => p,
            None => continue,
        };
        let mut out_path = dest_dir.join(&rel);

        if is_dir {
            if let Err(e) = fs::create_dir_all(&out_path) {
                log::warn!("Create dir {} failed: {}", out_path.display(), e);
            }
            continue;
        }

        // Ensure parent directories exist
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Create parent {} failed: {}", parent.display(), e))?;
        }

        // Avoid case-insensitive collisions
        let mut rel_key = rel.to_string_lossy().to_ascii_lowercase();
        if used_rel_lower.contains(&rel_key) {
            // Append (2), (3)... before extension
            let file_name = rel.file_name().and_then(|s| s.to_str()).unwrap_or("file");
            let (stem, ext_opt) = match file_name.rsplit_once('.') {
                Some((st, ex)) if !st.is_empty() => (st.to_string(), Some(ex.to_string())),
                _ => (file_name.to_string(), None),
            };
            let mut n = 2usize;
            loop {
                let mut new_name = format!("{stem} ({n})");
                if let Some(ex) = &ext_opt {
                    new_name.push('.');
                    new_name.push_str(ex);
                }
                let candidate_rel = rel.with_file_name(new_name);
                let candidate_key = candidate_rel.to_string_lossy().to_ascii_lowercase();
                if !used_rel_lower.contains(&candidate_key) {
                    out_path = dest_dir.join(&candidate_rel);
                    rel_key = candidate_key;
                    break;
                }
                n += 1;
            }
        }
        used_rel_lower.insert(rel_key);

        // Stream data from ZipFile to disk with a fixed-size buffer
        let mut out_file = StdFile::create(&out_path)
            .map_err(|e| format!("Create {} failed: {}", out_path.display(), e))?;

        let mut buf = [0u8; 64 * 1024];
        loop {
            match Read::read(&mut f, &mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    out_file
                        .write_all(&buf[..n])
                        .map_err(|e| format!("Write {} failed: {}", out_path.display(), e))?;
                    extracted_bytes = extracted_bytes.saturating_add(n as u64);
                    let progress = if total_bytes == 0 {
                        1.0
                    } else {
                        (extracted_bytes as f32) / (total_bytes as f32)
                    };
                    let _ = sd.send(GameDownloadStatus::Unzipping(Progress::Pending(progress)));
                }
                Err(e) => return Err(format!("Read entry {} failed: {}", name, e)),
            }
        }
    }

    // Ensure final 100% notification
    let _ = sd.send(GameDownloadStatus::Unzipping(Progress::Pending(1.0)));

    Ok((dest_dir.clone(), find_first_exe(&dest_dir)))
}

fn archive_dest_dir(archive_path: &Path, dest_base: &Path) -> PathBuf {
    let fname = archive_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("extracted")
        .to_ascii_lowercase();

    // Known multi-part and single extensions we want to strip (longest first)
    const SUFFIXES: [&str; 13] = [
        ".tar.gz", ".tar.bz2", ".tar.xz", ".tgz", ".tbz2", ".txz", ".zip", ".rar", ".7z", ".tar",
        ".gz", ".bz2", ".xz",
    ];

    let mut base = fname.clone();
    for suf in SUFFIXES {
        if base.ends_with(suf) {
            base.truncate(base.len() - suf.len());
            break;
        }
    }
    let stem = if base.is_empty() { "extracted" } else { &base };
    dest_base.join(stem)
}

fn extract_with_sevenz(
    archive_path: &Path,
    dest_base: &Path,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    // Use archive_dest_dir and ensure unique directory to avoid mixing contents
    let base_dest = archive_dest_dir(archive_path, dest_base);
    let mut dest_dir = base_dest.clone();
    if dest_dir.exists() {
        let orig_name = dest_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("extracted")
            .to_string();
        let mut idx = 2usize;
        loop {
            let candidate = dest_dir.with_file_name(format!("{orig_name}-{idx}"));
            if !candidate.exists() {
                dest_dir = candidate;
                break;
            }
            idx += 1;
        }
    }

    std::fs::create_dir_all(&dest_dir).map_err(|e| format!("Create dest dir failed: {e}"))?;
    match sevenz_rust::decompress_file(archive_path, &dest_dir) {
        Ok(()) => Ok((dest_dir.clone(), find_first_exe(&dest_dir))),
        Err(e) => {
            let msg = e.to_string();
            if is_memory_alloc_failure(&msg) {
                Err(format!(
                    "7z decompress failed due to insufficient memory: {msg}"
                ))
            } else {
                Err(format!("7z decompress (pure Rust) failed: {msg}"))
            }
        }
    }
}

fn is_memory_alloc_failure(s: &str) -> bool {
    let lc = s.to_ascii_lowercase();
    lc.contains("memory allocation")
        || lc.contains("can not allocate memory")
        || lc.contains("cannot allocate memory")
        || (lc.contains("allocation of") && lc.contains("bytes failed"))
}

pub fn extract_archive(
    archive_path: &Path,
    dest_base: &Path,
    sd: &UnboundedSender<GameDownloadStatus>,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let name_lower = archive_path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .ok_or_else(|| "Archive has no file name".to_string())?;

    // Supported formats:
    // - .zip (native streaming unzip)
    // - .7z (pure Rust via sevenz_rust)
    // - .rar (via unrar + UnRAR.dll on Windows)
    if name_lower.ends_with(".zip") {
        return unzip_streaming(archive_path, dest_base, sd);
    }
    if name_lower.ends_with(".7z") {
        return extract_with_sevenz(archive_path, dest_base);
    }
    if name_lower.ends_with(".rar") {
        return extract_with_unrar(archive_path, dest_base);
    }

    Err(format!("Unsupported archive format: {}", name_lower))
}

fn extract_with_unrar(
    archive_path: &Path,
    dest_base: &Path,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    // Use archive_dest_dir and ensure unique directory to avoid mixing contents
    let base_dest = archive_dest_dir(archive_path, dest_base);
    let mut dest_dir = base_dest.clone();
    if dest_dir.exists() {
        let orig_name = dest_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("extracted")
            .to_string();
        let mut idx = 2usize;
        loop {
            let candidate = dest_dir.with_file_name(format!("{orig_name}-{idx}"));
            if !candidate.exists() {
                dest_dir = candidate;
                break;
            }
            idx += 1;
        }
    }

    std::fs::create_dir_all(&dest_dir).map_err(|e| format!("Create dest dir failed: {e}"))?;

    // Open for processing and extract every entry under dest_dir
    let rar_path = archive_path
        .to_str()
        .ok_or_else(|| "RAR path contains invalid UTF-8".to_string())?;

    let mut open = unrar::Archive::new(rar_path)
        .open_for_processing()
        .map_err(|e| format!("UnRAR open failed: {e}"))?;

    loop {
        match open.read_header() {
            Ok(Some(hdr)) => {
                // Extract current entry into base directory (creates subdirs as needed)
                open = hdr
                    .extract_with_base(&dest_dir)
                    .map_err(|e| format!("UnRAR extract failed: {e}"))?;
            }
            Ok(None) => break,
            Err(e) => return Err(format!("UnRAR read header failed: {e}")),
        }
    }

    Ok((dest_dir.clone(), find_first_exe(&dest_dir)))
}
