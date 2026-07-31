// Логика приложения вынесена из main.rs, чтобы убрать глубокую вложенность в конце main.
// Рефакторинг: крупные группы полей вынесены в отдельные структуры в app/state.rs.

#[cfg(feature = "legacy-egui")]
use eframe::{App, egui};
#[cfg(feature = "legacy-egui")]
use std::collections::HashMap;

#[cfg(feature = "legacy-egui")]
mod about_ui;
pub mod config;
#[cfg(feature = "legacy-egui")]
mod errors_ui;
#[cfg(feature = "legacy-egui")]
pub mod game_updates;
#[cfg(feature = "legacy-egui")]
mod grid;
#[cfg(feature = "legacy-egui")]
pub mod library;
#[cfg(feature = "legacy-egui")]
mod logs_ui;
pub mod persistable;
pub mod settings;
#[cfg(feature = "legacy-egui")]
mod update_ui;

#[cfg(feature = "legacy-egui")]
mod downloads;
#[cfg(feature = "legacy-egui")]
mod fetch;
#[path = "app/fetch/helpers.rs"]
pub mod fetch_helpers;
mod runtime;
#[cfg(feature = "legacy-egui")]
mod state;

// UI под разные состояния приложения
#[cfg(feature = "legacy-egui")]
mod auth_screen;
#[cfg(feature = "legacy-egui")]
mod main_screen;

#[cfg(feature = "legacy-egui")]
use downloads::DownloadState;
#[cfg(feature = "legacy-egui")]
pub use fetch::CoverMsg;
pub use runtime::RUNTIME;
pub use runtime::rt;
#[cfg(feature = "legacy-egui")]
use state::{AuthState, FiltersState, ImagesState, NetState, Screen};

#[cfg(feature = "legacy-egui")]
pub struct NoLagApp {
    page: u32,

    filters: FiltersState,
    net: NetState,
    images: ImagesState,
    auth: AuthState,
    settings_ui: settings::SettingsUiState,
    downloads: HashMap<u64, DownloadState>,

    library_manager: library::LibraryCardManager,

    startup_time: std::time::Instant,
    auto_update_check_triggered: bool,
}

#[cfg(feature = "legacy-egui")]
impl Default for NoLagApp {
    fn default() -> Self {
        crate::app::config::load_config_from_disk();
        let need_auth = {
            let cfg = crate::app::config::APP_CONFIG.read().unwrap();
            cfg.cookies
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
        };
        let screen = if need_auth {
            Screen::AuthLogin
        } else {
            Screen::Main
        };

        let cache_dir = settings::APP_SETTINGS.read().unwrap().cache_dir.clone();
        let cache_dir = if cache_dir.is_relative() {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
                .map(|exe_dir| exe_dir.join(&cache_dir))
                .unwrap_or(cache_dir)
        } else {
            cache_dir
        };
        log::info!("Using cache directory: {:?}", cache_dir);

        let provider = std::sync::Arc::new(library::CachingProvider::new(
            library::NetworkProvider::new(),
            cache_dir,
            library::RealFileSystem,
            library::RealImageCodec,
            library::RealMetadataCodec,
        ));

        Self {
            page: 1,
            filters: FiltersState::default(),
            net: NetState::new(),
            images: ImagesState::new(),
            auth: AuthState::new(screen),
            settings_ui: settings::SettingsUiState::default(),
            downloads: HashMap::new(),
            library_manager: library::LibraryCardManager::new(provider),

            startup_time: std::time::Instant::now(),
            auto_update_check_triggered: false,
        }
    }
}

#[cfg(feature = "legacy-egui")]
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

        // Authorization gating: если нет cookies в конфиге — показываем экран авторизации и выходим
        if self.auth.screen != Screen::Main {
            auth_screen::update_auth(self, ctx);
            return;
        }

        // Auto-check for game updates on startup (after 5 seconds)
        if !self.auto_update_check_triggered && self.startup_time.elapsed().as_secs() >= 5 {
            self.auto_update_check_triggered = true;

            let should_check = {
                let settings = settings::APP_SETTINGS.read().unwrap();
                match settings.update_check_frequency {
                    settings::store::UpdateCheckFrequency::Manual => false,
                    settings::store::UpdateCheckFrequency::OnStartup => true,
                    settings::store::UpdateCheckFrequency::EveryNDays(n) => {
                        if let Some(last_check) = settings.last_update_check {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs() as i64;
                            let days_elapsed = (now - last_check) / 86400;
                            days_elapsed >= n as i64
                        } else {
                            true
                        }
                    }
                }
            };

            if should_check {
                game_updates::ui::trigger_update_check(ctx);

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                {
                    let mut settings = settings::APP_SETTINGS.write().unwrap();
                    settings.last_update_check = Some(now);
                }
                settings::save_settings_to_disk();
            }
        }

        // Основной экран приложения
        main_screen::update_main(self, ctx);
    }
}
