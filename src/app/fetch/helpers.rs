use crate::parser::F95Thread;
use crate::parser::game_info::ThreadId;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Select preferred image URL for a thread:
/// - Prefer the cover if present
/// - Otherwise fallback to the first screenshot (if any)
pub fn get_cover_or_first_screen_url(t: &F95Thread) -> Option<String> {
    if !t.cover.is_empty() {
        Some(t.cover.clone())
    } else {
        t.screens.first().cloned()
    }
}

/// Collect installed games (thread_id, folder) from settings, filtering non-existing folders.
pub fn collect_installs() -> Vec<(u64, PathBuf)> {
    crate::app::settings::with_settings(|st| {
        st.downloaded_games
            .iter()
            .filter(|g| crate::app::settings::game_folder_exists(&g.folder))
            .map(|g| (g.thread_id, g.folder.clone()))
            .collect()
    })
}

/// Collect persisted pending downloads.
pub fn collect_pending_ids() -> Vec<u64> {
    crate::app::settings::with_settings(|st| st.pending_downloads.clone())
}

/// Build unique list of target thread IDs: installed + downloading + pending.
pub fn build_targets(
    installs: &[(u64, PathBuf)],
    downloading_ids: &HashSet<u64>,
    pending_ids: &[u64],
) -> Vec<u64> {
    let mut targets: Vec<u64> = installs.iter().map(|(id, _)| *id).collect();
    for id in downloading_ids {
        if !targets.contains(id) {
            targets.push(*id);
        }
    }
    for id in pending_ids {
        if !targets.contains(id) {
            targets.push(*id);
        }
    }
    targets
}

/// Snapshot current results into a map so we don't re-fetch if a card is already filled.
pub fn build_existing_map(
    source: Option<&crate::parser::F95Msg>,
) -> HashMap<u64, crate::parser::F95Thread> {
    if let Some(msg) = source {
        msg.data
            .iter()
            .map(|t| (t.thread_id.get(), t.clone()))
            .collect()
    } else {
        HashMap::new()
    }
}

/// Map (thread_id -> install folder) for quick lookups.
pub fn build_install_map(installs: &[(u64, PathBuf)]) -> HashMap<u64, PathBuf> {
    installs.iter().cloned().collect()
}

