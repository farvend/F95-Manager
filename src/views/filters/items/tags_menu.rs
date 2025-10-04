use eframe::egui::Ui;

use crate::tags::TAGS;
use super::picker::dropdown_picker;

/// Dynamic tags picker with inline search and dropdown popup.
/// Returns Some(tag_id) when user picks a tag; otherwise None.
pub fn tags_picker(ui: &mut Ui, key: &str, placeholder: &str) -> Option<u32> {
    dropdown_picker(ui, key, placeholder, "tags_picker", |q| {
        let ql = q.to_lowercase();
        let mut items: Vec<(u32, String)> = TAGS
            .tags
            .iter()
            .filter_map(|(k, v)| {
                if !ql.is_empty() && !v.to_lowercase().contains(&ql) {
                    return None;
                }
                match k.parse::<u32>() {
                    Ok(id) => Some((id, v.clone())),
                    Err(_) => None,
                }
            })
            .collect();
        // Sorting is handled inside dropdown_picker, but leaving this as-is is fine too.
        items
    })
}
