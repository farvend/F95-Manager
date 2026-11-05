use crate::game_download::{GameDownloadStatus, Progress};
use crate::parser::game_info::link::DownloadLink;
use crate::parser::{F95Filters, F95Thread};
use crate::types::{DateLimit, SearchMode, Sorting, TagLogic};
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};

slint::include_modules!();

pub mod captcha;
mod state;
use state::*;
mod callbacks;
use callbacks::*;
mod filters;
use filters::*;
mod catalog;
use catalog::*;
mod images;
use images::*;
mod downloads;
use downloads::*;
mod errors;
use errors::*;
mod cards;
use cards::*;
mod bookmarks;
use bookmarks::*;
mod translations;
use translations::*;

fn show_and_focus<C: ComponentHandle>(component: &C) {
    if let Err(error) = component.show() {
        log::error!("Failed to show auxiliary window: {error}");
        return;
    }
    component.window().set_minimized(false);
    component
        .window()
        .with_winit_window(|window| window.focus_window());
}

pub fn run() -> Result<(), slint::PlatformError> {
    let renderer = std::env::var("F95_RENDERER").unwrap_or_else(|_| "femtovg".to_string());
    if let Err(error) = slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name(renderer.clone())
        .select()
    {
        if renderer == "software" {
            return Err(error);
        }
        log::warn!("Slint renderer {renderer} failed ({error}); falling back to software");
        slint::BackendSelector::new()
            .backend_name("winit".into())
            .renderer_name("software".into())
            .select()?;
    }

    let settings = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| settings.clone())
        .unwrap_or_default();
    let selected_bookmarks: HashSet<String> = settings.filter_bookmarks.iter().cloned().collect();
    let start_in_library = !selected_bookmarks.is_empty();
    let state = Arc::new(Mutex::new(UiState {
        library_mode: start_in_library,
        selected_bookmarks,
        include_tags: settings.startup_tags.clone(),
        exclude_tags: settings.startup_exclude_tags.clone(),
        prefixes: settings.startup_prefixes.clone(),
        exclude_prefixes: settings.startup_exclude_prefixes.clone(),
        ..UiState::default()
    }));

    let ui = MainWindow::new()?;
    ui.window().on_close_requested(|| {
        // This application owns several top-level Slint windows. Hiding the
        // main window alone therefore does not necessarily stop winit's event
        // loop while one of the auxiliary window components is still alive.
        // Closing the main window means exiting the application.
        if let Err(error) = slint::quit_event_loop() {
            log::error!("Failed to quit Slint event loop: {error}");
        }
        slint::CloseRequestResponse::HideWindow
    });
    let authenticated = crate::app::config::APP_CONFIG
        .read()
        .ok()
        .and_then(|config| config.cookies.clone())
        .is_some_and(|cookies| !cookies.trim().is_empty());
    ui.set_authenticated(authenticated);
    if let Ok(config) = crate::app::config::APP_CONFIG.read() {
        ui.set_auth_username(config.username.clone().unwrap_or_default().into());
    }
    let settings_window = Rc::new(SettingsWindow::new()?);
    let logs_window = Rc::new(LogsWindow::new()?);
    let about_window = Rc::new(AboutWindow::new()?);
    let errors_window = Rc::new(ErrorsWindow::new()?);
    let bookmarks_window = Rc::new(BookmarksWindow::new()?);
    about_window.set_repository_url(env!("CARGO_PKG_REPOSITORY").into());
    about_window.on_open_repository(|| {
        crate::app::settings::open_in_browser(env!("CARGO_PKG_REPOSITORY"));
    });
    update_all_translations(
        &ui,
        &settings_window,
        &logs_window,
        &about_window,
        &errors_window,
        &bookmarks_window,
    );
    ui.set_ui_scale_percent(settings.ui_scale_percent.into());
    ui.set_card_scale_percent(settings.card_scale_percent.into());
    ui.set_library_mode(start_in_library);
    ui.set_session_mode(false);
    ui.set_classic_library(settings.classic_library_toggle);
    ui.global::<AppTheme>()
        .set_scale(settings.ui_scale_percent as f32 / 100.0);
    ui.global::<AppTheme>()
        .set_card_scale(settings.card_scale_percent as f32 / 100.0);
    ui.global::<AppTheme>().set_card_width(
        320.0 * settings.ui_scale_percent as f32 / 100.0 * settings.card_scale_percent as f32
            / 100.0,
    );
    update_bookmarks(&ui, &state);
    update_selected_filters(&ui, &state);
    wire_callbacks(
        &ui,
        state.clone(),
        settings_window,
        logs_window,
        about_window,
        errors_window,
        bookmarks_window,
    );
    if authenticated {
        preload_library_data(state.clone(), ui.as_weak());
        if start_in_library {
            load_library(state, ui.as_weak());
        } else {
            load_catalog(1, String::new(), state, ui.as_weak());
        }
    } else {
        ui.set_loading(false);
    }
    check_for_app_update(&ui);
    ui.show()?;
    fit_height_to_filters(&ui);
    ui.run()
}

/// Ask GitHub once per launch whether a newer version is out and, if so, show
/// the notice in the corner; clicking it opens that release's page.
fn check_for_app_update(ui: &MainWindow) {
    let weak = ui.as_weak();
    crate::app::rt().spawn(async move {
        let update = match crate::app::app_update::check().await {
            Ok(Some(update)) => update,
            Ok(None) => return,
            Err(error) => {
                log::warn!("update check failed: {error}");
                return;
            }
        };
        log::info!("update available: {} ({})", update.version, update.url);
        let shown = weak.upgrade_in_event_loop(move |ui| {
            ui.set_update_version(update.version.to_string().into());
            let url = update.url;
            ui.on_open_update(move || crate::app::settings::open_in_browser(&url));
        });
        if let Err(error) = shown {
            log::warn!("cannot show the update notice: {error}");
        }
    });
}

/// Start tall enough for the whole right panel to fit without scrolling, but
/// no taller than half the screen. Runs once the window has a monitor.
fn fit_height_to_filters(ui: &MainWindow) {
    let window = ui.window();
    let scale = window.scale_factor();
    let wanted = ui.get_filters_fit_height();
    let Some(monitor_height) = window.with_winit_window(|native| {
        native
            .current_monitor()
            .map(|monitor| monitor.size().height as f32 / scale)
    }) else {
        log::warn!("main window has no native window yet; keeping its default height");
        return;
    };
    let Some(monitor_height) = monitor_height else {
        log::warn!("main window is on no monitor; keeping its default height");
        return;
    };
    let height = wanted.min(monitor_height / 2.0);
    let width = window.size().to_logical(scale).width;
    window.set_size(slint::LogicalSize::new(width, height));
}
