// Cache metadata and images for a thread on download click.
use serde::Serialize;
use std::path::PathBuf;

use crate::app::settings::APP_SETTINGS;
use lazy_static::lazy_static;
use std::collections::HashSet;
use std::sync::RwLock;
use std::time::Duration;
use tokio::sync::Semaphore;

#[derive(Serialize)]
struct CachedThreadMeta {
    thread_id: u64,
    title: String,
    creator: String,
    version: String,
    cover_url: String,
    screens: Vec<String>,
    tag_ids: Vec<u32>,
}

// Track in-flight cache tasks to avoid duplicate downloads
lazy_static! {
    static ref CACHING_IN_PROGRESS: RwLock<HashSet<u64>> = RwLock::new(HashSet::new());
}

lazy_static! {
    static ref CACHE_CONCURRENCY: Semaphore = Semaphore::new(3);
}

fn cache_dir_for(thread_id: u64) -> PathBuf {
    let base = { APP_SETTINGS.read().unwrap().cache_dir.clone() };
    base.join(thread_id.to_string())
}

fn cache_meta_path(thread_id: u64) -> PathBuf {
    cache_dir_for(thread_id).join("meta.json")
}

fn has_cache(thread_id: u64) -> bool {
    let p = cache_meta_path(thread_id);
    std::fs::metadata(&p).map(|m| m.is_file()).unwrap_or(false)
}

