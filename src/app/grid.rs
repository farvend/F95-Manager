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
        let tx = self.images.cover_tx.clone();
        let ctx2 = ctx.clone();
        let url_cloned = url.clone();

        // Try cached file first: cache/<thread_id>/screen_<idx+1>.png
        let cache_path = {
            let base = crate::app::settings::APP_SETTINGS.read().unwrap().cache_dir.clone();
            base.join(thread_id.to_string()).join(format!("screen_{}.png", idx + 1))
        };

        super::rt().spawn(async move {
            let mut served_from_cache = false;
            if tokio::fs::metadata(&cache_path).await.is_ok() {
                match tokio::task::spawn_blocking(move || -> Result<(usize, usize, Vec<u8>), String> {
                    let bytes = std::fs::read(&cache_path).map_err(|e| format!("read cache error: {}", e))?;
                    let img = image::load_from_memory(&bytes).map_err(|e| format!("decode cache error: {}", e))?;
                    let rgba = img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    Ok((w as usize, h as usize, rgba.into_vec()))
                }).await {
                    Ok(Ok((w, h, rgba))) => {
                        log::info!("screen cache hit: id={} idx={}", thread_id, idx);
                        let _ = tx.send(super::CoverMsg::ScreenOk { thread_id, idx, w, h, rgba });
                        served_from_cache = true;
                    }
                    Ok(Err(e)) => {
                        log::warn!("screen cache decode failed: id={} idx={} err={}", thread_id, idx, e);
                    }
                    Err(e) => {
                        log::warn!("screen cache task join failed: id={} idx={} err={}", thread_id, idx, e);
                    }
                }
            }

            if !served_from_cache {
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
            }
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
            if self.filters.library_only {
                super::cache::ensure_cache_for_thread_from(t);
            }
            let hover = {
                let cover = self.images.covers.get(&id);
                let screens_slice = self.images.screens.get(&id).map(|v| v.as_slice());
                let progress = self.downloads.get(&id).and_then(|s| s.progress.clone());
                let link_choices = self.downloads.get(&id).and_then(|s| s.link_choices.as_ref().map(|v| v.as_slice()));
                thread_card(ui, t, card_w, cover, screens_slice, progress, link_choices)
            };

            // Prefetch all screenshots as soon as the cursor hovers the card
            if hover.hovered {
                // Collect targets without holding a mutable borrow of self across spawns
                let mut to_download: Vec<(usize, String)> = Vec::new();
                {
                    let entry = self
                        .images.screens
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
                    if !self.images.screens_loading.contains(&(id, idx)) {
                        self.images.screens_loading.insert((id, idx));
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
                        .images.screens
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
                    if !self.images.screens_loading.contains(&(id, idx)) {
                        self.images.screens_loading.insert((id, idx));
                        self.spawn_screen_download(ctx, id, idx, url);
                    }
                }
            }
            // If user selected a specific link from "SELECT LINK" overlay, start that download
            if let Some(link) = hover.selected_link {
                let rx_new = game_download::create_download_from_link(link);
                // Update or insert download state for this thread without moving the same rx into multiple closures
                if let Some(st) = self.downloads.get_mut(&id) {
                    st.rx = rx_new;
                    st.progress = Some(crate::game_download::Progress::Unknown);
                    st.link_choices = None;
                } else {
                    self.downloads.insert(id, super::downloads::DownloadState {
                        rx: rx_new,
                        progress: Some(crate::game_download::Progress::Unknown),
                        link_choices: None,
                    });
                }
                super::settings::record_pending_download(id);
                // Keep Library in sync immediately
                self.refresh_prefetch_library(ctx);
                ctx.request_repaint();
            }

            if hover.download_clicked {
                // Allow restart if previous attempt failed
                let should_start = match self.downloads.get(&id) {
                    None => true,
                    Some(st) => matches!(st.progress, Some(crate::game_download::Progress::Error(_))),
                };
                if should_start {
                    // Drop previous errored state if present
                    self.downloads.remove(&id);
                    // Persist pending download in settings so Library can show it across restarts
                    super::settings::record_pending_download(id);
                    super::cache::spawn_cache_for_thread_from(t);
                    let rx = game_download::create_download_task(t.thread_id.get_page());
                    self.downloads.insert(id, super::downloads::DownloadState {
                        rx,
                        progress: Some(crate::game_download::Progress::Unknown),
                        link_choices: None,
                    });
                    // Update background Library snapshot to include this downloading thread immediately
                    self.refresh_prefetch_library(ctx);
                    // Library view will update from prefetch immediately via lib_rx; no direct fetch needed here
                    ctx.request_repaint();
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
        // Virtualized grid rendering: draw only the rows that intersect the visible viewport.
        let total_items = data.len();
        if total_items == 0 || cols == 0 {
            return;
        }
        let cols = cols.max(1);
        let total_rows = (total_items + cols - 1) / cols;

        // Compute stable card height based on our fixed layout in thread_card().
        // Layout breakdown:
        // - Frame inner margins: 16 (8 top + 8 bottom)
        // - Cover: (card_w - 16) * 9 / 16
        // - Markers under cover: 12
        // - Gap after cover: 20
        // - Title line: Heading height
        // - Gap after title: 4
        // - Creator line: Small height
        // - Gap after creator: 4
        // - Meta row frame: Small height + 12 (inner vertical margins)
        let heading_h = ui.text_style_height(&egui::TextStyle::Heading);
        let small_h = ui.text_style_height(&egui::TextStyle::Small);
        let inner_w = (card_w - 16.0).max(1.0);
        let cover_h = inner_w * 9.0 / 16.0;
        let markers_h = 12.0;
        let card_h =
            16.0 + cover_h + markers_h + 20.0 + heading_h + 4.0 + small_h + 4.0 + (small_h + 12.0);
        let row_h = card_h + gap;

        // Determine which rows are visible in the current clip rect
        let start_y = ui.cursor().min.y;
        let clip = ui.clip_rect();
        let top_y = clip.top();
        let bottom_y = clip.bottom();

        let mut first_row = ((top_y - start_y) / row_h).floor() as isize;
        let mut last_row = ((bottom_y - start_y) / row_h).ceil() as isize;

        // Overscan a bit for smoothness
        let overscan: isize = 2;
        first_row = (first_row - overscan).max(0);
        last_row = (last_row + overscan).min(total_rows as isize);

        let start_row = first_row as usize;
        let end_row = last_row as usize;

        // Space for skipped rows above
        let top_skip = (start_row as f32) * row_h;
        if top_skip > 0.0 {
            ui.add_space(top_skip);
        }

        // Render only visible rows
        for r in start_row..end_row {
            ui.horizontal(|ui| {
                ui.add_space(left_pad);
                let base = r * cols;
                for c in 0..cols {
                    if let Some(t) = data.get(base + c) {
                        self.on_card_ui(ui, ctx, t, card_w, gap, c, cols);
                    }
                }
            });
            // Keep spacing consistent even on the last row to preserve total height
            ui.add_space(gap);
        }

        // Trailing space for rows below the visible range
        let rendered_rows = end_row.saturating_sub(start_row) as f32;
        let total_h = (total_rows as f32) * row_h;
        let used_h = top_skip + rendered_rows * row_h;
        let bottom_skip = (total_h - used_h).max(0.0);
        if bottom_skip > 0.0 {
            ui.add_space(bottom_skip);
        }
    }
}
