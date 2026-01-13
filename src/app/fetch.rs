use crate::parser::game_info::ThreadId;
use eframe::egui;

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
        if self.net.lib_result.is_some() {
            helpers::build_existing_map(self.net.lib_result.as_ref())
        } else {
            helpers::build_existing_map(self.net.last_result.as_ref())
        }
    }

    fn spawn_lib_pipeline_concurrent(
        &self,
        ctx: &egui::Context,
        installs: Vec<(u64, std::path::PathBuf)>,
        targets: Vec<u64>,
        existing_map: std::collections::HashMap<u64, crate::parser::F95Thread>,
    ) {
        let tx = self.net.lib_tx.clone();
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
                    let res = crate::parser::game_info::thread_meta::fetch_thread_meta(id).await;
                    (id, res)
                });
            }

            while let Some(joined) = set.join_next().await {
                if let Ok((id, res)) = joined {
                    match res {
                        Ok(meta) => {
                            if let Some(th) = all_found.iter_mut().find(|t| t.thread_id.get() == id)
                            {
                                let (sc_len, tg_len) = helpers::apply_meta(th, meta);
                                log::info!(
                                    "Prefetch meta for {id}: screens={sc_len} tags={tg_len}"
                                );
                            }

                            // Push incremental update
                            result = helpers::make_msg_from_threads(all_found.clone());
                            let _ = tx.send(Ok(result.clone()));
                            ctx2.request_repaint();
                        }
                        Err(e) => {
                            log::warn!("Couldn't prefetch metadata for {id}: {}", e);
                        }
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
        let tx2 = self.net.tx.clone();
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
                match crate::parser::game_info::thread_meta::fetch_thread_meta(id).await {
                    Ok(meta) => {
                        if let Some(th) = all_found.iter_mut().find(|t| t.thread_id.get() == id) {
                            let (sc_len, tg_len) = helpers::apply_meta(th, meta);
                            log::info!(
                                "Direct meta fetched for {id}: screens={sc_len} tags={tg_len}"
                            );
                        }

                        // Push incremental update
                        let result2 = helpers::make_msg_from_threads(all_found.clone());
                        let _ = tx2.send((req_id, Ok(result2)));
                        ctx3.request_repaint();
                    }
                    Err(e) => {
                        log::warn!("Couldn't fetch metadata for {id}: {}", e);
                    }
                }
            }
        });
    }

    /// Start async fetch for threads list based on current filters.
    pub(super) fn start_fetch(&mut self, ctx: &egui::Context) {
        // Allow restarting fetch even if one is in-flight; results are deduped by request id
        self.net.loading = true;
        // Reset last state so UI shows loading spinner and clears previous error
        self.net.last_error = None;
        self.net.last_result = None;
        ctx.request_repaint();

        // bump fetch request id
        self.net.counter = self.net.counter.wrapping_add(1);
        let req_id = self.net.counter;

        let tx = self.net.tx.clone();
        let ctx2 = ctx.clone();
        let page = self.page;

        // Build filters with full set mapped from UI state
        let filters = crate::parser::F95Filters::default()
            .with_category("games")
            .with_search_query(self.filters.query.clone())
            .with_sort(self.filters.sort.clone())
            .with_include_tags(self.filters.include_tags.clone())
            .with_exclude_tags(self.filters.exclude_tags.clone())
            .with_prefixes(self.filters.include_prefixes.clone())
            .with_noprefixes(self.filters.exclude_prefixes.clone())
            .with_date_limit(self.filters.date_limit);

        rt().spawn(async move {
            let res = crate::parser::fetch_list_page(page, &filters).await;
            if let Err(err) = &res {
                log::error!("Error getting latest updates: {err:?}");
            }

            let _ = tx.send((req_id, res));
            ctx2.request_repaint();
        });
    }

    /// Start async fetch of all pages to build the Library view from installed games on disk.
    pub(super) fn start_fetch_library(&mut self, ctx: &egui::Context) {
        log::info!("Library fetch start");
        self.net.loading = true;
        self.net.last_error = None;
        self.net.last_result = None;
        ctx.request_repaint();

        // bump fetch request id
        self.net.counter = self.net.counter.wrapping_add(1);
        let req_id = self.net.counter;

        // mark this req_id as a Library sequential pipeline request
        self.net.library_req_ids.clear();
        self.net.library_req_ids.insert(req_id);

        let (installs, targets) = self.compute_library_targets();
        log::info!(
            "Library targets count: {} (installed: {}, downloading: {})",
            targets.len(),
            installs.len(),
            self.downloads.len()
        );

        // Snapshot current results (prefer existing lib_result) so we don't re-fetch if a card is already filled
        let existing_map = self.build_existing_map_for_refresh();

        self.spawn_lib_pipeline_sequential_with_req(ctx, req_id, installs, targets, existing_map);

        // Do not scan listing pages at all in Library mode
        return;
    }

    /// Start background prefetch of Library data right after app start.
    pub(super) fn start_prefetch_library(&mut self, ctx: &egui::Context) {
        if self.net.lib_started {
            return;
        }
        self.net.lib_started = true;
        self.net.lib_error = None;
        self.net.lib_result = None;

        let (installs, targets) = self.compute_library_targets();

        // Snapshot current results so we don't re-fetch if a card is already filled
        let existing_map: std::collections::HashMap<u64, crate::parser::F95Thread> =
            helpers::build_existing_map(self.net.last_result.as_ref());

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
        if let Some(msg) = &self.net.last_result {
            for t in &msg.data {
                let thread_id = t.thread_id.clone();
                let id = t.thread_id.get();
                if self.images.covers.contains_key(&id)
                    || self.images.covers_loading.contains(&id)
                    || t.cover.is_empty()
                {
                    continue;
                }
                self.images.covers_loading.insert(id);
                // Prefer cover; if missing, fallback to first screenshot so the main tile isn't blank.
                let url_raw = if let Some(u) = helpers::get_cover_or_first_screen_url(t) {
                    u
                } else {
                    self.images.covers_loading.remove(&id);
                    continue;
                };
                let is_cover_choice = !t.cover.is_empty() && url_raw == t.cover;
                let url = crate::parser::normalize_url(&url_raw);
                let tx = self.images.cover_tx.clone();
                let ctx2 = ctx.clone();

                // Attempt to load from cache first (cover.png or screen_1.png if we fell back)
                let cache_path = {
                    let base = crate::app::settings::APP_SETTINGS
                        .read()
                        .unwrap()
                        .cache_dir
                        .clone();
                    let file = if is_cover_choice {
                        "cover.png".to_string()
                    } else {
                        "screen_1.png".to_string()
                    };
                    base.join(id.to_string()).join(file)
                };

                log::info!(
                    "cover schedule: id={} url={} cache_path={}",
                    id,
                    url,
                    cache_path.to_string_lossy()
                );
                super::rt().spawn(async move {
                    let mut served_from_cache = false;
                    if tokio::fs::metadata(&cache_path).await.is_ok() {
                        match tokio::task::spawn_blocking(
                            move || -> Result<(usize, usize, Vec<u8>), String> {
                                let bytes = std::fs::read(&cache_path)
                                    .map_err(|e| format!("read cache error: {}", e))?;
                                let img = image::load_from_memory(&bytes)
                                    .map_err(|e| format!("decode cache error: {}", e))?;
                                let rgba = img.to_rgba8();
                                let (w, h) = rgba.dimensions();
                                Ok((w as usize, h as usize, rgba.into_vec()))
                            },
                        )
                        .await
                        {
                            Ok(Ok((w, h, rgba))) => {
                                let _ = tx.send(CoverMsg::Ok {
                                    thread_id,
                                    w,
                                    h,
                                    rgba,
                                });
                                served_from_cache = true;
                            }
                            Ok(Err(e)) => {
                                log::warn!("cover cache decode failed: id={} err={}", id, e);
                            }
                            Err(e) => {
                                log::warn!("cover cache task join failed: id={} err={}", id, e);
                            }
                        }
                    }

                    if !served_from_cache {
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
                    }
                    ctx2.request_repaint();
                });
            }
        }
    }

    /// Poll incoming async messages and update state accordingly.
    pub(super) fn poll_incoming(&mut self, ctx: &egui::Context) {
        // Fetch results
        while let Ok((id, res)) = self.net.rx.try_recv() {
            let is_library_req = self.net.library_req_ids.contains(&id);
            // Ignore stale results unless it's an active Library sequential request
            if id != self.net.counter && !is_library_req {
                continue;
            }
            // In Library view, ignore non-Library listing results
            if self.filters.library_only && !is_library_req {
                continue;
            }
            // In Main view, ignore Library sequential results
            if !self.filters.library_only && is_library_req {
                continue;
            }
            self.net.loading = false;
            match res {
                Ok(msg) => {
                    self.net.last_error = None;
                    self.net.last_result = Some(msg);
                    self.schedule_cover_downloads(ctx);
                }
                Err(e) => {
                    self.net.last_result = None;
                    self.net.last_error = Some(e.to_string());
                }
            }
        }

        // Handle prefetched Library results
        while let Ok(res) = self.net.lib_rx.try_recv() {
            match res {
                Ok(msg) => {
                    self.net.lib_error = None;
                    self.net.lib_result = Some(msg.clone());
                    // If user is in Library view and waiting for data, show immediately
                    if self.filters.library_only {
                        self.net.last_result = Some(msg);
                        self.net.last_error = None;
                        self.net.loading = false;
                        self.schedule_cover_downloads(ctx);
                    }
                }
                Err(e) => {
                    self.net.lib_result = None;
                    self.net.lib_error = Some(e.to_string());
                    if self.filters.library_only {
                        self.net.last_error = Some(e.to_string());
                        self.net.loading = false;
                    }
                }
            }
            ctx.request_repaint();
        }

        // Images (covers/screens)
        while let Ok(msg) = self.images.cover_rx.try_recv() {
            match msg {
                CoverMsg::Ok {
                    thread_id,
                    w,
                    h,
                    rgba,
                } => {
                    let thread_id = thread_id.get();
                    // Opportunistic cache save (if enabled)
                    super::cache::maybe_save_cover_png(thread_id, w, h, rgba.clone());
                    let image = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                    let tex = ctx.load_texture(
                        format!("cover_{:?}", thread_id),
                        image,
                        egui::TextureOptions::default(),
                    );
                    self.images.covers.insert(thread_id, tex);
                    self.images.covers_loading.remove(&thread_id);
                    log::info!("cover ok: id={} size={}x{}", thread_id, w, h);
                }
                CoverMsg::Err { thread_id } => {
                    self.images.covers_loading.remove(&thread_id);
                }
                CoverMsg::ScreenOk {
                    thread_id,
                    idx,
                    w,
                    h,
                    rgba,
                } => {
                    // Opportunistic cache save (if enabled)
                    super::cache::maybe_save_screen_png(thread_id, idx, w, h, rgba.clone());
                    let image = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                    let tex = ctx.load_texture(
                        format!("screen_{}_{}", thread_id, idx),
                        image,
                        egui::TextureOptions::default(),
                    );
                    let entry = self
                        .images
                        .screens
                        .entry(thread_id)
                        .or_insert_with(|| Vec::new());
                    if entry.len() < idx + 1 {
                        entry.resize_with(idx + 1, || None);
                    }
                    entry[idx] = Some(tex);
                    self.images.screens_loading.remove(&(thread_id, idx));
                }
                CoverMsg::ScreenErr { thread_id, idx } => {
                    self.images.screens_loading.remove(&(thread_id, idx));
                }
            }
        }
    }
}
