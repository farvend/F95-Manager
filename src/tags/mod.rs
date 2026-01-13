mod types;
pub use crate::tags::types::*;

lazy_static::lazy_static!(
    pub static ref TAGS: Tags = serde_json::from_str(include_str!("tags.json")).unwrap();
);

/// Helper function to get prefix name by ID.
/// DRY principle: Extracts duplicated prefix name lookup logic.
pub fn get_prefix_name_by_id(id: u32) -> String {
    TAGS.prefixes
        .games
        .iter()
        .flat_map(|group| &group.prefixes)
        .find(|p| p.id as u32 == id)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| id.to_string())
}

/// Helper function to get tag name by ID.
pub fn get_tag_name_by_id(id: u32) -> String {
    TAGS.tags
        .get(&id.to_string())
        .cloned()
        .unwrap_or_else(|| id.to_string())
}
