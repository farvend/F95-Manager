pub mod check;
#[cfg(feature = "legacy-egui")]
pub mod ui;

pub use check::{GameUpdateInfo, check_all_updates, check_single_game};
