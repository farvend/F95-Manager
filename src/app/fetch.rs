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

        let tx = self.tx.clone();
        let ctx2 = ctx.clone();

        // Library ignores user filters to ensure we can find all installed games
        let filters = crate::parser::F95Filters::default().with_category("games");

        // Capture installed thread_ids with existing folders + keep id->folder mapping for fallbacks
        let installs: Vec<(u64, std::path::PathBuf)> = {
            let st = crate::app::settings::APP_SETTINGS.read().unwrap();
            st.downloaded_games
                .iter()
                .filter(|g| crate::app::settings::game_folder_exists(&g.folder))
                .map(|g| (g.thread_id, g.folder.clone()))
                .collect()
        };
        // Include in-progress downloads as library targets as well
        let mut targets: Vec<u64> = installs.iter().map(|(id, _)| *id).collect();
        let downloading_ids: std::collections::HashSet<u64> =
            self.downloads.keys().copied().collect();
        for id in downloading_ids.iter() {
            if !targets.contains(id) {
                targets.push(*id);
            }
        }
        // Include persisted pending downloads (from previous sessions/errors)
        let pending_ids: Vec<u64> = {
            let st = crate::app::settings::APP_SETTINGS.read().unwrap();
            st.pending_downloads.clone()
        };
        for id in pending_ids {
            if !targets.contains(&id) {
                targets.push(id);
            }
        }
        log::info!(
            "Library targets count: {} (installed: {}, downloading: {})",
            targets.len(),
            installs.len(),
            downloading_ids.len()
        );
        // Direct mode: build Library strictly from installed thread pages (no list scanning).
        // 1) Reuse already loaded cards from the previous result to avoid redundant requests.
        // 2) Create placeholders for the rest.
        // 3) Enrich ONLY cards with missing data (cover/tags/screens) by fetching the thread page.
        {
            use std::collections::HashMap;

            // Snapshot current results so we don't re-fetch if a card is already filled
            let existing_map: HashMap<u64, crate::parser::F95Thread> = {
                if let Some(msg) = &self.last_result {
                    msg.data
                        .iter()
                        .map(|t| (t.thread_id.get(), t.clone()))
                        .collect()
                } else {
                    HashMap::new()
                }
            };

            let installs2 = installs.clone();
            let targets2 = targets.clone();
            let tx2 = self.tx.clone();
            let ctx3 = ctx.clone();
            let req_id2 = req_id;

            super::rt().spawn(async move {
                use std::collections::HashMap;

                if targets2.is_empty() {
                    let empty = crate::parser::F95Msg {
                        data: Vec::new(),
                        pagination: crate::parser::Pagination { page: 1, total: 1 },
                        count: 0,
                    };
                    let _ = tx2.send((req_id2, Ok(empty)));
                    ctx3.request_repaint();
                    return;
                }

                // Initial list from cache (if any) + placeholders
                let install_map: HashMap<u64, std::path::PathBuf> =
                    installs2.iter().cloned().collect();

                let mut all_found: Vec<crate::parser::F95Thread> = Vec::new();
                for id in targets2.iter() {
                    if let Some(ex) = existing_map.get(id) {
                        all_found.push(ex.clone());
                    } else {
                        let title = install_map
                            .get(id)
                            .and_then(|folder| folder.file_name().and_then(|s| s.to_str()))
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| format!("Thread #{}", id));
                        all_found.push(crate::parser::F95Thread {
                            thread_id: ThreadId(*id),
                            title,
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
                        });
                    }
                }

                // Send initial snapshot
                log::info!("Direct library initial: items={}", all_found.len());
                let mut result = crate::parser::F95Msg {
                    data: all_found.clone(),
                    pagination: crate::parser::Pagination { page: 1, total: 1 },
                    count: all_found.len() as u64,
                };
                let _ = tx2.send((req_id2, Ok(result)));
                ctx3.request_repaint();

                // Enrich only cards with missing data, strictly from the thread page
                let to_enrich: Vec<u64> = all_found
                    .iter()
                    .filter(|t| t.cover.is_empty() || t.tags.is_empty() || t.screens.is_empty())
                    .map(|t| t.thread_id.get())
                    .collect();

                for id in to_enrich {
                    log::info!("Direct enrich thread {}", id);
                    if let Some(mut meta) = crate::parser::game_info::thread_meta::fetch_thread_meta(id).await {
                        // Snapshot before moving fields
                        let has_title = meta.title.as_ref().is_some();
                        let has_cover = meta.cover.as_ref().is_some();
                        let sc_len = meta.screens.len();
                        let tg_len = meta.tag_ids.len();

                        if let Some(th) = all_found.iter_mut().find(|t| t.thread_id.get() == id) {
                            // Title: replace folder-name fallback if needed
                            if let Some(tt) = meta.title.take() {
                                let looks_like_folder = install_map
                                    .get(&id)
                                    .and_then(|p| p.file_name().and_then(|s| s.to_str()))
                                    .map(|n| n == th.title)
                                    .unwrap_or(false);
                                if th.title.is_empty() || looks_like_folder {
                                    th.title = tt;
                                }
                            }
                            // Media/tags only if missing
                            if let Some(c) = meta.cover.take() {
                                if th.cover.is_empty() {
                                    th.cover = c;
                                }
                            }
                            //if sc_len > 0 && th.screens.is_empty() {
                                th.screens = meta.screens;
                                
                            //}
                            if tg_len > 0 && th.tags.is_empty() {
                                th.tags = meta.tag_ids;
                            }
                        }

                        log::info!("Direct meta fetched for {}: title={} cover={} screens={} tags={}", id, has_title, has_cover, sc_len, tg_len);

                        // Push incremental update
                        result = crate::parser::F95Msg {
                            data: all_found.clone(),
                            pagination: crate::parser::Pagination { page: 1, total: 1 },
                            count: all_found.len() as u64,
                        };
                        let _ = tx2.send((req_id2, Ok(result)));
                        ctx3.request_repaint();
                    }
                }
            });

            // Do not scan listing pages at all in Library mode
            return;
        }
    }

    /// Start background prefetch of Library data right after app start.
    pub(super) fn start_prefetch_library(&mut self, ctx: &egui::Context) {
        if self.lib_started {
            return;
        }
        self.lib_started = true;
        self.lib_error = None;
        self.lib_result = None;

        let tx = self.lib_tx.clone();
        let ctx2 = ctx.clone();

        // Capture installed thread_ids with existing folders + keep id->folder mapping for fallbacks
        let installs: Vec<(u64, std::path::PathBuf)> = {
            let st = crate::app::settings::APP_SETTINGS.read().unwrap();
            st.downloaded_games
                .iter()
                .filter(|g| crate::app::settings::game_folder_exists(&g.folder))
                .map(|g| (g.thread_id, g.folder.clone()))
                .collect()
        };
        // Include current in-progress downloads as library targets
        let mut targets: Vec<u64> = installs.iter().map(|(id, _)| *id).collect();
        let downloading_ids: std::collections::HashSet<u64> =
            self.downloads.keys().copied().collect();
        for id in downloading_ids.iter() {
            if !targets.contains(id) {
                targets.push(*id);
            }
        }
        // Include persisted pending downloads (from previous sessions/errors)
        let pending_ids: Vec<u64> = {
            let st = crate::app::settings::APP_SETTINGS.read().unwrap();
            st.pending_downloads.clone()
        };
        for id in pending_ids {
            if !targets.contains(&id) {
                targets.push(id);
            }
        }

        // Snapshot current results so we don't re-fetch if a card is already filled
        let existing_map: std::collections::HashMap<u64, crate::parser::F95Thread> = {
            if let Some(msg) = &self.last_result {
                msg.data
                    .iter()
                    .map(|t| (t.thread_id.get(), t.clone()))
                    .collect()
            } else {
                std::collections::HashMap::new()
            }
        };

        super::rt().spawn(async move {
            use std::collections::HashMap;
            use crate::parser::game_info::ThreadId;

            if targets.is_empty() {
                let empty = crate::parser::F95Msg {
                    data: Vec::new(),
                    pagination: crate::parser::Pagination { page: 1, total: 1 },
                    count: 0,
                };
                let _ = tx.send(Ok(empty));
                ctx2.request_repaint();
                return;
            }

            // Initial list from cache (if any) + placeholders
            let install_map: HashMap<u64, std::path::PathBuf> =
                installs.iter().cloned().collect();

            let mut all_found: Vec<crate::parser::F95Thread> = Vec::new();
            for id in targets.iter() {
                if let Some(ex) = existing_map.get(id) {
                    all_found.push(ex.clone());
                } else {
                    let title = install_map
                        .get(id)
                        .and_then(|folder| folder.file_name().and_then(|s| s.to_str()))
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("Thread #{}", id));
                    all_found.push(crate::parser::F95Thread {
                        thread_id: ThreadId(*id),
                        title,
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
                    });
                }
            }

            // Send initial snapshot
            let mut result = crate::parser::F95Msg {
                data: all_found.clone(),
                pagination: crate::parser::Pagination { page: 1, total: 1 },
                count: all_found.len() as u64,
            };
            let _ = tx.send(Ok(result.clone()));
            ctx2.request_repaint();

            // Enrich only cards with missing data, strictly from the thread page (concurrently)
            let to_enrich: Vec<u64> = all_found
                .iter()
                .filter(|t| t.cover.is_empty() || t.tags.is_empty() || t.screens.is_empty())
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
                if let Ok((id, Some(mut meta))) = joined {
                    // Snapshot before moving fields
                    let has_title = meta.title.as_ref().is_some();
                    let has_cover = meta.cover.as_ref().is_some();
                    let sc_len = meta.screens.len();
                    let tg_len = meta.tag_ids.len();

                    if let Some(th) = all_found.iter_mut().find(|t| t.thread_id.get() == id) {
                        // Title: replace folder-name fallback if needed
                        if let Some(tt) = meta.title.take() {
                            let looks_like_folder = install_map
                                .get(&id)
                                .and_then(|p| p.file_name().and_then(|s| s.to_str()))
                                .map(|n| n == th.title)
                                .unwrap_or(false);
                            if th.title.is_empty() || looks_like_folder {
                                th.title = tt;
                            }
                        }
                        // Media/tags only if missing
                        if let Some(c) = meta.cover.take() {
                            if th.cover.is_empty() {
                                th.cover = c;
                            }
                        }
                        th.screens = meta.screens;
                        if tg_len > 0 && th.tags.is_empty() {
                            th.tags = meta.tag_ids;
                        }
                    }

                    log::info!(
                        "Prefetch meta for {}: title={} cover={} screens={} tags={}",
                        id, has_title, has_cover, sc_len, tg_len
                    );

                    // Push incremental update
                    result = crate::parser::F95Msg {
                        data: all_found.clone(),
                        pagination: crate::parser::Pagination { page: 1, total: 1 },
                        count: all_found.len() as u64,
                    };
                    let _ = tx.send(Ok(result.clone()));
                    ctx2.request_repaint();
                }
            }
        });
    }

    /// Refresh background Library data snapshot, including current in-progress downloads.
    pub(super) fn refresh_prefetch_library(&mut self, ctx: &egui::Context) {
        // Non-destructive refresh: do not flip lib_started or clear current lib_result.
        let tx = self.lib_tx.clone();
        let ctx2 = ctx.clone();

        // Capture installed thread_ids with existing folders + keep id->folder mapping for fallbacks
        let installs: Vec<(u64, std::path::PathBuf)> = {
            let st = crate::app::settings::APP_SETTINGS.read().unwrap();
            st.downloaded_games
                .iter()
                .filter(|g| crate::app::settings::game_folder_exists(&g.folder))
                .map(|g| (g.thread_id, g.folder.clone()))
                .collect()
        };
        // Include current in-progress downloads as library targets too
        let mut targets: Vec<u64> = installs.iter().map(|(id, _)| *id).collect();
        let downloading_ids: std::collections::HashSet<u64> =
            self.downloads.keys().copied().collect();
        for id in downloading_ids.iter() {
            if !targets.contains(id) {
                targets.push(*id);
            }
        }
        // Include persisted pending downloads (from previous sessions/errors)
        let pending_ids: Vec<u64> = {
            let st = crate::app::settings::APP_SETTINGS.read().unwrap();
            st.pending_downloads.clone()
        };
        for id in pending_ids {
            if !targets.contains(&id) {
                targets.push(id);
            }
        }

        // Snapshot current results (prefer existing lib_result) so we don't re-fetch if a card is already filled
        let existing_map: std::collections::HashMap<u64, crate::parser::F95Thread> = {
            if let Some(msg) = &self.lib_result {
                msg.data
                    .iter()
                    .map(|t| (t.thread_id.get(), t.clone()))
                    .collect()
            } else if let Some(msg) = &self.last_result {
                msg.data
                    .iter()
                    .map(|t| (t.thread_id.get(), t.clone()))
                    .collect()
            } else {
                std::collections::HashMap::new()
            }
        };

        super::rt().spawn(async move {
            use std::collections::HashMap;
            use crate::parser::game_info::ThreadId;

            if targets.is_empty() {
                let empty = crate::parser::F95Msg {
                    data: Vec::new(),
                    pagination: crate::parser::Pagination { page: 1, total: 1 },
                    count: 0,
                };
                let _ = tx.send(Ok(empty));
                ctx2.request_repaint();
                return;
            }

            // Initial list from cache (if any) + placeholders
            let install_map: HashMap<u64, std::path::PathBuf> =
                installs.iter().cloned().collect();

            let mut all_found: Vec<crate::parser::F95Thread> = Vec::new();
            for id in targets.iter() {
                if let Some(ex) = existing_map.get(id) {
                    all_found.push(ex.clone());
                } else {
                    let title = install_map
                        .get(id)
                        .and_then(|folder| folder.file_name().and_then(|s| s.to_str()))
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("Thread #{}", id));
                    all_found.push(crate::parser::F95Thread {
                        thread_id: ThreadId(*id),
                        title,
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
                    });
                }
            }

            // Send initial snapshot
            let mut result = crate::parser::F95Msg {
                data: all_found.clone(),
                pagination: crate::parser::Pagination { page: 1, total: 1 },
                count: all_found.len() as u64,
            };
            let _ = tx.send(Ok(result.clone()));
            ctx2.request_repaint();

            // Enrich only cards with missing data, strictly from the thread page (concurrently)
            let to_enrich: Vec<u64> = all_found
                .iter()
                .filter(|t| t.cover.is_empty() || t.tags.is_empty() || t.screens.is_empty())
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
                if let Ok((id, Some(mut meta))) = joined {
                    if let Some(th) = all_found.iter_mut().find(|t| t.thread_id.get() == id) {
                        // Title: replace folder-name fallback if needed
                        if let Some(tt) = meta.title.take() {
                            let looks_like_folder = install_map
                                .get(&id)
                                .and_then(|p| p.file_name().and_then(|s| s.to_str()))
                                .map(|n| n == th.title)
                                .unwrap_or(false);
                            if th.title.is_empty() || looks_like_folder {
                                th.title = tt;
                            }
                        }
                        // Media/tags only if missing
                        if let Some(c) = meta.cover.take() {
                            if th.cover.is_empty() {
                                th.cover = c;
                            }
                        }
                        th.screens = meta.screens;
                        if !th.tags.is_empty() {
                            // keep
                        } else {
                            th.tags = meta.tag_ids;
                        }
                    }

                    // Push incremental update
                    result = crate::parser::F95Msg {
                        data: all_found.clone(),
                        pagination: crate::parser::Pagination { page: 1, total: 1 },
                        count: all_found.len() as u64,
                    };
                    let _ = tx.send(Ok(result.clone()));
                    ctx2.request_repaint();
                }
            }
        });
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
