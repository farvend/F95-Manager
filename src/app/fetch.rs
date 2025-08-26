use eframe::egui;

use crate::parser::game_info::ThreadId;

use super::rt;
mod helpers;

/// Messages for cover and screenshot loading.
pub enum CoverMsg {
    Ok {
        thread_id: ThreadId,
        w: usize,
        h: usize,
        rgba: Vec<u8>,
    },
    Err {
        thread_id: u64,
    },
    ScreenOk {
        thread_id: u64,
        idx: usize,
        w: usize,
        h: usize,
        rgba: Vec<u8>,
    },
    ScreenErr {
        thread_id: u64,
        idx: usize,
    },
}

impl super::NoLagApp {
    // Common helpers to reduce duplication in library fetch pipelines
    fn compute_library_targets(&self) -> (Vec<(u64, std::path::PathBuf)>, Vec<u64>) {
        let installs: Vec<(u64, std::path::PathBuf)> = helpers::collect_installs();
        let downloading_ids: std::collections::HashSet<u64> =
            self.downloads.keys().copied().collect();
        let pending_ids: Vec<u64> = helpers::collect_pending_ids();
        let targets: Vec<u64> = helpers::build_targets(&installs, &downloading_ids, &pending_ids);
        (installs, targets)
    }

    fn build_existing_map_for_refresh(
        &self,
    ) -> std::collections::HashMap<u64, crate::parser::F95Thread> {
        if self.lib_result.is_some() {
            helpers::build_existing_map(self.lib_result.as_ref())
        } else {
            helpers::build_existing_map(self.last_result.as_ref())
        }
    }

    fn spawn_lib_pipeline_concurrent(
        &self,
        ctx: &egui::Context,
        installs: Vec<(u64, std::path::PathBuf)>,
        targets: Vec<u64>,
        existing_map: std::collections::HashMap<u64, crate::parser::F95Thread>,
    ) {
        let tx = self.lib_tx.clone();
        let ctx2 = ctx.clone();
        super::rt().spawn(async move {
            if targets.is_empty() {
                let empty = helpers::make_msg_from_threads(Vec::new());
                let _ = tx.send(Ok(empty));
                ctx2.request_repaint();
                return;
            }

            // Initial list from cache (if any) + placeholders
            let install_map: std::collections::HashMap<u64, std::path::PathBuf> =
                helpers::build_install_map(&installs);

            let mut all_found: Vec<crate::parser::F95Thread> =
                helpers::fill_threads_from_targets(&targets, &existing_map, &install_map);

            // Send initial snapshot
            let mut result = helpers::make_msg_from_threads(all_found.clone());
            let _ = tx.send(Ok(result.clone()));
            ctx2.request_repaint();

            // Enrich only cards with missing data, strictly from the thread page (concurrently)
            let to_enrich: Vec<u64> = all_found
                .iter()
                .filter(|t| helpers::needs_enrich(t))
                .map(|t| t.thread_id.get())
                .collect();

            let mut set = tokio::task::JoinSet::new();
            for id in to_enrich {
                set.spawn(async move {
                    let meta = crate::parser::game_info::thread_meta::fetch_thread_meta(id).await;
                    (id, meta)
                });
            }

            while let Some(joined) = set.join_next().await {
                if let Ok((id, mut meta)) = joined {
                    if let Some(meta) = meta {
                        if let Some(th) = all_found.iter_mut().find(|t| t.thread_id.get() == id) {
                            let (sc_len, tg_len) =
                                helpers::apply_meta(th, meta);
                            log::info!(
                                "Prefetch meta for {id}: screens={sc_len} tags={tg_len}"
                            );
                        }

                        // Push incremental update
                        result = helpers::make_msg_from_threads(all_found.clone());
                        let _ = tx.send(Ok(result.clone()));
                        ctx2.request_repaint();
                    } else {
                        log::warn!("Couldn't prefetch metadata for {id} from F95 page.");

                    }
                }
                    
            }
        });
    }

