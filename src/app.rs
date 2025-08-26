// Логика приложения вынесена из main.rs, чтобы убрать глубокую вложенность в конце main.
// Здесь находится состояние NoLagApp и отрисовка UI. Получение данных и runtime вынесены в подмодули.

use eframe::egui::RichText;
use eframe::{egui, App};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
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

// Вынесено: tokio runtime и вся логика получения данных
mod runtime;
mod fetch;
mod downloads;
mod cache;
pub use runtime::rt;
pub use runtime::RUNTIME;
pub use fetch::CoverMsg;
use downloads::DownloadState;

const DOWNLOAD_WEIGHT: f32 = 0.75;
const UNZIP_WEIGHT: f32 = 1.0 - DOWNLOAD_WEIGHT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    AuthLogin,
    Main,
}


pub struct NoLagApp {
    counter: u64,
    page: u32,
    sort: Sorting,
    date_limit: DateLimit,
    include_logic: TagLogic,
    include_tags: Vec<u32>,
    exclude_tags: Vec<u32>,
    include_prefixes: Vec<u32>,
    exclude_prefixes: Vec<u32>,
    exclude_mode: Vec<u32>,
    search_mode: SearchMode,
    query: String,
    library_only: bool,
    last_library_only: bool,
    search_due_at: Option<Instant>,
    // Async fetch wiring
    loading: bool,
    tx: mpsc::Sender<(u64, Result<crate::parser::F95Msg, crate::parser::F95Error>)>,
    rx: mpsc::Receiver<(u64, Result<crate::parser::F95Msg, crate::parser::F95Error>)>,
    last_result: Option<crate::parser::F95Msg>,
    last_error: Option<String>,
    // Covers loading and cache
    covers: HashMap<u64, egui::TextureHandle>,
    covers_loading: HashSet<u64>,
    // Screenshots loading and cache
    screens: HashMap<u64, Vec<Option<egui::TextureHandle>>>,
    screens_loading: HashSet<(u64, usize)>,
    cover_tx: mpsc::Sender<CoverMsg>,
    cover_rx: mpsc::Receiver<CoverMsg>,
    downloads: HashMap<u64, DownloadState>,
    // Background Library prefetch state
    lib_started: bool,
    lib_result: Option<crate::parser::F95Msg>,
    lib_error: Option<String>,
    lib_tx: mpsc::Sender<Result<crate::parser::F95Msg, crate::parser::F95Error>>,
    lib_rx: mpsc::Receiver<Result<crate::parser::F95Msg, crate::parser::F95Error>>,
    // Authorization/login UI state
    screen: Screen,
    login_username: String,
    login_password: String,
    login_cookies_input: String,
    login_error: Option<String>,
    login_in_progress: bool,
    auth_tx: mpsc::Sender<Result<(), String>>,
    auth_rx: mpsc::Receiver<Result<(), String>>,
}

impl Default for NoLagApp {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        let (cover_tx, cover_rx) = mpsc::channel();
        let (lib_tx, lib_rx) = mpsc::channel();
        let (auth_tx, auth_rx) = mpsc::channel();
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
            counter: 0,
            page: 1,
            sort: Sorting::default(),
            date_limit: DateLimit::default(),
            include_logic: TagLogic::default(),
            include_tags: Vec::new(),
            exclude_tags: Vec::new(),
            include_prefixes: Vec::new(),
            exclude_prefixes: Vec::new(),
            exclude_mode: Vec::new(),
            search_mode: SearchMode::default(),
            query: String::new(),
            library_only: false,
            last_library_only: false,
            search_due_at: None,
            loading: false,
            tx,
            rx,
            last_result: None,
            last_error: None,
            covers: HashMap::new(),
            covers_loading: HashSet::new(),
            screens: HashMap::new(),
            screens_loading: HashSet::new(),
            cover_tx,
            cover_rx,
            downloads: HashMap::new(),
            lib_started: false,
            lib_result: None,
            lib_error: None,
            lib_tx,
            lib_rx,
            screen,
            login_username: String::new(),
            login_password: String::new(),
            login_cookies_input: String::new(),
            login_error: None,
            login_in_progress: false,
            auth_tx,
            auth_rx,
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
        while let Ok(res) = self.auth_rx.try_recv() {
            self.login_in_progress = false;
            match res {
                Ok(()) => {
                    self.login_error = None;
                    self.screen = Screen::Main;
                    // Trigger initial fetch now that main UI is enabled
                    self.page = 1;
                    self.search_due_at = None;
                    self.loading = false;
                    self.start_fetch(ctx);
                }
                Err(e) => {
                    self.login_error = Some(e);
                }
            }
            ctx.request_repaint();
        }

