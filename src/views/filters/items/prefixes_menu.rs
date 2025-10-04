use eframe::egui::Ui;

use crate::tags::TAGS;
use super::picker::dropdown_picker;

/// Dynamic prefixes picker (for F95 prefixes) with inline search and dropdown popup.
/// Returns Some(prefix_id) when user picks a prefix; otherwise None.
/// Currently lists prefixes for the "games" category (which is what the app queries).
pub fn prefixes_picker(ui: &mut Ui, key: &str, placeholder: &str) -> Option<u32> {
    dropdown_picker(ui, key, placeholder, "prefixes_picker", |q| {
        let ql = q.to_lowercase();
        let mut items: Vec<(u32, String)> = Vec::new();
        for group in &TAGS.prefixes.games {
            for p in &group.prefixes {
                let name = p.name.as_str();
                if !ql.is_empty() && !name.to_lowercase().contains(&ql) {
                    continue;
                }
                items.push((p.id as u32, name.to_string()));
            }
        }
        items
    })
}
