// Логика приложения вынесена из main.rs, чтобы убрать глубокую вложенность в конце main.
// Рефакторинг: крупные группы полей вынесены в отдельные структуры в app/state.rs.

use eframe::{App, egui};
use std::collections::HashMap;

mod about_ui;
pub mod config;
mod errors_ui;
mod grid;
mod logs_ui;
pub mod settings;
mod update_ui;

// Вынесено: tokio runtime и вся логика получения данных
mod cache;
mod downloads;
mod fetch;
mod runtime;
mod state;

// UI под разные состояния приложения
mod auth_screen;
mod main_screen;

use downloads::DownloadState;
pub use fetch::CoverMsg;
pub use runtime::RUNTIME;
pub use runtime::rt;
use state::{AuthState, FiltersState, ImagesState, NetState, Screen};

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
        let screen = if need_auth {
            Screen::AuthLogin
        } else {
            Screen::Main
        };

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

        // Authorization gating: если нет cookies в конфиге — показываем экран авторизации и выходим
        if self.auth.screen != Screen::Main {
            auth_screen::update_auth(self, ctx);
            return;
        }

        // Основной экран приложения
        main_screen::update_main(self, ctx);
    }
}
