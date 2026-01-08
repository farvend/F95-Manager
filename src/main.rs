#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // скрыть консоль только в release
#![feature(macro_metavar_expr_concat)]
#![feature(iter_intersperse)]
// Точка входа оставлена минимальной: только конфиг окна и запуск приложения.
// Вся логика вынесена в модуль app (src/app.rs), чтобы убрать глубокую вложенность и "лес" табов.

use eframe::{egui, egui_wgpu::WgpuConfiguration, wgpu::PresentMode};

mod parser;
mod types;
mod views;
mod app;
mod tags;
mod logger;
mod localization;
mod ui_constants;
pub mod game_download;

//#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
fn main() -> eframe::Result<()> {
    // Initialize in-app GUI logger (also mirrors to stderr)
    logger::init();
    app::settings::load_settings_from_disk();
    // Load lightweight app_config (for cookies/auth gating)
    app::config::load_config_from_disk();
    // Initialize localization based on settings or system locale (enum-based)
    let preferred_lang = { app::settings::APP_SETTINGS.read().unwrap().language };
    if let Err(e) = localization::initialize_localization(preferred_lang) {
        log::error!("Localization initialization failed: {e}");
    }

    // Настройки для минимальной задержки:
    // - renderer: Wgpu (быстрее и даёт контроль над present mode)
    // - vsync: false (меньше задержка, возможен tearing)
    let wgpu_options = WgpuConfiguration {
        present_mode: eframe::wgpu::PresentMode::AutoNoVsync,
        ..Default::default()
    };
    let native_options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        vsync: false,
        hardware_acceleration: eframe::HardwareAcceleration::Preferred,
        wgpu_options,
        viewport: egui::ViewportBuilder::default()
            //.with_decorations(false)
            .with_inner_size([520.0, 320.0])
            .with_resizable(true),
        ..Default::default()
    };

    let res = eframe::run_native(
        localization::translate("app-window-title").as_str(),
        native_options,
        Box::new(|_cc| Box::new(app::NoLagApp::default())),
    );
    if let Err(ref e) = res {
        log::error!("eframe::run_native failed: {e}");
    }
    res
}
