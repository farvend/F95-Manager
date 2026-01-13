// Facade module for filter UI building blocks.
// Re-exports existing ui_items so render.rs can import them under views::filters::items.
pub mod discrete_slider;
pub mod mode_switch;
pub mod picker;
pub mod prefixes_menu;
pub mod search_with_mode;
pub mod segmented_panel;
pub mod tags_menu;
//pub use segmented_panel;
// pub use crate::ui_items::discrete_slider;
// pub use crate::ui_items::mode_menu;
// pub use crate::ui_items::mode_switch;
// pub use crate::ui_items::search_with_mode;
