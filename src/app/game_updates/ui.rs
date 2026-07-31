use eframe::egui;
use lazy_static::lazy_static;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

use super::check::{GameUpdateInfo, check_all_updates};

lazy_static! {
    pub static ref GAMES_WITH_UPDATES: RwLock<Vec<GameUpdateInfo>> = RwLock::new(Vec::new());
    static ref CHECK_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
    static ref CHECK_PROGRESS: RwLock<(usize, usize)> = RwLock::new((0, 0));
}

pub fn is_update_available(thread_id: u64) -> bool {
    if let Ok(games) = GAMES_WITH_UPDATES.read() {
        games.iter().any(|g| g.thread_id == thread_id)
    } else {
        false
    }
}

pub fn trigger_update_check(ctx: &egui::Context) {
    if CHECK_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return;
    }

    if let Ok(mut progress) = CHECK_PROGRESS.write() {
        *progress = (0, 0);
    }

    let ctx_clone = ctx.clone();
    crate::app::rt().spawn(async move {
        let updates = check_all_updates().await;

        if let Ok(mut progress) = CHECK_PROGRESS.write() {
            *progress = (updates.len(), updates.len());
        }

        if let Ok(mut games) = GAMES_WITH_UPDATES.write() {
            *games = updates;
        }

        CHECK_IN_PROGRESS.store(false, Ordering::SeqCst);
        ctx_clone.request_repaint();
    });
}

pub fn trigger_update_all() {
    let thread_ids: Vec<u64> = {
        if let Ok(games) = GAMES_WITH_UPDATES.read() {
            games.iter().map(|g| g.thread_id).collect()
        } else {
            return;
        }
    };

    for thread_id in thread_ids {
        let tid = crate::parser::game_info::types::ThreadId(thread_id);
        let page = tid.get_page();
        let _rx = crate::game_download::create_download_task(page);
        crate::app::settings::record_pending_download(thread_id);
    }

    if let Ok(mut games) = GAMES_WITH_UPDATES.write() {
        games.clear();
    }
}
