// Game launching logic: choose best executable and start the game.
// Windows has specific spawning strategies; non-Windows reveals the folder.

use std::path::{Path, PathBuf};

use crate::app::settings::store::{
    downloaded_game_exe, downloaded_game_folder, record_downloaded_game, APP_SETTINGS,
};
use super::open::reveal_in_file_manager;

#[cfg(target_os = "windows")]
fn run_executable(path: &Path) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x00000008;
    const CREATE_NEW_CONSOLE: u32 = 0x00000010;

    let dir = path.parent().map(|p| p.to_path_buf());
    // Make path absolute if possible to avoid any resolution differences
    let abs_exe = match std::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => path.to_path_buf(),
    };

    // Try custom launch template if provided (uses {{path}} placeholder)
    {
        let template = APP_SETTINGS.read().map(|s| s.custom_launch.clone()).unwrap_or_default();
        if !template.trim().is_empty() {
            let path_s = abs_exe.to_string_lossy().to_string();
            
            let cmdline = template.replace("{{path}}", &format!("\"{}\"", path_s));

            // Split command line into program and args respecting quotes (simple parser)
            fn split_cmdline(s: &str) -> Vec<String> {
                let mut out = Vec::new();
                let mut cur = String::new();
                let mut in_quotes = false;
                for ch in s.chars() {
                    match ch {
                        '"' => in_quotes = !in_quotes,
                        c if c.is_whitespace() && !in_quotes => {
                            if !cur.is_empty() {
                                out.push(cur.clone());
                                cur.clear();
                            }
                        }
                        _ => cur.push(ch),
                    }
                }
                if !cur.is_empty() {
                    out.push(cur);
                }
                out
            }

            let parts = split_cmdline(&cmdline);
            if let Some((prog, args)) = parts.split_first() {
                let mut child = std::process::Command::new(prog);
                if let Some(d) = &dir {
                    child.current_dir(d);
                }
                child.args(args);
                log::info!("Custom launch (direct): {} {:?}", prog, args);
                match child.spawn() {
                    Ok(_) => {
                        log::info!("Launched game (custom): {}", abs_exe.to_string_lossy());
                        return;
                    }
                    Err(e) => {
                        log::error!("Custom launch failed: {}", e);
                        return;
                    }
                }
            } else {
                log::error!("Custom launch: empty command after parsing");
                return;
            }
        }
    }

    // Launch directly without invoking a shell to avoid cmd injection
    {
        let mut direct = std::process::Command::new(&abs_exe);
        if let Some(d) = &dir {
            direct.current_dir(d);
        }
        direct.creation_flags(DETACHED_PROCESS | CREATE_NEW_CONSOLE);
        match direct.spawn() {
            Ok(_) => {
                log::info!("Launched game (direct): {}", abs_exe.to_string_lossy());
                return;
            }
            Err(e) => {
                log::warn!(
                    "Direct launch failed for {}: {}",
                    abs_exe.to_string_lossy(),
                    e
                );
            }
        }
    }

    // Try PowerShell Start-Process (ShellExecute) as fallback
    {
        let mut ps_cmd = String::new();
        ps_cmd.push_str("Start-Process -FilePath \\\"");
        ps_cmd.push_str(&abs_exe.to_string_lossy());
        ps_cmd.push_str("\\\"");
        let mut pwsh = std::process::Command::new("powershell");
        pwsh.arg("-NoProfile").arg("-Command").arg(&ps_cmd);
        if let Some(d) = &dir {
            pwsh.current_dir(d);
        }
        log::info!("PS Start-Process: {}", ps_cmd);
        match pwsh.spawn() {
            Ok(_) => {
                log::info!("Launched game (powershell): {}", abs_exe.to_string_lossy());
                return;
            }
            Err(e) => {
                log::warn!(
                    "PowerShell Start-Process failed for {}: {}",
                    abs_exe.to_string_lossy(),
                    e
                );
            }
        }
    }

    // Final fallback: reveal folder for manual start
    if let Some(d) = &dir {
        reveal_in_file_manager(d);
    }
}

#[cfg(target_os = "windows")]
fn find_exe_closest_to_root(root: &Path) -> Option<PathBuf> {
    use std::collections::VecDeque;
    use std::fs;

    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    queue.push_back(root.to_path_buf());

    while let Some(dir) = queue.pop_front() {
        let mut exes: Vec<PathBuf> = Vec::new();
        let mut subdirs: Vec<PathBuf> = Vec::new();

        if let Ok(rd) = fs::read_dir(&dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    subdirs.push(p);
                } else if p.is_file() {
                    let is_exe = p
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|s| s.eq_ignore_ascii_case("exe"))
                        .unwrap_or(false);
                    if is_exe {
                        exes.push(p);
                    }
                }
            }
        }

        if !exes.is_empty() {
            if let Some(best) = pick_best_exe(&exes) {
                return Some(best);
            } else {
                return exes.into_iter().next();
            }
        }

        for sd in subdirs {
            queue.push_back(sd);
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn pick_best_exe(exes: &[PathBuf]) -> Option<PathBuf> {
    // Avoid common non-game executables (uninstallers, redistributables, installers)
    let bad_keywords = [
        "unins",
        "setup",
        "install",
        "vcredist",
        "directx",
        "dxsetup",
        "updater",
        "crash",
        "unitycrash",
        "unitycrashhandler",
        "python",
        "IconUpdater",
        "WindowsIconUpdater"
    ];
    let filtered: Vec<&PathBuf> = exes
        .iter()
        .filter(|p| {
            let name = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            !bad_keywords.iter().any(|k| name.contains(k))
        })
        .collect();

    let candidates: Vec<&PathBuf> = if filtered.is_empty() {
        exes.iter().collect()
    } else {
        filtered
    };
    candidates
        .into_iter()
        .min_by_key(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.len())
                .unwrap_or(usize::MAX)
        })
        .cloned()
}

#[cfg(target_os = "windows")]
fn depth_from(root: &Path, path: &Path) -> Option<usize> {
    let rel = path.strip_prefix(root).ok()?;
    Some(rel.components().count())
}

/// Public: run a downloaded game by thread_id (Windows: try best .exe; others: open folder)
pub fn run_downloaded_game(thread_id: u64) {
    let folder = match downloaded_game_folder(thread_id) {
        Some(f) => f,
        None => return,
    };

    #[cfg(target_os = "windows")]
    {
        let recorded = downloaded_game_exe(thread_id).filter(|p| p.is_file());
        let best = find_exe_closest_to_root(&folder);

        let chosen = match (recorded, best) {
            (Some(r), Some(b)) => {
                let rd = depth_from(&folder, &r).unwrap_or(usize::MAX);
                let bd = depth_from(&folder, &b).unwrap_or(usize::MAX);
                if bd < rd {
                    b
                } else {
                    r
                }
            }
            (Some(r), None) => r,
            (None, Some(b)) => b,
            (None, None) => {
                // Nothing found: open folder for manual start
                reveal_in_file_manager(&folder);
                return;
            }
        };

        // Persist the chosen exe (cache or update if changed)
        {
            let current = downloaded_game_exe(thread_id);
            if current.as_ref().map(|p| p != &chosen).unwrap_or(true) {
                record_downloaded_game(thread_id, folder.clone(), Some(chosen.clone()));
            }
        }

        run_executable(&chosen);
        return;
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Non-Windows fallback
        reveal_in_file_manager(&folder);
    }
}
