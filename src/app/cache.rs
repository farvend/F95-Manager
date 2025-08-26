// Cache metadata and images for a thread on download click.
use serde::Serialize;
use std::path::PathBuf;

use crate::app::settings::APP_SETTINGS;
use lazy_static::lazy_static;
use std::collections::HashSet;
use std::sync::RwLock;
use std::time::Duration;

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

fn cache_dir_for(thread_id: u64) -> PathBuf {
    PathBuf::from("cache").join(thread_id.to_string())
}

fn cache_meta_path(thread_id: u64) -> PathBuf {
    cache_dir_for(thread_id).join("meta.json")
}

fn has_cache(thread_id: u64) -> bool {
    let p = cache_meta_path(thread_id);
    std::fs::metadata(&p).map(|m| m.is_file()).unwrap_or(false)
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
    // Spawn fill
    spawn_cache_for_thread(thread_id);

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

pub fn spawn_cache_for_thread(thread_id: u64) {
    // Check setting; default is false
    let enabled = { APP_SETTINGS.read().unwrap().cache_on_download };
    if !enabled {
        return;
    }

    // Spawn async task on the runtime
    crate::app::rt().spawn(async move {
        // Create ./cache/<thread_id> folder
        let cache_dir: PathBuf = PathBuf::from("cache").join(thread_id.to_string());
        if let Err(e) = tokio::fs::create_dir_all(&cache_dir).await {
            log::warn!(
                "cache: failed to create dir {}: {}",
                cache_dir.to_string_lossy(),
                e
            );
            return;
        }

        // Fetch thread metadata from thread page
        let meta = match crate::parser::game_info::thread_meta::fetch_thread_meta(thread_id).await {
            Ok(m) => m,
            Err(e) => {
                log::warn!("cache: fetch_thread_meta({}): {}", thread_id, e);
                return;
            }
        };

        // Persist metadata JSON
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
        match serde_json::to_string_pretty(&cached) {
            Ok(s) => {
                if let Err(e) = tokio::fs::write(&meta_json_path, s).await {
                    log::warn!(
                        "cache: write meta.json failed {}: {}",
                        meta_json_path.to_string_lossy(),
                        e
                    );
                }
            }
            Err(e) => {
                log::warn!("cache: serialize meta.json failed: {}", e);
            }
        }

        // Helper: encode RGBA -> PNG and write to disk
        async fn write_png(path: &PathBuf, w: usize, h: usize, rgba: Vec<u8>) -> Result<(), String> {
            use image::codecs::png::PngEncoder;
            use image::ColorType;
            use image::ImageEncoder;

            // Encode to PNG in-memory
            let mut buf: Vec<u8> = Vec::new();
            {
                let mut encoder = PngEncoder::new(&mut buf);
                encoder
                    .write_image(&rgba, w as u32, h as u32, ColorType::Rgba8.into())
                    .map_err(|e| format!("png encode error: {}", e))?;
            }
            tokio::fs::write(path, buf)
                .await
                .map_err(|e| format!("write error: {}", e))?;
            Ok(())
        }

        // Save cover as cover.png
        let cover_url = crate::parser::normalize_url(&meta.cover);
        match crate::parser::fetch_image_f95(&cover_url).await {
            Ok((w, h, rgba)) => {
                let cover_path = cache_dir.join("cover.png");
                if let Err(e) = write_png(&cover_path, w, h, rgba).await {
                    log::warn!(
                        "cache: save cover {} failed: {}",
                        cover_path.to_string_lossy(),
                        e
                    );
                } else {
                    log::info!(
                        "cache: saved cover for {} -> {}",
                        thread_id,
                        cover_path.to_string_lossy()
                    );
                }
            }
            Err(e) => {
                log::warn!("cache: fetch cover failed: id={} err={}", thread_id, e);
            }
        }

        // Save screenshots as screen_1.png, screen_2.png, ...
        for (idx, url) in meta.screens.iter().enumerate() {
            let norm = crate::parser::normalize_url(url);
            match crate::parser::fetch_image_f95(&norm).await {
                Ok((w, h, rgba)) => {
                    let path = cache_dir.join(format!("screen_{}.png", idx + 1));
                    if let Err(e) = write_png(&path, w, h, rgba).await {
                        log::warn!(
                            "cache: save screen {} failed: {}",
                            path.to_string_lossy(),
                            e
                        );
                    } else {
                        log::info!(
                            "cache: saved screen {} for {} -> {}",
                            idx + 1,
                            thread_id,
                            path.to_string_lossy()
                        );
                    }
                }
                Err(e) => {
                    log::warn!(
                        "cache: fetch screen failed: id={} idx={} err={}",
                        thread_id,
                        idx,
                        e
                    );
                }
            }
        }
    });
}
