use eframe::egui::{self, Layout, RichText};
use strum::IntoEnumIterator;

use crate::types::*;
use crate::views::filters::items::{
    segmented_panel::segmented_panel,
    discrete_slider::discrete_slider,
    mode_switch::mode_switch_small,
    search_with_mode::search_with_mode,
    tags_menu::tags_picker,
    prefixes_menu::prefixes_picker,
};

/// Draws the right-side filters panel.
/// State is passed in by mutable references and updated in-place.
pub fn draw_filters_panel(
    ctx: &egui::Context,
    sort: &mut Sorting,
    date_limit: &mut DateLimit,
    include_logic: &mut TagLogic,
    include_tags: &mut Vec<u32>,
    exclude_mode: &mut Vec<u32>,
    exclude_tags: &mut Vec<u32>,
    include_prefixes: &mut Vec<u32>,
    exclude_prefixes: &mut Vec<u32>,
    search_mode: &mut SearchMode,
    query: &mut String,
    library_only: &mut bool,
) -> (bool, bool, bool) {
    let mut changed_now: bool = false;
    let mut settings_clicked: bool = false;
    let mut logs_clicked: bool = false;
    egui::SidePanel::right("filters_panel")
        .frame(
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(30, 30, 30))
                .inner_margin(10.0),
        )
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(RichText::new("Filters").strong());
            ui.separator();

            // SORTING
            if segmented_panel(ui, "SORTING", sort) {
                changed_now = true;
            }

            ui.separator();

            // DATE LIMIT
            let values: Vec<DateLimit> = DateLimit::iter().collect();
            if let Some(new_limit) = discrete_slider(ui, "DATE LIMIT", date_limit, &values) {
                *date_limit = new_limit;
                changed_now = true;
            }

            ui.separator();

            // SEARCH
            if let Some(new_mode) = mode_switch_small(ui, "SEARCH", search_mode) {
                *search_mode = new_mode;
                changed_now = true;
            }
            let _ = search_with_mode(ui, query);

            ui.separator();

            // TAGS (MAX 10) with OR/AND logic
            if let Some(new_mode) = mode_switch_small(ui, "TAGS (MAX 10)", include_logic) {
                *include_logic = new_mode;
                changed_now = true;
            }
            if let Some(id) = tags_picker(ui, "include_tags", "Select a tag to filter...") {
                if include_tags.len() < 10 && !include_tags.contains(&id) {
                    include_tags.push(id);
                    // Clear main text query when picking a tag
                    query.clear();
                    changed_now = true;
                }
            }
            ui.horizontal_wrapped(|ui| {
                let mut to_remove: Option<usize> = None;
                for (i, id) in include_tags.iter().enumerate() {
                    let name = crate::tags::TAGS
                        .tags
                        .get(&id.to_string())
                        .cloned()
                        .unwrap_or_else(|| id.to_string());
                    if ui.button(format!("{} ×", name)).clicked() {
                        to_remove = Some(i);
                    }
                }
                if let Some(i) = to_remove {
                    include_tags.remove(i);
                    changed_now = true;
                }
            });

            ui.separator();

            // EXCLUDE TAGS (MAX 10)
            ui.label(RichText::new("EXCLUDE TAGS (MAX 10)").weak());
            if let Some(id) = tags_picker(ui, "exclude_tags", "Select a tag to exclude...") {
                if exclude_tags.len() < 10 && !exclude_tags.contains(&id) {
                    exclude_tags.push(id);
                    // Clear main text query when picking a tag
                    query.clear();
                    changed_now = true;
                }
            }
            ui.horizontal_wrapped(|ui| {
                let mut to_remove: Option<usize> = None;
                for (i, id) in exclude_tags.iter().enumerate() {
                    let name = crate::tags::TAGS
                        .tags
                        .get(&id.to_string())
                        .cloned()
                        .unwrap_or_else(|| id.to_string());
                    if ui.button(format!("{} ×", name)).clicked() {
                        to_remove = Some(i);
                    }
                }
                if let Some(i) = to_remove {
                    exclude_tags.remove(i);
                    changed_now = true;
                }
            });
            ui.separator();

            // PREFIXES (MAX 10)
            ui.label(RichText::new("PREFIXES (MAX 10)").weak());
            if let Some(id) = prefixes_picker(ui, "include_prefixes", "Select a prefix to filter...") {
                if include_prefixes.len() < 10 && !include_prefixes.contains(&id) {
                    include_prefixes.push(id);
                    changed_now = true;
                }
            }
            ui.horizontal_wrapped(|ui| {
                let mut to_remove: Option<usize> = None;
                for (i, id) in include_prefixes.iter().enumerate() {
                    // Find prefix name by id
                    let mut name: Option<String> = None;
                    for group in &crate::tags::TAGS.prefixes.games {
                        if let Some(pref) = group.prefixes.iter().find(|p| p.id as u32 == *id) {
                            name = Some(pref.name.clone());
                            break;
                        }
                    }
                    let name = name.unwrap_or_else(|| id.to_string());
                    if ui.button(format!("{} ×", name)).clicked() {
                        to_remove = Some(i);
                    }
                }
                if let Some(i) = to_remove {
                    include_prefixes.remove(i);
                    changed_now = true;
                }
            });

            ui.separator();

            // EXCLUDE PREFIXES (MAX 10)
            ui.label(RichText::new("EXCLUDE PREFIXES (MAX 10)").weak());
            if let Some(id) = prefixes_picker(ui, "exclude_prefixes", "Select a prefix to exclude...") {
                if exclude_prefixes.len() < 10 && !exclude_prefixes.contains(&id) {
                    exclude_prefixes.push(id);
                    changed_now = true;
                }
            }
            ui.horizontal_wrapped(|ui| {
                let mut to_remove: Option<usize> = None;
                for (i, id) in exclude_prefixes.iter().enumerate() {
                    // Find prefix name by id
                    let mut name: Option<String> = None;
                    for group in &crate::tags::TAGS.prefixes.games {
                        if let Some(pref) = group.prefixes.iter().find(|p| p.id as u32 == *id) {
                            name = Some(pref.name.clone());
                            break;
                        }
                    }
                    let name = name.unwrap_or_else(|| id.to_string());
                    if ui.button(format!("{} ×", name)).clicked() {
                        to_remove = Some(i);
                    }
                }
                if let Some(i) = to_remove {
                    exclude_prefixes.remove(i);
                    changed_now = true;
                }
            });

            ui.add_space(8.0);
            ui.with_layout(Layout::bottom_up(egui::Align::LEFT), |ui| {
                if ui.button("Logs").clicked() {
                    logs_clicked = true;
                }
                if ui.button("Settings").clicked() {
                    settings_clicked = true;
                }
                // Library toggle above Settings
                let label = if *library_only { "Library (ON)" } else { "Library" };
                if ui.button(label).clicked() {
                    *library_only = !*library_only;
                }
            });
        });

        (changed_now, settings_clicked, logs_clicked)
    }
