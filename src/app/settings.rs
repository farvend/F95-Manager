// Settings module split: store (data & persistence), helpers (fs/OS utils), ui (egui windows).
// This file aggregates submodules and re-exports public API to preserve existing imports.

pub mod helpers;
pub mod migrate;
pub mod store;
pub mod ui;

// Store: data types, global state, persistence, and records management
pub use store::{
    APP_SETTINGS, AppSettings, DownloadedGame, delete_downloaded_game, downloaded_game_exe,
    downloaded_game_folder, hide_thread, is_pending_download, is_thread_hidden,
    load_settings_from_disk, record_downloaded_game, record_pending_download,
    remove_pending_download, save_settings_to_disk,
};

// Helpers: filesystem utilities, launching games, and convenience funcs
pub use helpers::{
    copy_dir_all, game_folder_exists, move_directory, open_in_browser, reveal_in_file_manager,
    run_downloaded_game,
};

// UI: egui viewport window for settings and separate eframe App
pub use ui::{SettingsApp, SettingsMsg, draw_settings_viewport, open_settings};

/// Helper function to read settings with a closure.
/// DRY principle: Reduces boilerplate of `.read().unwrap()` pattern.
pub fn with_settings<F, R>(f: F) -> R
where
    F: FnOnce(&AppSettings) -> R,
{
    let st = APP_SETTINGS.read().unwrap();
    f(&st)
}

/// Helper function to modify settings with a closure.
/// DRY principle: Reduces boilerplate of `.write().unwrap()` pattern.
pub fn with_settings_mut<F, R>(f: F) -> R
where
    F: FnOnce(&mut AppSettings) -> R,
{
    let mut st = APP_SETTINGS.write().unwrap();
    f(&mut st)
}
