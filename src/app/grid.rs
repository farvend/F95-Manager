use eframe::egui;

use crate::views::cards::thread_card;
use crate::game_download;

/// Grid rendering and on-demand screenshot downloading logic split from app.rs.
impl super::NoLagApp {
    fn spawn_screen_download(
        &self,
        ctx: &egui::Context,
        thread_id: u64,
        idx: usize,
        url: String,
    ) {
        let tx = self.cover_tx.clone();
        let ctx2 = ctx.clone();
        let url_cloned = url.clone();
        super::rt().spawn(async move {
            let result = crate::parser::fetch_image_f95(&url_cloned).await;

            let msg = match result {
                Ok((w, h, rgba)) => {
                    log::info!(
                        "screen ok: id={} idx={} size={}x{} url={}",
                        thread_id,
                        idx,
                        w,
                        h,
                        url_cloned
                    );
                    super::CoverMsg::ScreenOk { thread_id, idx, w, h, rgba }
                }
                Err(err) => {
                    log::warn!(
                        "screen fetch failed: id={} idx={} err={} url={}",
                        thread_id,
                        idx,
                        err,
                        url_cloned
                    );
                    super::CoverMsg::ScreenErr { thread_id, idx }
                }
            };
            let _ = tx.send(msg);
            ctx2.request_repaint();
        });
    }

    pub(super) fn on_card_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        t: &crate::parser::F95Thread,
        card_w: f32,
        gap: f32,
        c: usize,
        cols: usize,
    ) {
        ui.vertical(|ui| {
            ui.set_min_width(card_w);
            ui.set_max_width(card_w);
            let id = t.thread_id.get();
            let hover = {
                let cover = self.covers.get(&id);
                let screens_slice = self.screens.get(&id).map(|v| v.as_slice());
                let download_progress = self.downloads.get(&id).map(|s| s.progress);
                thread_card(ui, t, card_w, cover, screens_slice, download_progress)
            };

            // Prefetch all screenshots as soon as the cursor hovers the card
            if hover.hovered {
                // Collect targets without holding a mutable borrow of self across spawns
                let mut to_download: Vec<(usize, String)> = Vec::new();
                {
                    let entry = self
                        .screens
                        .entry(id)
                        .or_insert_with(|| vec![None; t.screens.len()]);
                    if entry.len() < t.screens.len() {
                        entry.resize_with(t.screens.len(), || None);
                    }
                    for (idx, url) in t.screens.iter().enumerate() {
                        if !url.is_empty() && entry.get(idx).and_then(|s| s.as_ref()).is_none() {
                            to_download.push((idx, crate::parser::normalize_url(url)));
                        }
                    }
                }
                for (idx, url) in to_download {
                    if !self.screens_loading.contains(&(id, idx)) {
                        self.screens_loading.insert((id, idx));
                        self.spawn_screen_download(ctx, id, idx, url);
                    }
                }
            }

            // Also keep lazy-load on a specific hovered marker (safe due to dedupe guards)
            if let Some(idx) = hover.hovered_line {
                // Determine a single target without overlapping borrows
                let mut maybe_url: Option<String> = None;
                {
                    let entry = self
                        .screens
                        .entry(id)
                        .or_insert_with(|| vec![None; t.screens.len()]);
                    if idx < entry.len() && entry[idx].is_none() {
                        if let Some(url) = t.screens.get(idx) {
                            if !url.is_empty() {
                                maybe_url = Some(crate::parser::normalize_url(url));
                            }
                        }
                    }
                }
                if let Some(url) = maybe_url {
                    if !self.screens_loading.contains(&(id, idx)) {
                        self.screens_loading.insert((id, idx));
                        self.spawn_screen_download(ctx, id, idx, url);
                    }
                }
            }
            if hover.download_clicked {
                if !self.downloads.contains_key(&id) {
                    // Persist pending download in settings so Library can show it across restarts
                    super::settings::record_pending_download(id);
                    let rx = game_download::create_download_task(t.thread_id.get_page());
                    self.downloads.insert(id, super::downloads::DownloadState {
                        rx,
                        progress: 0.0,
                        error: None,
                        completed: false,
                    });
                    // Update background Library snapshot to include this downloading thread immediately
                    self.refresh_prefetch_library(ctx);
                    // Ensure Library view includes this in-progress download immediately
                    if self.library_only {
                        self.start_fetch_library(ctx);
                    }
                    ctx.request_repaint();
                }
            }

            // Download errors (if any)
            if let Some(state) = self.downloads.get(&id) {
                if let Some(err) = &state.error {
                    ui.colored_label(eframe::egui::Color32::RED, format!("Error: {}", err));
                }
            }
        });
        if c + 1 < cols {
            ui.add_space(gap);
        }
    }

    pub(super) fn draw_threads_grid(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        data: &[crate::parser::F95Thread],
        cols: usize,
        left_pad: f32,
        gap: f32,
        card_w: f32,
    ) {
        let mut i = 0usize;
        while i < data.len() {
            ui.horizontal(|ui| {
                ui.add_space(left_pad);
                for c in 0..cols {
                    if let Some(t) = data.get(i + c) {
                        self.on_card_ui(ui, ctx, t, card_w, gap, c, cols);
                    }
                }
            });
            ui.add_space(gap);
            i += cols;
        }
    }
}
