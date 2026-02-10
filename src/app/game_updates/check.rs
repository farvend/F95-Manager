use std::time::Duration;
use tokio::task::JoinSet;

use crate::app::fetch::helpers::load_from_cache;
use crate::app::settings::store::APP_SETTINGS;
use crate::parser::game_info::thread_meta::fetch_thread_meta;

#[derive(Debug, Clone)]
pub struct GameUpdateInfo {
    pub thread_id: u64,
    pub cached_version: String,
    pub new_version: String,
}

pub async fn check_single_game(thread_id: u64) -> Option<GameUpdateInfo> {
    let cache_dir = crate::app::settings::with_settings(|s| s.cache_dir.clone());

    let cached_thread = load_from_cache(&cache_dir, thread_id)?;
    let cached_version = cached_thread.version.clone();

    match fetch_thread_meta(thread_id).await {
        Ok(thread_meta) => {
            let new_version = thread_meta.version;
            if cached_version != new_version {
                Some(GameUpdateInfo {
                    thread_id,
                    cached_version,
                    new_version,
                })
            } else {
                None
            }
        }
        Err(e) => {
            log::warn!("Failed to fetch thread {} metadata: {}", thread_id, e);
            None
        }
    }
}

pub async fn check_all_updates() -> Vec<GameUpdateInfo> {
    let thread_ids: Vec<u64> = crate::app::settings::with_settings(|settings| {
        settings
            .downloaded_games
            .iter()
            .map(|g| g.thread_id)
            .collect()
    });

    if thread_ids.is_empty() {
        return Vec::new();
    }

    let mut set = JoinSet::new();
    let mut results = Vec::new();

    for thread_id in thread_ids {
        if set.len() >= 3 {
            if let Some(res) = set.join_next().await {
                if let Ok(Some(update_info)) = res {
                    results.push(update_info);
                }
            }
        }

        let delay = Duration::from_millis(500);
        set.spawn(async move {
            tokio::time::sleep(delay).await;
            check_single_game(thread_id).await
        });
    }

    while let Some(res) = set.join_next().await {
        if let Ok(Some(update_info)) = res {
            results.push(update_info);
        }
    }

    results
}