    fn spawn_lib_pipeline_sequential_with_req(
        &self,
        ctx: &egui::Context,
        req_id: u64,
        installs: Vec<(u64, std::path::PathBuf)>,
        targets: Vec<u64>,
        existing_map: std::collections::HashMap<u64, crate::parser::F95Thread>,
    ) {
        let tx2 = self.tx.clone();
        let ctx3 = ctx.clone();
        super::rt().spawn(async move {
            if targets.is_empty() {
                let empty = helpers::make_msg_from_threads(Vec::new());
                let _ = tx2.send((req_id, Ok(empty)));
                ctx3.request_repaint();
                return;
            }

            // Initial list from cache (if any) + placeholders
            let install_map: std::collections::HashMap<u64, std::path::PathBuf> =
                helpers::build_install_map(&installs);

            let mut all_found: Vec<crate::parser::F95Thread> =
                helpers::fill_threads_from_targets(&targets, &existing_map, &install_map);

            // Send initial snapshot
            log::info!("Direct library initial: items={}", all_found.len());
            let result = helpers::make_msg_from_threads(all_found.clone());
            let _ = tx2.send((req_id, Ok(result)));
            ctx3.request_repaint();

            // Enrich only cards with missing data, strictly from the thread page
            let to_enrich: Vec<u64> = all_found
                .iter()
                .filter(|t| helpers::needs_enrich(t))
                .map(|t| t.thread_id.get())
                .collect();

            for id in to_enrich {
                log::info!("Direct enrich thread {}", id);
                if let Some(mut meta) =
                    crate::parser::game_info::thread_meta::fetch_thread_meta(id).await
                {
                    if let Some(th) = all_found.iter_mut().find(|t| t.thread_id.get() == id) {
                        let (sc_len, tg_len) =
                            helpers::apply_meta(th, meta);
                        log::info!(
                            "Direct meta fetched for {id}: screens={sc_len} tags={tg_len}"
                        );
                    }

                    // Push incremental update
                    let result2 = helpers::make_msg_from_threads(all_found.clone());
                    let _ = tx2.send((req_id, Ok(result2)));
                    ctx3.request_repaint();
                } else {
                    log::warn!("Couldn't fetch metadata for {id} from F95 page.");
                }
            }
        });
    }

    /// Start async fetch for threads list based on current filters.
    pub(super) fn start_fetch(&mut self, ctx: &egui::Context) {
        // Allow restarting fetch even if one is in-flight; results are deduped by request id
        self.loading = true;
        // Reset last state so UI shows loading spinner and clears previous error
        self.last_error = None;
        self.last_result = None;
        ctx.request_repaint();

        // bump fetch request id
        self.counter = self.counter.wrapping_add(1);
        let req_id = self.counter;

        let tx = self.tx.clone();
        let ctx2 = ctx.clone();
        let page = self.page;

        // Build filters with full set mapped from UI state
        let filters = crate::parser::F95Filters::default()
            .with_category("games")
            .with_search_query(self.query.clone())
            .with_sort(self.sort.clone())
            .with_include_tags(self.include_tags.clone())
            .with_exclude_tags(self.exclude_tags.clone())
            .with_prefixes(self.include_prefixes.clone())
            .with_noprefixes(self.exclude_prefixes.clone())
            .with_date_limit(self.date_limit);

        rt().spawn(async move {
            let res = crate::parser::fetch_list_page(page, &filters).await;

            let _ = tx.send((req_id, res));
            ctx2.request_repaint();
        });
    }

    /// Start async fetch of all pages to build the Library view from installed games on disk.
    pub(super) fn start_fetch_library(&mut self, ctx: &egui::Context) {
        log::info!("Library fetch start");
        self.loading = true;
        self.last_error = None;
        self.last_result = None;
        ctx.request_repaint();

        // bump fetch request id
        self.counter = self.counter.wrapping_add(1);
        let req_id = self.counter;

        let (installs, targets) = self.compute_library_targets();
        log::info!(
            "Library targets count: {} (installed: {}, downloading: {})",
            targets.len(),
            installs.len(),
            self.downloads.len()
        );

        // Snapshot current results so we don't re-fetch if a card is already filled
        let existing_map =
            helpers::build_existing_map(self.last_result.as_ref());

        self.spawn_lib_pipeline_sequential_with_req(ctx, req_id, installs, targets, existing_map);

        // Do not scan listing pages at all in Library mode
        return;
    }

    /// Start background prefetch of Library data right after app start.
    pub(super) fn start_prefetch_library(&mut self, ctx: &egui::Context) {
        if self.lib_started {
            return;
        }
        self.lib_started = true;
        self.lib_error = None;
        self.lib_result = None;

        let (installs, targets) = self.compute_library_targets();

        // Snapshot current results so we don't re-fetch if a card is already filled
        let existing_map: std::collections::HashMap<u64, crate::parser::F95Thread> =
            helpers::build_existing_map(self.last_result.as_ref());

        self.spawn_lib_pipeline_concurrent(ctx, installs, targets, existing_map);
    }