fn placeholder_title(id: u64, install_map: &HashMap<u64, PathBuf>) -> String {
    install_map
        .get(&id)
        .and_then(|folder| folder.file_name().and_then(|s| s.to_str()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("Thread #{}", id))
}

/// Create a placeholder thread entry when we don't yet have cached data.
pub fn placeholder_thread(
    id: u64,
    install_map: &HashMap<u64, PathBuf>,
) -> crate::parser::F95Thread {
    crate::parser::F95Thread {
        thread_id: ThreadId(id),
        title: placeholder_title(id, install_map),
        creator: String::new(),
        version: String::new(),
        views: 0,
        likes: 0,
        prefixes: Vec::new(),
        tags: Vec::new(),
        rating: 0.0,
        cover: String::new(),
        screens: Vec::new(),
        date: String::new(),
        watched: false,
        ignored: false,
        is_new: false,
        ts: 0,
    }
}

/// Merge targets with the existing cache, creating placeholder cards when needed.
/// Tries to load from disk cache first, falls back to placeholder if not available.
pub fn fill_threads_from_targets(
    targets: &[u64],
    existing_map: &HashMap<u64, crate::parser::F95Thread>,
    install_map: &HashMap<u64, PathBuf>,
) -> Vec<crate::parser::F95Thread> {
    let cache_dir = crate::app::settings::with_settings(|st| st.cache_dir.clone());

    let mut out = Vec::with_capacity(targets.len());
    for id in targets {
        if let Some(ex) = existing_map.get(id) {
            // Already have data in memory
            out.push(ex.clone());
        } else if let Some(cached) = load_from_cache(&cache_dir, *id) {
            // Load from disk cache
            log::info!("Cache hit for thread {}", id);
            out.push(cached);
        } else {
            // No cache, create placeholder
            log::debug!("Cache miss for thread {}", id);
            out.push(placeholder_thread(*id, install_map));
        }
    }
    out
}

/// Build F95Msg from a set of threads as a single-page result.
pub fn make_msg_from_threads(data: Vec<crate::parser::F95Thread>) -> crate::parser::F95Msg {
    let count = data.len() as u64;
    crate::parser::F95Msg {
        data,
        pagination: crate::parser::Pagination { page: 1, total: 1 },
        count,
    }
}

/// Whether a thread still needs enrichment from its thread page.
pub fn needs_enrich(t: &crate::parser::F95Thread) -> bool {
    t.cover.is_empty() || t.tags.is_empty() || t.screens.is_empty()
}

/// Apply parsed metadata to a thread in-place and report metrics for logging.
pub fn apply_meta(
    th: &mut crate::parser::F95Thread,
    meta: crate::parser::game_info::thread_meta::ThreadMeta,
) -> (usize, usize) {
    let screens_len = meta.screens.len();
    let tags_len = meta.tag_ids.len();

    th.title = meta.title;
    th.cover = meta.cover;
    th.screens = meta.screens;
    th.creator = meta.creator;
    th.version = meta.version;

    if tags_len > 0 && th.tags.is_empty() {
        th.tags = meta.tag_ids;
    }

    (screens_len, tags_len)
}

// ============================================================================
// Cache functions for thread metadata
// ============================================================================

/// Cached metadata structure matching the JSON format in cache/<id>/meta.json
#[derive(Serialize, Deserialize, Debug)]
struct CachedThreadMeta {
    thread_id: u64,
    title: String,
    creator: String,
    version: String,
    cover_url: String,
    screens: Vec<String>,
    tag_ids: Vec<u32>,
}

/// Get the path to the meta.json file for a thread
pub fn cache_meta_path(cache_dir: &Path, thread_id: u64) -> PathBuf {
    cache_dir.join(thread_id.to_string()).join("meta.json")
}

/// Load cached metadata from disk for a thread
/// Returns None if the cache doesn't exist or can't be read/parsed
pub fn load_from_cache(cache_dir: &Path, thread_id: u64) -> Option<F95Thread> {
    let path = cache_meta_path(cache_dir, thread_id);

    // Read the file
    let data = match std::fs::read_to_string(&path) {
        Ok(d) => d,
        Err(e) => {
            // Don't warn on file not found - that's expected for uncached items
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("Failed to read cache for thread {}: {}", thread_id, e);
            }
            return None;
        }
    };

    // Parse JSON
    let cached: CachedThreadMeta = match serde_json::from_str(&data) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Failed to parse cache for thread {}: {}", thread_id, e);
            return None;
        }
    };

    // Filter screens to only include image files (handle legacy cache with .zip etc)
    let image_extensions = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];
    let filtered_screens: Vec<String> = cached
        .screens
        .into_iter()
        .filter(|s| {
            let ext = s
                .split('?')
                .next()
                .unwrap_or(s)
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_lowercase();
            image_extensions.contains(&ext.as_str())
        })
        .collect();

    // Convert to F95Thread
    Some(F95Thread {
        thread_id: ThreadId(cached.thread_id),
        title: cached.title,
        creator: cached.creator,
        version: cached.version,
        views: 0,
        likes: 0,
        prefixes: Vec::new(),
        tags: cached.tag_ids,
        rating: 0.0,
        cover: cached.cover_url,
        screens: filtered_screens,
        date: String::new(),
        watched: false,
        ignored: false,
        is_new: false,
        ts: 0,
    })
}