async fn write_png_file(path: &PathBuf, w: usize, h: usize, rgba: Vec<u8>) -> Result<(), String> {
    // Offload CPU-heavy PNG encoding + file IO to blocking thread pool
    let path2 = path.clone();
    tokio::task::spawn_blocking(move || {
        use image::ColorType;
        use image::ImageEncoder;
        use image::codecs::png::PngEncoder;

        let mut buf: Vec<u8> = Vec::new();
        {
            let mut encoder = PngEncoder::new(&mut buf);
            encoder
                .write_image(&rgba, w as u32, h as u32, ColorType::Rgba8.into())
                .map_err(|e| format!("png encode error: {}", e))?;
        }
        std::fs::write(&path2, buf).map_err(|e| format!("write error: {}", e))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("join error: {}", e))?
}

// Public helpers to opportunistically persist already-downloaded images (no extra HTTP)
pub fn maybe_save_cover_png(thread_id: u64, w: usize, h: usize, rgba: Vec<u8>) {
    let enabled = { APP_SETTINGS.read().unwrap().cache_on_download };
    if !enabled {
        return;
    }
    let base = { APP_SETTINGS.read().unwrap().cache_dir.clone() };
    let dir = base.join(thread_id.to_string());
    let path = dir.join("cover.png");
    if std::fs::metadata(&path)
        .map(|m| m.is_file())
        .unwrap_or(false)
    {
        return;
    }
    // Ensure dir sync to avoid race; ignore errors
    let _ = std::fs::create_dir_all(&dir);
    // Persist in background (blocks CPU in a dedicated thread)
    crate::app::rt().spawn(async move {
        // Limit concurrency
        let _permit = CACHE_CONCURRENCY.acquire().await.unwrap();
        let _ = tokio::fs::create_dir_all(&dir).await;
        if let Err(e) = write_png_file(&path, w, h, rgba).await {
            log::warn!(
                "cache: write cover opportunistic failed {}: {}",
                path.to_string_lossy(),
                e
            );
        }
    });
}

pub fn maybe_save_screen_png(thread_id: u64, idx: usize, w: usize, h: usize, rgba: Vec<u8>) {
    let enabled = { APP_SETTINGS.read().unwrap().cache_on_download };
    if !enabled {
        return;
    }
    let base = { APP_SETTINGS.read().unwrap().cache_dir.clone() };
    let dir = base.join(thread_id.to_string());
    let path = dir.join(format!("screen_{}.png", idx + 1));
    if std::fs::metadata(&path)
        .map(|m| m.is_file())
        .unwrap_or(false)
    {
        return;
    }
    let _ = std::fs::create_dir_all(&dir);
    crate::app::rt().spawn(async move {
        let _permit = CACHE_CONCURRENCY.acquire().await.unwrap();
        let _ = tokio::fs::create_dir_all(&dir).await;
        if let Err(e) = write_png_file(&path, w, h, rgba).await {
            log::warn!(
                "cache: write screen opportunistic failed {}: {}",
                path.to_string_lossy(),
                e
            );
        }
    });
}

/// Ensure cache exists for a thread if the setting is enabled and the game is shown in Library.
/// If cache is missing, spawns a background task to fill it. Idempotent: avoids duplicate tasks.
pub fn ensure_cache_for_thread(thread_id: u64) {
    // Respect setting; default is false
    let enabled = { APP_SETTINGS.read().unwrap().cache_on_download };
    if !enabled {
        return;
    }
    if has_cache(thread_id) {
        return;
    }
    {
        let mut set = CACHING_IN_PROGRESS.write().unwrap();
        if !set.insert(thread_id) {
            // already in progress
            return;
        }
    }
    // Spawn quick fill (meta + cover)
    spawn_cache_quick(thread_id);

    // Watcher: clear in-progress flag when meta.json appears or after timeout
    let meta_path = cache_meta_path(thread_id);
    crate::app::rt().spawn(async move {
        use tokio::time::sleep;
        for _ in 0..60 {
            if tokio::fs::metadata(&meta_path).await.is_ok() {
                break;
            }
            sleep(Duration::from_millis(500)).await;
        }
        let mut set = CACHING_IN_PROGRESS.write().unwrap();
        set.remove(&thread_id);
    });
}

pub fn ensure_cache_for_thread_from(t: &crate::parser::F95Thread) {
    // Use already available thread data to avoid extra HTML request
    let enabled = { APP_SETTINGS.read().unwrap().cache_on_download };
    if !enabled {
        return;
    }
    let id = t.thread_id.get();
    if has_cache(id) {
        return;
    }
    {
        let mut set = CACHING_IN_PROGRESS.write().unwrap();
        if !set.insert(id) {
            return;
        }
    }
    spawn_cache_quick_from(t);

    // Same watcher as id-based ensure to clear in-progress
    let meta_path = cache_meta_path(id);
    crate::app::rt().spawn(async move {
        use tokio::time::sleep;
        for _ in 0..60 {
            if tokio::fs::metadata(&meta_path).await.is_ok() {
                break;
            }
            sleep(Duration::from_millis(500)).await;
        }
        let mut set = CACHING_IN_PROGRESS.write().unwrap();
        set.remove(&id);
    });
}

pub fn spawn_cache_quick_from(t: &crate::parser::F95Thread) {
    // Quick cache using thread fields (meta + cover)
    let enabled = { APP_SETTINGS.read().unwrap().cache_on_download };
    if !enabled {
        return;
    }
    let id = t.thread_id.get();
    let title = t.title.clone();
    let creator = t.creator.clone();
    let version = t.version.clone();
    let cover_url = t.cover.clone();
    let screens = t.screens.clone();
    let tag_ids = t.tags.clone();

    crate::app::rt().spawn(async move {
        // Limit global cache tasks concurrency
        let _permit = CACHE_CONCURRENCY.acquire().await.unwrap();

        let cache_dir: PathBuf = cache_dir_for(id);
        if let Err(e) = tokio::fs::create_dir_all(&cache_dir).await {
            log::warn!(
                "cache-quick-from: failed to create dir {}: {}",
                cache_dir.to_string_lossy(),
                e
            );
            return;
        }

        // Save meta.json
        let meta_json_path = cache_dir.join("meta.json");
        let cached = CachedThreadMeta {
            thread_id: id,
            title,
            creator,
            version,
            cover_url: cover_url.clone(),
            screens,
            tag_ids,
        };
        if let Ok(s) = serde_json::to_string_pretty(&cached) {
            if let Err(e) = tokio::fs::write(&meta_json_path, s).await {
                log::warn!(
                    "cache-quick-from: write meta.json failed {}: {}",
                    meta_json_path.to_string_lossy(),
                    e
                );
            }
        }

        // Save cover if missing
        let cover_path = cache_dir.join("cover.png");
        if tokio::fs::metadata(&cover_path).await.is_err() && !cover_url.is_empty() {
            let url = crate::parser::normalize_url(&cover_url);
            match crate::parser::fetch_image_f95(&url).await {
                Ok((w, h, rgba)) => {
                    if let Err(e) = write_png_file(&cover_path, w, h, rgba).await {
                        log::warn!(
                            "cache-quick-from: save cover {} failed: {}",
                            cover_path.to_string_lossy(),
                            e
                        );
                    }
                }
                Err(e) => {
                    log::warn!("cache-quick-from: fetch cover failed: id={} err={}", id, e);
                }
            }
        }
    });
}

pub fn spawn_cache_quick(thread_id: u64) {
    // Quick cache for Library view: meta + cover only
    let enabled = { APP_SETTINGS.read().unwrap().cache_on_download };
    if !enabled {
        return;
    }
    crate::app::rt().spawn(async move {
        // Limit global cache tasks concurrency
        let _permit = CACHE_CONCURRENCY.acquire().await.unwrap();

        let cache_dir: PathBuf = cache_dir_for(thread_id);
        if let Err(e) = tokio::fs::create_dir_all(&cache_dir).await {
            log::warn!(
                "cache-quick: failed to create dir {}: {}",
                cache_dir.to_string_lossy(),
                e
            );
            return;
        }

        // Fetch thread metadata from thread page
        let meta = match crate::parser::game_info::thread_meta::fetch_thread_meta(thread_id).await {
            Ok(m) => m,
            Err(e) => {
                log::warn!("cache-quick: fetch_thread_meta({}): {}", thread_id, e);
                return;
            }
        };

        // Save meta.json
        let meta_json_path = cache_dir.join("meta.json");
        let cached = CachedThreadMeta {
            thread_id,
            title: meta.title.clone(),
            creator: meta.creator.clone(),
            version: meta.version.clone(),
            cover_url: meta.cover.clone(),
            screens: meta.screens.clone(),
            tag_ids: meta.tag_ids.clone(),
        };
        if let Ok(s) = serde_json::to_string_pretty(&cached) {
            if let Err(e) = tokio::fs::write(&meta_json_path, s).await {
                log::warn!(
                    "cache-quick: write meta.json failed {}: {}",
                    meta_json_path.to_string_lossy(),
                    e
                );
            }
        }

        // Save cover if missing
        let cover_path = cache_dir.join("cover.png");
        if tokio::fs::metadata(&cover_path).await.is_err() {
            let cover_url = crate::parser::normalize_url(&meta.cover);
            match crate::parser::fetch_image_f95(&cover_url).await {
                Ok((w, h, rgba)) => {
                    if let Err(e) = write_png_file(&cover_path, w, h, rgba).await {
                        log::warn!(
                            "cache-quick: save cover {} failed: {}",
                            cover_path.to_string_lossy(),
                            e
                        );
                    }
                }
                Err(e) => {
                    log::warn!(
                        "cache-quick: fetch cover failed: id={} err={}",
                        thread_id,
                        e
                    );
                }
            }
        }
    });
}

pub fn spawn_cache_for_thread_from(t: &crate::parser::F95Thread) {
    // Full cache using thread fields (meta + cover + screens)
    let enabled = { APP_SETTINGS.read().unwrap().cache_on_download };
    if !enabled {
        return;
    }
    let id = t.thread_id.get();
    let title = t.title.clone();
    let creator = t.creator.clone();
    let version = t.version.clone();
    let cover_url = t.cover.clone();
    let screens = t.screens.clone();
    let tag_ids = t.tags.clone();

    // Spawn async task on the runtime
    crate::app::rt().spawn(async move {
        // Limit global cache tasks concurrency
        let _permit = CACHE_CONCURRENCY.acquire().await.unwrap();

        // Create cache/<thread_id> folder under configured base
        let cache_dir: PathBuf = cache_dir_for(id);
        if let Err(e) = tokio::fs::create_dir_all(&cache_dir).await {
            log::warn!(
                "cache-from: failed to create dir {}: {}",
                cache_dir.to_string_lossy(),
                e
            );
            return;
        }

        // Persist metadata JSON
        let meta_json_path = cache_dir.join("meta.json");
        let cached = CachedThreadMeta {
            thread_id: id,
            title,
            creator,
            version,
            cover_url: cover_url.clone(),
            screens: screens.clone(),
            tag_ids,
        };
        if let Ok(s) = serde_json::to_string_pretty(&cached) {
            if let Err(e) = tokio::fs::write(&meta_json_path, s).await {
                log::warn!(
                    "cache-from: write meta.json failed {}: {}",
                    meta_json_path.to_string_lossy(),
                    e
                );
            }
        }

        // Save cover as cover.png (skip if exists)
        let cover_path = cache_dir.join("cover.png");
        if tokio::fs::metadata(&cover_path).await.is_err() && !cover_url.is_empty() {
            let url = crate::parser::normalize_url(&cover_url);
            match crate::parser::fetch_image_f95(&url).await {
                Ok((w, h, rgba)) => {
                    if let Err(e) = write_png_file(&cover_path, w, h, rgba).await {
                        log::warn!(
                            "cache-from: save cover {} failed: {}",
                            cover_path.to_string_lossy(),
                            e
                        );
                    }
                }
                Err(e) => {
                    log::warn!("cache-from: fetch cover failed: id={} err={}", id, e);
                }
            }
        }

        // Save screenshots as screen_1.png, screen_2.png, ... concurrently (skip existing)
        let mut set = tokio::task::JoinSet::new();
        let max = 3usize;
        for (idx, url) in screens.iter().enumerate() {
            let path = cache_dir.join(format!("screen_{}.png", idx + 1));
            if tokio::fs::metadata(&path).await.is_ok() {
                continue;
            }
            if url.is_empty() {
                continue;
            }
            let url2 = crate::parser::normalize_url(url);
            set.spawn(async move {
                let res = crate::parser::fetch_image_f95(&url2).await;
                (idx, res, path)
            });
            if set.len() >= max {
                if let Some(joined) = set.join_next().await {
                    if let Ok((jidx, res, jpath)) = joined {
                        match res {
                            Ok((w, h, rgba)) => {
                                if let Err(e) = write_png_file(&jpath, w, h, rgba).await {
                                    log::warn!(
                                        "cache-from: save screen {} failed: {}",
                                        jpath.to_string_lossy(),
                                        e
                                    );
                                }
                            }
                            Err(e) => {
                                log::warn!(
                                    "cache-from: fetch screen failed: id={} idx={} err={}",
                                    id,
                                    jidx,
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }
        while let Some(joined) = set.join_next().await {
            if let Ok((jidx, res, jpath)) = joined {
                match res {
                    Ok((w, h, rgba)) => {
                        if let Err(e) = write_png_file(&jpath, w, h, rgba).await {
                            log::warn!(
                                "cache-from: save screen {} failed: {}",
                                jpath.to_string_lossy(),
                                e
                            );
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "cache-from: fetch screen failed: id={} idx={} err={}",
                            id,
                            jidx,
                            e
                        );
                    }
                }
            }
        }
    });
}
