use std::sync::mpsc;

use eframe::egui;

use crate::game_download::{GameDownloadStatus, Progress};
use crate::parser::game_info::link::DownloadLink;
use crate::ui_constants::download::{DOWNLOAD_WEIGHT, UNZIP_WEIGHT};

pub(super) struct DownloadState {
    pub(super) rx: mpsc::Receiver<GameDownloadStatus>,
    pub(super) progress: Option<Progress>,
    pub(super) link_choices: Option<Vec<DownloadLink>>,
}

/// Helper function to handle progress updates uniformly.
/// DRY principle: Unifies duplicated progress handling logic.
fn handle_progress(
    state: &mut DownloadState,
    progress: Progress,
    ctx: &egui::Context,
    id: u64,
    base_offset: f32,
    weight: f32,
    phase: &str,
) {
    match progress {
        Progress::Pending(p) => {
            let mapped = (base_offset + (weight * p).clamp(0.0, weight)).clamp(0.0, 1.0);
            state.progress = Some(Progress::Pending(mapped));
            ctx.request_repaint();
        }
        Progress::Paused => {
            state.progress = Some(Progress::Paused);
            ctx.request_repaint();
        }
        Progress::Error(e) => {
            if phase == "Unzip" {
                log::error!("error during {}: {e}", phase);
            }
            super::errors_ui::append_error(format!("{} error (thread {}): {}", phase, id, e));
            state.progress = Some(Progress::Error(e));
            ctx.request_repaint();
        }
        Progress::Unknown => {
            state.progress = Some(Progress::Unknown);
            ctx.request_repaint();
        }
    }
}

impl super::NoLagApp {
    pub(super) fn poll_downloads(&mut self, ctx: &egui::Context) {
        let mut done: Vec<u64> = Vec::new();
        let mut need_lib_refresh = false;
        for (id, state) in self.downloads.iter_mut() {
            while let Ok(status) = state.rx.try_recv() {
                match status {
                    GameDownloadStatus::SelectLinks(links) => {
                        // Ask UI to let user select a link; keep progress unknown to show "awaiting" state
                        state.link_choices = Some(links);
                        state.progress = Some(Progress::Unknown);
                        ctx.request_repaint();
                    }
                    GameDownloadStatus::Downloading(progress) => {
                        handle_progress(state, progress, ctx, *id, 0.0, DOWNLOAD_WEIGHT, "Download");
                    }
                    GameDownloadStatus::Unzipping(progress) => {
                        handle_progress(state, progress, ctx, *id, DOWNLOAD_WEIGHT, UNZIP_WEIGHT, "Unzip");
                    }
                    GameDownloadStatus::Completed { dest_dir, exe_path } => {
                        state.progress = None;
                        // Persist installed game info
                        super::settings::record_downloaded_game(*id, dest_dir, exe_path);
                        // Mark to refresh Library snapshot after we finish iterating (avoid borrow conflicts)
                        need_lib_refresh = true;
                        // Remove after loop to avoid borrow conflicts
                        done.push(*id);
                        ctx.request_repaint();
                    }
                }
            }
        }
        for id in done {
            self.downloads.remove(&id);
        }
        // Perform Library refresh after we've released the borrow on self.downloads
        if need_lib_refresh {
            self.refresh_prefetch_library(ctx);
        }
    }
}