/// Save thread metadata to cache
pub fn save_to_cache(cache_dir: &Path, thread_id: u64, thread: &F95Thread) -> std::io::Result<()> {
    let cache_thread_dir = cache_dir.join(thread_id.to_string());

    // Create directory if it doesn't exist
    std::fs::create_dir_all(&cache_thread_dir)?;

    // Build cached structure
    let cached = CachedThreadMeta {
        thread_id,
        title: thread.title.clone(),
        creator: thread.creator.clone(),
        version: thread.version.clone(),
        cover_url: thread.cover.clone(),
        screens: thread.screens.clone(),
        tag_ids: thread.tags.clone(),
    };

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&cached)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    // Write to file
    let path = cache_meta_path(cache_dir, thread_id);
    std::fs::write(&path, json)?;

    log::debug!("Saved cache for thread {} to {}", thread_id, path.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_thread(id: u64) -> F95Thread {
        F95Thread {
            thread_id: ThreadId(id),
            title: "Test Game".to_string(),
            creator: "Test Creator".to_string(),
            version: "v1.0".to_string(),
            views: 1000,
            likes: 500,
            prefixes: vec![1, 2],
            tags: vec![10, 20, 30],
            rating: 4.5,
            cover: "https://example.com/cover.jpg".to_string(),
            screens: vec![
                "https://example.com/screen1.jpg".to_string(),
                "https://example.com/screen2.jpg".to_string(),
            ],
            date: "2024-01-01".to_string(),
            watched: false,
            ignored: false,
            is_new: false,
            ts: 1234567890,
        }
    }

    #[test]
    fn test_save_and_load_cache_success() {
        let temp_dir = std::env::temp_dir().join("f95_test_cache_success");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let thread_id = 12345u64;
        let thread = create_test_thread(thread_id);

        let result = save_to_cache(&temp_dir, thread_id, &thread);
        assert!(result.is_ok(), "Save should succeed");

        let meta_path = cache_meta_path(&temp_dir, thread_id);
        assert!(meta_path.exists(), "Cache file should exist");

        let loaded = load_from_cache(&temp_dir, thread_id);
        assert!(loaded.is_some(), "Load should return Some");

        let loaded_thread = loaded.unwrap();
        assert_eq!(loaded_thread.thread_id.get(), thread_id);
        assert_eq!(loaded_thread.title, "Test Game");
        assert_eq!(loaded_thread.creator, "Test Creator");
        assert_eq!(loaded_thread.version, "v1.0");
        assert_eq!(loaded_thread.cover, "https://example.com/cover.jpg");
        assert_eq!(loaded_thread.screens.len(), 2);
        assert_eq!(loaded_thread.tags, vec![10, 20, 30]);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_cache_missing_file() {
        let temp_dir = std::env::temp_dir().join("f95_test_cache_missing");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let thread_id = 99999u64;
        let loaded = load_from_cache(&temp_dir, thread_id);
        assert!(loaded.is_none(), "Load should return None for missing file");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_cache_corrupted_json() {
        let temp_dir = std::env::temp_dir().join("f95_test_cache_corrupted");
        let _ = fs::remove_dir_all(&temp_dir);

        let thread_id = 54321u64;
        let cache_dir_for_thread = temp_dir.join(thread_id.to_string());
        fs::create_dir_all(&cache_dir_for_thread).unwrap();

        let meta_path = cache_meta_path(&temp_dir, thread_id);
        fs::write(&meta_path, "{ invalid json }").unwrap();

        let loaded = load_from_cache(&temp_dir, thread_id);
        assert!(
            loaded.is_none(),
            "Load should return None for corrupted JSON"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_creates_directory() {
        let temp_dir = std::env::temp_dir().join("f95_test_cache_create_dir");
        let _ = fs::remove_dir_all(&temp_dir);

        let thread_id = 11111u64;
        let thread = create_test_thread(thread_id);

        let result = save_to_cache(&temp_dir, thread_id, &thread);
        assert!(result.is_ok(), "Save should create directory and succeed");

        let cache_dir_for_thread = temp_dir.join(thread_id.to_string());
        assert!(
            cache_dir_for_thread.exists(),
            "Thread cache directory should be created"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_cache_meta_path() {
        let cache_dir = Path::new("/test/cache");
        let thread_id = 123u64;
        let path = cache_meta_path(cache_dir, thread_id);

        assert_eq!(
            path,
            Path::new("/test/cache/123/meta.json"),
            "Cache path should be cache_dir/thread_id/meta.json"
        );
    }
}
