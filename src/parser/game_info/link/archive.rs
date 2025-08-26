use std::{
    collections::HashSet,
    fs,
    fs::File as StdFile,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc as std_mpsc,
};
use threadpool::ThreadPool;
use tokio::sync::mpsc::UnboundedSender;
use zip::ZipArchive;

use crate::game_download::{GameDownloadStatus, Progress};

fn sanitize_relative_path(name: &str, strip_prefix: Option<&str>) -> Option<PathBuf> {
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
            Normal(os) => out.push(os),
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
                    .map(|n| n.contains("unitycrashhandler") || n.contains("unitycrash") || n.contains("python"))
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

fn unzip_with_threadpool(
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

    // Detect single top-level folder
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

    // Destination: <extract_base>/<zip_stem>
    let dest_dir = dest_base.join(
        zip_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("extracted"),
    );
    fs::create_dir_all(&dest_dir).map_err(|e| format!("Create dest dir failed: {e}"))?;

    // Count files to extract (exclude dirs)
    let mut total_files = 0usize;
    for i in 0..archive.len() {
        let f = archive
            .by_index(i)
            .map_err(|e| format!("Zip idx {i} err: {e}"))?;
        if f.is_dir() {
            continue;
        }
        if let Some(rel) = sanitize_relative_path(f.name(), strip_prefix.as_deref()) {
            if !rel.as_os_str().is_empty() {
                total_files += 1;
            }
        }
    }

    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let pool = ThreadPool::new(threads);
    let (tx, rx) = std_mpsc::channel::<Result<(), String>>();

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
        let out_path = dest_dir.join(rel);

        if is_dir {
            if let Err(e) = fs::create_dir_all(&out_path) {
                log::warn!("Create dir {} failed: {}", out_path.display(), e);
            }
            continue;
        }

        // Read entry data (must be sequential with ZipArchive)
        let mut buf = Vec::with_capacity(f.size() as usize);
        if let Err(e) = std::io::Read::read_to_end(&mut f, &mut buf) {
            return Err(format!("Read entry {} failed: {}", name, e));
        }

        let tx2 = tx.clone();
        pool.execute(move || {
            let res = (|| -> Result<(), String> {
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("Create parent {} failed: {}", parent.display(), e))?;
                }
                std::fs::write(&out_path, &buf)
                    .map_err(|e| format!("Write {} failed: {}", out_path.display(), e))?;
                Ok(())
            })();
            let _ = tx2.send(res);
        });
    }

    drop(tx);

    let mut done = 0usize;
    while let Ok(res) = rx.recv() {
        // Update progress
        done += 1;
        let progress = if total_files == 0 {
            1.0
        } else {
            (done as f32) / (total_files as f32)
        };
        let _ = sd.send(GameDownloadStatus::Unzipping(Progress::Pending(progress)));

        if let Err(msg) = res {
            // Surface the first error
            return Err(msg);
        }
    }

    pool.join();
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

fn is_memory_alloc_failure(s: &str) -> bool {
    let lc = s.to_ascii_lowercase();
    lc.contains("memory allocation")
        || lc.contains("can not allocate memory")
        || lc.contains("cannot allocate memory")
        || (lc.contains("allocation of") && lc.contains("bytes failed"))
}

fn try_unrar(archive_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let mut last_err: Option<String> = None;
    for bin in ["unrar", "unrar.exe", "WinRAR.exe"] {
        match Command::new(bin)
            .arg("x")
            .arg("-y")
            .arg("-o+")
            .arg(archive_path.as_os_str())
            .arg(dest_dir.as_os_str())
            .output()
        {
            Ok(out) => {
                if out.status.success() {
                    return Ok(());
                } else {
                    let o = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
                    last_err = Some(format!("{bin} exit status: {} output: {}", out.status, o));
                }
            }
            Err(e) => {
                last_err = Some(format!("{bin} spawn error: {e}"));
            }
        }
    }
    Err(format!(
        "UnRAR CLI not available or failed. Install WinRAR (UnRAR) and ensure it's in PATH. Details: {}",
        last_err.unwrap_or_else(|| "unknown error".to_string())
    ))
}

fn extract_with_7z(
    archive_path: &Path,
    dest_base: &Path,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let dest_dir = archive_dest_dir(archive_path, dest_base);
    std::fs::create_dir_all(&dest_dir).map_err(|e| format!("Create dest dir failed: {e}"))?;

    let name_lower = archive_path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    // Try `7z` first, then `7z.exe`, capturing output to detect specific failures
    let mut last_err: Option<String> = None;
    let mut last_out: Option<String> = None;
    for bin in ["7z", "7z.exe"] {
        match Command::new(bin)
            .arg("x")
            .arg("-y")
            .arg(archive_path.as_os_str())
            .arg(format!("-o\"{}\"", dest_dir.to_string_lossy()))
            .output()
        {
            Ok(out) => {
                if out.status.success() {
                    return Ok((dest_dir.clone(), find_first_exe(&dest_dir)));
                } else {
                    let o = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
                    last_out = Some(o.clone());
                    last_err = Some(format!("{bin} exit status: {}", out.status));

                    // Memory allocation failure on .rar: try UnRAR fallback if available
                    if name_lower.ends_with(".rar") && is_memory_alloc_failure(&o) {
                        match try_unrar(archive_path, &dest_dir) {
                            Ok(()) => return Ok((dest_dir.clone(), find_first_exe(&dest_dir))),
                            Err(unrar_err) => {
                                return Err(format!(
                                    "7-Zip failed due to memory allocation error, and UnRAR fallback also failed: {}. 7-Zip output: {}",
                                    unrar_err,
                                    last_out.unwrap_or_default()
                                ));
                            }
                        }
                    }
                }
            }
            Err(e) => {
                last_err = Some(format!("{bin} spawn error: {e}"));
            }
        }
    }

    Err(format!(
        "7-Zip CLI not available or failed. Install 7-Zip and ensure `7z` is in PATH. Details: {}{}",
        last_err.unwrap_or_else(|| "unknown error".to_string()),
        last_out.as_ref().map(|s| format!("; output: {}", s)).unwrap_or_default()
    ))
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
    // - .zip (native unzip)
    // - .7z, .rar
    // - .tar, .tar.gz, .tgz, .tar.bz2, .tbz2, .tar.xz, .txz
    // - .gz, .bz2, .xz (single-file archives)
    if name_lower.ends_with(".zip") {
        return unzip_with_threadpool(archive_path, dest_base, sd);
    }

    const SUPPORTED_7Z: [&str; 12] = [
        ".7z", ".rar", ".tar", ".tar.gz", ".tgz", ".tar.bz2", ".tbz2", ".tar.xz", ".txz", ".gz",
        ".bz2", ".xz",
    ];
    if SUPPORTED_7Z.iter().any(|suf| name_lower.ends_with(suf)) {
        return extract_with_7z(archive_path, dest_base);
    }

    Err(format!("Unsupported archive format: {}", name_lower))
}
