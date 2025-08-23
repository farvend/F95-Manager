// Helpers module split into focused submodules for clarity and reuse.
//
// Submodules:
// - open: cross-platform helpers to open browser and reveal folders
// - paths: path utils and folder existence checks
// - fs_ops: move/copy directory helpers
// - run: game launching logic (Windows-specific runner + cross-platform fallback)

pub mod open;
pub mod paths;
pub mod fs_ops;
pub mod run;

// Re-export public API to preserve existing imports via crate::app::settings::helpers::*
pub use open::{open_in_browser, reveal_in_file_manager};
pub use paths::game_folder_exists;
pub use fs_ops::{move_directory, copy_dir_all};
pub use run::run_downloaded_game;
