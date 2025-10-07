// Settings module split: store (data & persistence), helpers (fs/OS utils), ui (egui windows).
// This file aggregates submodules and re-exports public API to preserve existing imports.

pub mod store;
pub mod helpers;
pub mod ui;
pub mod migrate;

// Store: data types, global state, persistence, and records management
pub use store::{
    AppSettings,
    DownloadedGame,
    APP_SETTINGS,
    load_settings_from_disk,
    save_settings_to_disk,
    record_downloaded_game,
    record_pending_download,
    remove_pending_download,
    hide_thread,
    is_thread_hidden,
    is_pending_download,
    downloaded_game_folder,
    downloaded_game_exe,
    delete_downloaded_game,
};

// Helpers: filesystem utilities, launching games, and convenience funcs
pub use helpers::{
    open_in_browser,
    reveal_in_file_manager,
    game_folder_exists,
    run_downloaded_game,
    move_directory,
    copy_dir_all,
};

// UI: egui viewport window for settings and separate eframe App
pub use ui::{
    open_settings,
    draw_settings_viewport,
    SettingsMsg,
    SettingsApp,
};

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
