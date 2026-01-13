// Facade module for cards building blocks.
// Re-export card primitives so render.rs can import via views::cards::items.
pub mod card;
mod cover_helpers;
mod cover_hover;
mod meta_row;
mod tags_panel;
pub use card::thread_card;
