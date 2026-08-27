// Settings data, persistence and filesystem/OS helpers.

pub mod helpers;
pub mod migrate;
pub mod store;

// Store: data types, global state, persistence, and records management
pub use store::{
    APP_SETTINGS, AppSettings, create_bookmark, delete_bookmark, delete_downloaded_game,
    get_bookmark, get_bookmarks, hide_thread, load_settings_from_disk, record_downloaded_game,
    record_pending_download, remove_pending_download, save_settings_to_disk, update_bookmark,
};

// Helpers: filesystem utilities, launching games, and convenience funcs
pub use helpers::{
    game_folder_exists, open_in_browser, reveal_in_file_manager, run_downloaded_game,
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
#[allow(dead_code)]
pub fn with_settings_mut<F, R>(f: F) -> R
where
    F: FnOnce(&mut AppSettings) -> R,
{
    let mut st = APP_SETTINGS.write().unwrap();
    f(&mut st)
}
