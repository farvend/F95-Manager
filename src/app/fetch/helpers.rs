use crate::parser::F95Thread;
use crate::parser::game_info::ThreadId;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

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
    let st = crate::app::settings::APP_SETTINGS.read().unwrap();
    st.downloaded_games
        .iter()
        .filter(|g| crate::app::settings::game_folder_exists(&g.folder))
        .map(|g| (g.thread_id, g.folder.clone()))
        .collect()
}

/// Collect persisted pending downloads.
pub fn collect_pending_ids() -> Vec<u64> {
    let st = crate::app::settings::APP_SETTINGS.read().unwrap();
    st.pending_downloads.clone()
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
pub fn fill_threads_from_targets(
    targets: &[u64],
    existing_map: &HashMap<u64, crate::parser::F95Thread>,
    install_map: &HashMap<u64, PathBuf>,
) -> Vec<crate::parser::F95Thread> {
    let mut out = Vec::with_capacity(targets.len());
    for id in targets {
        if let Some(ex) = existing_map.get(id) {
            out.push(ex.clone());
        } else {
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
    let id = th.thread_id.get();

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
