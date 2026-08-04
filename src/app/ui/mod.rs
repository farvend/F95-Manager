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
        .renderer_name(renderer.clone().into())
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
    about_window.set_version(env!("CARGO_PKG_VERSION").into());
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

    ui.run()
}
