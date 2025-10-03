// Логика приложения вынесена из main.rs, чтобы убрать глубокую вложенность в конце main.
// Рефакторинг: крупные группы полей вынесены в отдельные структуры в app/state.rs.

use eframe::egui::RichText;
use eframe::{egui, App};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::types::*;
use crate::views::cards::CARD_WIDTH;
use crate::views::filters::draw_filters_panel;

mod grid;
pub mod settings;
pub mod config;
mod logs_ui;
mod about_ui;
mod errors_ui;
mod update_ui;

// Вынесено: tokio runtime и вся логика получения данных
mod runtime;
mod fetch;
mod downloads;
mod cache;
mod state;

pub use runtime::rt;
pub use runtime::RUNTIME;
pub use fetch::CoverMsg;
use downloads::DownloadState;
use state::{AuthState, FiltersState, ImagesState, NetState, Screen};

const DOWNLOAD_WEIGHT: f32 = 0.75;
const UNZIP_WEIGHT: f32 = 1.0 - DOWNLOAD_WEIGHT;

pub struct NoLagApp {
    // Пагинация
    page: u32,

    // Новый, сгруппированный стейт
    filters: FiltersState,
    net: NetState,
    images: ImagesState,
    auth: AuthState,

    // Загрузки оставлены здесь (выполняют побочные эффекты в UI/Library)
    downloads: HashMap<u64, DownloadState>,
}

impl Default for NoLagApp {
    fn default() -> Self {
        // Ensure app_config.json is loaded before deciding which screen to show
        crate::app::config::load_config_from_disk();
        let need_auth = {
            let cfg = crate::app::config::APP_CONFIG.read().unwrap();
            cfg.cookies
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
        };
        let screen = if need_auth { Screen::AuthLogin } else { Screen::Main };

        Self {
            page: 1,
            filters: FiltersState::default(),
            net: NetState::new(),
            images: ImagesState::new(),
            auth: AuthState::new(screen),
            downloads: HashMap::new(),
        }
    }
}

