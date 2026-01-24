use eframe::egui;
use lazy_static::lazy_static;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

use super::check::{check_all_updates, GameUpdateInfo};

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
