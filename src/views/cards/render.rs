// Render facade for cards: re-export the implementation from views::cards::items
// so external code keeps using views::cards::{thread_card, CARD_WIDTH}.

pub use crate::views::cards::items::thread_card;

/// Default card width used by the grid (in logical pixels).
pub const CARD_WIDTH: f32 = 320.0;