        // Authorization gating: if there is no app_config cookies, show auth flow and skip main UI
        if self.screen != Screen::Main {
            egui::CentralPanel::default().show(ctx, |ui| {
                match self.screen {
                    Screen::AuthLogin => {
                        ui.add_space(24.0);
                        ui.vertical_centered(|ui| {
                            ui.heading("Login");
                        });
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label("Username:");
                            ui.text_edit_singleline(&mut self.login_username);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Password:");
                            let te = egui::TextEdit::singleline(&mut self.login_password).password(true);
                            ui.add(te);
                        });
                        if let Some(err) = &self.login_error {
                            ui.colored_label(egui::Color32::RED, err);
                        }
                        ui.add_space(8.0);
                        let login_clicked = ui.add_enabled(!self.login_in_progress, egui::Button::new("Login")).clicked();
                        if self.login_in_progress {
                            ui.add_space(4.0);
                            ui.add(egui::Spinner::new());
                            ui.label("Authorizing...");
                        }
                        if login_clicked {
                            if self.login_username.trim().is_empty() || self.login_password.is_empty() {
                                self.login_error = Some("Please enter username and password".to_string());
                            } else {
                                self.login_error = None;
                                self.login_in_progress = true;
                                let u = self.login_username.clone();
                                let p = self.login_password.clone();
                                let tx = self.auth_tx.clone();
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
                        ui.label("Or paste cookies (Cookie header):");
                        let te2 = egui::TextEdit::multiline(&mut self.login_cookies_input).desired_rows(3);
                        ui.add(te2);
                        ui.add_space(4.0);
                        let use_clicked = ui.add_enabled(!self.login_in_progress, egui::Button::new("Use cookies")).clicked();
                        if use_clicked {
                            let c = self.login_cookies_input.trim();
                            if c.is_empty() {
                                self.login_error = Some("Please paste cookies".to_string());
                            } else {
                                {
                                    let mut cfg = crate::app::config::APP_CONFIG.write().unwrap();
                                    cfg.cookies = Some(c.to_string());
                                    if !self.login_username.trim().is_empty() {
                                        cfg.username = Some(self.login_username.clone());
                                    }
                                }
                                crate::app::config::save_config_to_disk();
                                self.login_error = None;
                                self.screen = Screen::Main;
                                self.page = 1;
                                self.search_due_at = None;
                                self.loading = false;
                                self.start_fetch(ctx);
                            }
                        }
                        ui.add_space(8.0);
                        ui.label(RichText::new("This information is needed to get download links from games' pages").small());
                    }
                    Screen::Main => {}
                }
            });
            // Logs, Errors and Settings viewports remain accessible
            logs_ui::draw_logs_viewport(ctx);
            errors_ui::draw_errors_button(ctx);
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
        if self.last_result.is_none() && !self.loading {
            if self.library_only {
                // Если приложение стартует в режиме Library — сразу запускаем параллельную подзагрузку
                self.start_prefetch_library(ctx);
            } else {
                // Стартуем обычный список
                self.start_fetch(ctx);
                // И параллельно сразу же подгружаем библиотеку в фоне
                if !self.lib_started {
                    self.start_prefetch_library(ctx);
                }
            }
        } else {
            // Гарантируем, что фоновая подзагрузка библиотеки запущена один раз
            if !self.lib_started {
                self.start_prefetch_library(ctx);
            }
        }

        // Правая панель — фильтры
        let prev_query = self.query.clone();
        let (apply, open_settings, open_logs, open_about) = draw_filters_panel(
            ctx,
            &mut self.sort,
            &mut self.date_limit,
            &mut self.include_logic,
            &mut self.include_tags,
            &mut self.exclude_mode,
            &mut self.exclude_tags,
            &mut self.include_prefixes,
            &mut self.exclude_prefixes,
            &mut self.search_mode,
            &mut self.query,
            &mut self.library_only,
        );
        if apply {
            // Немедленно перезапустить поиск при изменении фильтров (кроме текста)
            self.page = 1;
            self.search_due_at = None;
            if self.library_only {
                self.start_fetch_library(ctx);
            } else {
                self.start_fetch(ctx);
            }
        }
        // Debounce text query changes: run search 0.3s after last edit
        let query_changed = self.query != prev_query;
        if query_changed {
            if apply {
                // Filters changed this frame and already triggered immediate fetch; skip debounce
                self.search_due_at = None;
            } else {
                self.page = 1;
                self.search_due_at = Some(Instant::now() + Duration::from_millis(300));
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
        // Trigger new fetch when Library mode toggles
        if self.last_library_only != self.library_only {
            self.last_library_only = self.library_only;
            if self.library_only {
                // Если фоновые данные уже есть — мгновенно показываем их
                if let Some(msg) = &self.lib_result {
                    self.last_result = Some(msg.clone());
                    self.last_error = None;
                    self.loading = false;
                    // Immediately schedule cover downloads for the freshly shown Library data
                    self.schedule_cover_downloads(ctx);
                    ctx.request_repaint();
                } else {
                    // Обеспечим запуск фоновой загрузки и покажем спиннер
                    if !self.lib_started {
                        self.start_prefetch_library(ctx);
                    }
                    self.last_result = None;
                    self.last_error = None;
                    self.loading = true;
                }
            } else {
                self.start_fetch(ctx);
            }
        }

        // Run debounced query fetch if deadline passed
        if let Some(due) = self.search_due_at {
            if Instant::now() >= due {
                self.search_due_at = None;
                if self.library_only {
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

                    if let Some(err) = &self.last_error {
                        ui.vertical_centered(|ui| {
                            ui.colored_label(egui::Color32::RED, format!("Error: {}", err));
                        });
                    } else if self.loading && self.last_result.is_none() {
                        ui.add_space(24.0);
                        ui.vertical_centered(|ui| {
                            ui.add(egui::Spinner::new());
                            ui.label("Loading...");
                        });
                    } else if self.last_result.is_some() {
                        // Clone data so we don't hold an immutable borrow of `self` across a call
                        // that needs `&mut self` (draw_threads_grid).
                        let data_cloned = {
                            let msg = self.last_result.as_ref().unwrap();
                            msg.data.clone()
                        };
                        // Build a set of hidden thread_ids and filter them out from rendering
                        let hidden: std::collections::HashSet<u64> = {
                            let st = settings::APP_SETTINGS.read().unwrap();
                            st.hidden_threads.iter().copied().collect()
                        };

                        // When Library mode is ON, show downloaded AND in-progress games; always ignore hidden ones
                        let display_data: Vec<crate::parser::F95Thread> = if self.library_only {
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
                        self.draw_threads_grid(ui, ctx, &display_data, cols, left_pad, gap, card_w);

                        // Bottom controls: pagination in normal mode, summary in Library mode
                        ui.add_space(8.0);
                        ui.vertical_centered(|ui| {
                            if self.library_only {
                                let installed_count = {
                                    let st = settings::APP_SETTINGS.read().unwrap();
                                    st.downloaded_games
                                        .iter()
                                        .filter(|g| settings::game_folder_exists(&g.folder))
                                        .count()
                                };
                                ui.label(format!("Library: {} / {} found", display_data.len(), installed_count));
                            } else {
                                let (cur, total) = {
                                    let msg = self.last_result.as_ref().unwrap();
                                    (msg.pagination.page, msg.pagination.total)
                                };
                                ui.horizontal(|ui| {
                                    let prev_enabled = cur > 1;
                                    if ui.add_enabled(prev_enabled, egui::Button::new("◀")).clicked() {
                                        self.page = cur.saturating_sub(1);
                                        self.start_fetch(ctx);
                                    }
                                    ui.label(format!("Page {} / {}", cur, total));
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

        // Floating Errors button + Errors window
        errors_ui::draw_errors_button(ctx);
        errors_ui::draw_errors_viewport(ctx);

        // Logs window (separate OS viewport)
        logs_ui::draw_logs_viewport(ctx);

        // About window (separate OS viewport)
        about_ui::draw_about_viewport(ctx);

        // Settings window (separate OS viewport)
        settings::draw_settings_viewport(ctx);
    }
}
