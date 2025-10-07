// Render facade for cards: re-export the implementation from views::cards::items
// so external code keeps using views::cards::{thread_card, CARD_WIDTH}.

pub use crate::views::cards::items::thread_card;