impl App for NoLagApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Any new logs? ensure we repaint to keep button/window fresh
        if crate::logger::take_new_flag() {
            ctx.request_repaint();
        }

        // Handle async login results
        while let Ok(res) = self.auth.auth_rx.try_recv() {
            self.auth.login_in_progress = false;
            match res {
                Ok(()) => {
                    self.auth.login_error = None;
                    self.auth.screen = Screen::Main;
                    // Trigger initial fetch now that main UI is enabled
                    self.page = 1;
                    self.filters.search_due_at = None;
                    self.net.loading = false;
                    self.start_fetch(ctx);
                }
                Err(e) => {
                    self.auth.login_error = Some(e);
                }
            }
            ctx.request_repaint();
        }

        // Authorization gating: if there is no app_config cookies, show auth flow and skip main UI
        if self.auth.screen != Screen::Main {
            egui::CentralPanel::default().show(ctx, |ui| {
                match self.auth.screen {
                    Screen::AuthLogin => {
                        ui.add_space(24.0);
                        ui.vertical_centered(|ui| {
                            ui.heading(crate::localization::translate("auth-login-title"));
                        });
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label(crate::localization::translate("auth-username"));
                            ui.text_edit_singleline(&mut self.auth.login_username);
                        });
                        ui.horizontal(|ui| {
                            ui.label(crate::localization::translate("auth-password"));
                            let te = egui::TextEdit::singleline(&mut self.auth.login_password).password(true);
                            ui.add(te);
                        });
                        if let Some(err) = &self.auth.login_error {
                            ui.colored_label(egui::Color32::RED, err);
                        }
                        ui.add_space(8.0);
                        let login_clicked = ui.add_enabled(!self.auth.login_in_progress, egui::Button::new(crate::localization::translate("auth-login-button"))).clicked();
                        if self.auth.login_in_progress {
                            ui.add_space(4.0);
                            ui.add(egui::Spinner::new());
                            ui.label(crate::localization::translate("auth-authorizing"));
                        }
                        if login_clicked {
                            if self.auth.login_username.trim().is_empty() || self.auth.login_password.is_empty() {
                                self.auth.login_error = Some(crate::localization::translate("auth-please-enter-credentials"));
                            } else {
                                self.auth.login_error = None;
                                self.auth.login_in_progress = true;
                                let u = self.auth.login_username.clone();
                                let p = self.auth.login_password.clone();
                                let tx = self.auth.auth_tx.clone();
                                let ctx2 = ctx.clone();
                                crate::app::rt().spawn(async move {
                                    let res = crate::app::config::login_and_store(u, p).await;
                                    let _ = tx.send(res);
                                    ctx2.request_repaint();
                                });
                            }
                        }

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(8.0);
                        ui.label(crate::localization::translate("auth-or-paste-cookies"));
                        let te2 = egui::TextEdit::multiline(&mut self.auth.login_cookies_input).desired_rows(3);
                        ui.add(te2);
                        ui.add_space(4.0);
                        let use_clicked = ui.add_enabled(!self.auth.login_in_progress, egui::Button::new(crate::localization::translate("auth-use-cookies"))).clicked();
                        if use_clicked {
                            let c = self.auth.login_cookies_input.trim();
                            if c.is_empty() {
                                self.auth.login_error = Some(crate::localization::translate("auth-please-paste-cookies"));
                            } else {
                                {
                                    let mut cfg = crate::app::config::APP_CONFIG.write().unwrap();
                                    cfg.cookies = Some(c.to_string());
                                    if !self.auth.login_username.trim().is_empty() {
                                        cfg.username = Some(self.auth.login_username.clone());
                                    }
                                }
                                crate::app::config::save_config_to_disk();
                                self.auth.login_error = None;
                                self.auth.screen = Screen::Main;
                                self.page = 1;
                                self.filters.search_due_at = None;
                                self.net.loading = false;
                                self.start_fetch(ctx);
                            }
                        }
                        ui.add_space(8.0);
                        ui.label(RichText::new(crate::localization::translate("auth-info-needed")).small());
                    }
                    Screen::Main => {}
                }
            });
            // Logs, Errors and Settings viewports remain accessible
            logs_ui::draw_logs_viewport(ctx);
            let bottom_offset = update_ui::draw_update_notice(ctx);
            errors_ui::draw_errors_button(ctx, bottom_offset);
            errors_ui::draw_errors_viewport(ctx);
            about_ui::draw_about_viewport(ctx);
            settings::draw_settings_viewport(ctx);
            return;
        }

        // Обработка входящих сообщений (список тредов, обложки/скриншоты) вынесена в модуль fetch
        self.poll_incoming(ctx);

        // Poll active downloads and update progress
        self.poll_downloads(ctx);

        // Ensure covers for currently displayed items are scheduled (idempotent)
        self.schedule_cover_downloads(ctx);

        // Первый автозапуск загрузки
        // Не перезапускать автоматически при наличии ошибки (например, 429), чтобы не было бесконечного цикла запросов
        if self.net.last_result.is_none() && self.net.last_error.is_none() && !self.net.loading {
            if self.filters.library_only {
                // Если приложение стартует в режиме Library — сразу запускаем параллельную подзагрузку
                self.start_prefetch_library(ctx);
            } else {
                // Стартуем обычный список
                self.start_fetch(ctx);
                // И параллельно сразу же подгружаем библиотеку в фоне
                if !self.net.lib_started {
                    self.start_prefetch_library(ctx);
                }
            }
        } else {
            // Гарантируем, что фоновая подзагрузка библиотеки запущена один раз
            if !self.net.lib_started {
                self.start_prefetch_library(ctx);
            }
        }

        // Правая панель — фильтры
        let prev_query = self.filters.query.clone();
        let (apply, open_settings, open_logs, open_about) = draw_filters_panel(
            ctx,
            &mut self.filters.sort,
            &mut self.filters.date_limit,
            &mut self.filters.include_logic,
            &mut self.filters.include_tags,
            &mut self.filters.exclude_mode,
            &mut self.filters.exclude_tags,
            &mut self.filters.include_prefixes,
            &mut self.filters.exclude_prefixes,
            &mut self.filters.search_mode,
            &mut self.filters.query,
            &mut self.filters.library_only,
        );
        if apply {
            // Немедленно перезапустить поиск при изменении фильтров (кроме текста)
            self.page = 1;
            self.filters.search_due_at = None;
            if self.filters.library_only {
                self.start_fetch_library(ctx);
            } else {
                self.start_fetch(ctx);
            }
        }
        // Debounce text query changes: run search 0.3s after last edit
        let query_changed = self.filters.query != prev_query;
        if query_changed {
            if apply {
                // Filters changed this frame and already triggered immediate fetch; skip debounce
                self.filters.search_due_at = None;
            } else {
                self.page = 1;
                self.filters.search_due_at = Some(Instant::now() + Duration::from_millis(300));
                // Wake up after the debounce interval to fire the search
                ctx.request_repaint_after(Duration::from_millis(300));
            }
        }
        if open_settings {
            settings::open_settings();
            ctx.request_repaint();
        }
        if open_logs {
            logs_ui::open_logs();
            ctx.request_repaint();
        }
        if open_about {
            about_ui::open_about();
            ctx.request_repaint();
        }
        // When filters changed this frame, auto-save selected tags if enabled in settings
        if apply {
            let do_autosave = { settings::APP_SETTINGS.read().unwrap().autosave_selected_tags };
            if do_autosave {
                let mut need_save = false;
                {
                    let mut st = settings::APP_SETTINGS.write().unwrap();
                    if st.startup_tags != self.filters.include_tags {
                        st.startup_tags = self.filters.include_tags.clone();
                        need_save = true;
                    }
                    if st.startup_exclude_tags != self.filters.exclude_tags {
                        st.startup_exclude_tags = self.filters.exclude_tags.clone();
                        need_save = true;
                    }
                }
                if need_save {
                    settings::save_settings_to_disk();
                }
            }
        }
        // Trigger new fetch when Library mode toggles
        if self.filters.last_library_only != self.filters.library_only {
            self.filters.last_library_only = self.filters.library_only;
            if self.filters.library_only {
                // Если фоновые данные уже есть — мгновенно показываем их
                if let Some(msg) = &self.net.lib_result {
                    self.net.last_result = Some(msg.clone());
                    self.net.last_error = None;
                    self.net.loading = false;
                    // Immediately schedule cover downloads for the freshly shown Library data
                    self.schedule_cover_downloads(ctx);
                    ctx.request_repaint();
                } else {
                    // Обеспечим запуск фоновой загрузки и покажем спиннер
                    if !self.net.lib_started {
                        self.start_prefetch_library(ctx);
                    }
                    self.net.last_result = None;
                    self.net.last_error = None;
                    self.net.loading = true;
                }
            } else {
                self.start_fetch(ctx);
            }
        }

        // Run debounced query fetch if deadline passed
        if let Some(due) = self.filters.search_due_at {
            if Instant::now() >= due {
                self.filters.search_due_at = None;
                if self.filters.library_only {
                    self.start_fetch_library(ctx);
                } else {
                    self.start_fetch(ctx);
                }
            }
        }

        // Центральная панель — сетка карточек
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let avail_w = ui.available_width().floor();
                    let card_w = CARD_WIDTH;
                    let gap = 16.0;

                    let mut cols = ((avail_w + gap) / (card_w + gap)).floor() as usize;
                    if cols == 0 {
                        cols = 1;
                    }
                    let row_w = (cols as f32) * card_w + ((cols - 1) as f32) * gap;
                    let left_pad = ((avail_w - row_w) / 2.0).max(0.0);

                    if let Some(err) = &self.net.last_error {
                        ui.vertical_centered(|ui| {
                            ui.colored_label(egui::Color32::RED, crate::localization::translate_with("error-prefix", &[("err", err.clone())]));
                        });
                    } else if self.net.loading && self.net.last_result.is_none() {
                        ui.add_space(24.0);
                        ui.vertical_centered(|ui| {
                            ui.add(egui::Spinner::new());
                            ui.label(crate::localization::translate("loading"));
                        });
                    } else if self.net.last_result.is_some() {
                        // Clone data so we don't hold an immutable borrow of `self` across a call
                        // that needs `&mut self` (draw_threads_grid).
                        let data_cloned = {
                            let msg = self.net.last_result.as_ref().unwrap();
                            msg.data.clone()
                        };
                        // Build a set of hidden thread_ids and filter them out from rendering
                        let hidden: std::collections::HashSet<u64> = {
                            let st = settings::APP_SETTINGS.read().unwrap();
                            st.hidden_threads.iter().copied().collect()
                        };

                        // When Library mode is ON, show downloaded AND in-progress games; always ignore hidden ones
                        let mut display_data: Vec<crate::parser::F95Thread> = if self.filters.library_only {
                            // Persisted completed downloads
                            let downloaded_ids: std::collections::HashSet<u64> = {
                                let st = settings::APP_SETTINGS.read().unwrap();
                                st.downloaded_games
                                    .iter()
                                    .filter(|g| settings::game_folder_exists(&g.folder))
                                    .map(|g| g.thread_id)
                                    .collect()
                            };
                            // In-progress downloads (runtime-only)
                            let downloading_ids: std::collections::HashSet<u64> =
                                self.downloads.keys().copied().collect();
                            // Persisted pending/incomplete downloads (from previous sessions or failed attempts)
                            let pending_ids: std::collections::HashSet<u64> = {
                                let st = settings::APP_SETTINGS.read().unwrap();
                                st.pending_downloads.iter().copied().collect()
                            };
                            let in_library = |id: u64| {
                                downloaded_ids.contains(&id)
                                    || downloading_ids.contains(&id)
                                    || pending_ids.contains(&id)
                            };

                            data_cloned
                                .into_iter()
                                .filter(|t| in_library(t.thread_id.get()))
                                .filter(|t| !hidden.contains(&t.thread_id.get()))
                                .collect()
                        } else {
                            data_cloned
                                .into_iter()
                                .filter(|t| !hidden.contains(&t.thread_id.get()))
                                .collect()
                        };

                        // Apply client-side filters and sorting in Library mode
                        if self.filters.library_only {
                            // Text query (Title or Creator)
                            let q = self.filters.query.to_lowercase();
                            let use_query = !q.trim().is_empty();

                            display_data.retain(|t| {
                                // Query
                                if use_query {
                                    let hay = t.title.to_lowercase();
                                    if !hay.contains(&q) {
                                        return false;
                                    }
                                }

                                // Include tags with OR/AND logic
                                if !self.filters.include_tags.is_empty() {
                                    let has = |id: &u32| t.tags.contains(id);
                                    let ok = match self.filters.include_logic {
                                        TagLogic::And => self.filters.include_tags.iter().all(has),
                                        TagLogic::Or => self.filters.include_tags.iter().any(has),
                                    };
                                    if !ok {
                                        return false;
                                    }
                                }

                                // Exclude tags
                                if !self.filters.exclude_tags.is_empty()
                                    && self.filters.exclude_tags.iter().any(|id| t.tags.contains(id))
                                {
                                    return false;
                                }
                                true
                            });
                        }

                        self.draw_threads_grid(ui, ctx, &display_data, cols, left_pad, gap, card_w);

                        // Bottom controls: pagination in normal mode, summary in Library mode
                        ui.add_space(8.0);
                        ui.vertical_centered(|ui| {
                            if self.filters.library_only {
                                let installed_count = {
                                    let st = settings::APP_SETTINGS.read().unwrap();
                                    st.downloaded_games
                                        .iter()
                                        .filter(|g| settings::game_folder_exists(&g.folder))
                                        .count()
                                };
                                ui.label(crate::localization::translate_with("library-summary", &[("shown", display_data.len().to_string()), ("installed", installed_count.to_string())]));
                            } else {
                                let (cur, total) = {
                                    let msg = self.net.last_result.as_ref().unwrap();
                                    (msg.pagination.page, msg.pagination.total)
                                };
                                ui.horizontal(|ui| {
                                    let prev_enabled = cur > 1;
                                    if ui.add_enabled(prev_enabled, egui::Button::new("◀")).clicked() {
                                        self.page = cur.saturating_sub(1);
                                        self.start_fetch(ctx);
                                    }
                                    ui.label(crate::localization::translate_with("pagination-page", &[("cur", cur.to_string()), ("total", total.to_string())]));
                                    let next_enabled = cur < total;
                                    if ui.add_enabled(next_enabled, egui::Button::new("▶")).clicked() {
                                        self.page = cur + 1;
                                        self.start_fetch(ctx);
                                    }
                                });
                            }
                        });
                    }
                });
        });

        // Floating Update + Errors overlay
        let bottom_offset = update_ui::draw_update_notice(ctx);
        errors_ui::draw_errors_button(ctx, bottom_offset);
        errors_ui::draw_errors_viewport(ctx);

        // Logs window (separate OS viewport)
        logs_ui::draw_logs_viewport(ctx);

        // About window (separate OS viewport)
        about_ui::draw_about_viewport(ctx);

        // Settings window (separate OS viewport)
        settings::draw_settings_viewport(ctx);
    }
}
