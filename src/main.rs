#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![feature(macro_metavar_expr_concat)]
#![feature(iter_intersperse)]

mod app;
pub mod game_download;
mod localization;
mod logger;
mod net;
mod parser;
mod slint_ui;
mod tags;
mod types;
mod ui_constants;
#[cfg(feature = "legacy-egui")]
mod views;

fn main() {
    logger::init();
    app::settings::load_settings_from_disk();
    app::config::load_config_from_disk();
    let preferred_lang = { app::settings::APP_SETTINGS.read().unwrap().language };
    if let Err(error) = localization::initialize_localization(preferred_lang) {
        log::error!("Localization initialization failed: {error}");
    }
    let _ = app::rt();

    if let Err(error) = slint_ui::run() {
        log::error!("Slint UI failed: {error}");
    }
}
