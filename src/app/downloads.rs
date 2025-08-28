use std::sync::mpsc;

use eframe::egui;

use crate::game_download::GameDownloadStatus;
use crate::parser::game_info::link::DownloadLink;

pub(super) struct DownloadState {
    pub(super) rx: mpsc::Receiver<GameDownloadStatus>,
    pub(super) progress: Option<crate::game_download::Progress>,
    pub(super) link_choices: Option<Vec<DownloadLink>>,
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
                        state.progress = Some(crate::game_download::Progress::Unknown);
                        ctx.request_repaint();
                    }
                    GameDownloadStatus::Downloading(progress) => {
                        match progress {
                            crate::game_download::Progress::Pending(p) => {
                                // Map downloading [0..1] to overall [0..DOWNLOAD_WEIGHT]
                                state.progress = Some(crate::game_download::Progress::Pending(
                                    (super::DOWNLOAD_WEIGHT * p).clamp(0.0, super::DOWNLOAD_WEIGHT),
                                ));
                                ctx.request_repaint();
                            }
                            crate::game_download::Progress::Paused => {
                                state.progress = Some(crate::game_download::Progress::Paused);
                                ctx.request_repaint();
                            }
                            crate::game_download::Progress::Error(e) => {
                                let err_str = e;
                                super::errors_ui::append_error(format!(
                                    "Download error (thread {}): {}",
                                    id, err_str
                                ));
                                state.progress =
                                    Some(crate::game_download::Progress::Error(err_str));
                                ctx.request_repaint();
                            }
                            crate::game_download::Progress::Unknown => {
                                state.progress = Some(crate::game_download::Progress::Unknown);
                                ctx.request_repaint();
                            }
                        }
                    }
                    GameDownloadStatus::Unzipping(progress) => {
                        match progress {
                            crate::game_download::Progress::Pending(p) => {
                                // Map unzipping [0..1] to overall [DOWNLOAD_WEIGHT..1.0]
                                state.progress = Some(crate::game_download::Progress::Pending(
                                    super::DOWNLOAD_WEIGHT
                                        + (super::UNZIP_WEIGHT * p)
                                            .clamp(0.0, super::UNZIP_WEIGHT),
                                ));
                                ctx.request_repaint();
                            }
                            crate::game_download::Progress::Paused => {
                                state.progress = Some(crate::game_download::Progress::Paused);
                                ctx.request_repaint();
                            }
                            crate::game_download::Progress::Error(e) => {
                                log::error!("error during download: {e}");
                                let err_str = e;
                                super::errors_ui::append_error(format!(
                                    "Unzip error (thread {}): {}",
                                    id, err_str
                                ));
                                state.progress =
                                    Some(crate::game_download::Progress::Error(err_str));
                                ctx.request_repaint();
                            }
                            crate::game_download::Progress::Unknown => {
                                state.progress = Some(crate::game_download::Progress::Unknown);
                                ctx.request_repaint();
                            }
                        }
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