    /// Refresh background Library data snapshot, including current in-progress downloads.
    pub(super) fn refresh_prefetch_library(&mut self, ctx: &egui::Context) {
        // Non-destructive refresh: do not flip lib_started or clear current lib_result.
        let (installs, targets) = self.compute_library_targets();

        // Snapshot current results (prefer existing lib_result) so we don't re-fetch if a card is already filled
        let existing_map: std::collections::HashMap<u64, crate::parser::F95Thread> =
            self.build_existing_map_for_refresh();

        self.spawn_lib_pipeline_concurrent(ctx, installs, targets, existing_map);
    }

    /// Schedule background cover downloads for newly arrived items.
    pub(super) fn schedule_cover_downloads(&mut self, ctx: &egui::Context) {
        if let Some(msg) = &self.last_result {
            for t in &msg.data {
                let thread_id = t.thread_id.clone();
                let id = t.thread_id.get();
                if self.covers.contains_key(&id)
                    || self.covers_loading.contains(&id)
                    || t.cover.is_empty()
                {
                    continue;
                }
                self.covers_loading.insert(id);
                // Prefer cover; if missing, fallback to first screenshot so the main tile isn't blank.
                let url_raw = if let Some(u) = helpers::get_cover_or_first_screen_url(t) {
                    u
                } else {
                    // nothing to show
                    self.covers_loading.remove(&id);
                    continue;
                };
                let url = crate::parser::normalize_url(&url_raw);
                let tx = self.cover_tx.clone();
                let ctx2 = ctx.clone();
                log::info!("cover schedule: id={} url={}", id, url);
                rt().spawn(async move {
                    let result = crate::parser::fetch_image_f95(&url).await;

                    let _ = tx.send(match result {
                        Ok((w, h, rgba)) => CoverMsg::Ok {
                            thread_id,
                            w,
                            h,
                            rgba,
                        },
                        Err(err) => {
                            log::warn!("cover fetch failed: id={} err={} url={}", id, err, url);
                            CoverMsg::Err { thread_id: id }
                        }
                    });
                    ctx2.request_repaint();
                });
            }
        }
    }

    /// Poll incoming async messages and update state accordingly.
    pub(super) fn poll_incoming(&mut self, ctx: &egui::Context) {
        // Fetch results
        while let Ok((id, res)) = self.rx.try_recv() {
            // Ignore stale results from previous requests
            if id != self.counter {
                continue;
            }
            self.loading = false;
            match res {
                Ok(msg) => {
                    self.last_error = None;
                    self.last_result = Some(msg);
                    self.schedule_cover_downloads(ctx);
                }
                Err(e) => {
                    self.last_result = None;
                    self.last_error = Some(e.to_string());
                }
            }
        }

        // Handle prefetched Library results
        while let Ok(res) = self.lib_rx.try_recv() {
            match res {
                Ok(msg) => {
                    self.lib_error = None;
                    self.lib_result = Some(msg.clone());
                    // If user is in Library view and waiting for data, show immediately
                    if self.library_only {
                        self.last_result = Some(msg);
                        self.last_error = None;
                        self.loading = false;
                        self.schedule_cover_downloads(ctx);
                    }
                }
                Err(e) => {
                    self.lib_result = None;
                    self.lib_error = Some(e.to_string());
                    if self.library_only {
                        self.last_error = Some(e.to_string());
                        self.loading = false;
                    }
                }
            }
            ctx.request_repaint();
        }

        // Images (covers/screens)
        while let Ok(msg) = self.cover_rx.try_recv() {
            match msg {
                CoverMsg::Ok { thread_id, w, h, rgba } => {
                    let thread_id = thread_id.get();
                    let image = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                    let tex = ctx.load_texture(
                        format!("cover_{:?}", thread_id),
                        image,
                        egui::TextureOptions::default(),
                    );
                    self.covers.insert(thread_id, tex);
                    self.covers_loading.remove(&thread_id);
                    log::info!("cover ok: id={} size={}x{}", thread_id, w, h);
                }
                CoverMsg::Err { thread_id } => {
                    self.covers_loading.remove(&thread_id);
                }
                CoverMsg::ScreenOk { thread_id, idx, w, h, rgba } => {
                    let image = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                    let tex = ctx.load_texture(
                        format!("screen_{}_{}", thread_id, idx),
                        image,
                        egui::TextureOptions::default(),
                    );
                    let entry = self.screens.entry(thread_id).or_insert_with(|| Vec::new());
                    if entry.len() < idx + 1 {
                        entry.resize_with(idx + 1, || None);
                    }
                    entry[idx] = Some(tex);
                    self.screens_loading.remove(&(thread_id, idx));
                }
                CoverMsg::ScreenErr { thread_id, idx } => {
                    self.screens_loading.remove(&(thread_id, idx));
                }
            }
        }
    }
}
