
mod types;
pub use crate::tags::types::*;
lazy_static::lazy_static!(
    pub static ref TAGS: Tags = serde_json::from_str(include_str!("tags.json")).unwrap();
);