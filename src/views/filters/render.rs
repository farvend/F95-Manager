use eframe::egui::{self, Layout, RichText};
use strum::IntoEnumIterator;

use crate::types::*;
use crate::views::filters::items::{
    discrete_slider::discrete_slider, mode_switch::mode_switch_small,
    prefixes_menu::prefixes_picker, search_with_mode::search_with_mode,
    segmented_panel::segmented_panel, tags_menu::tags_picker,
};

/// Helper function to render removable items (tags/prefixes) with close buttons.
/// Returns true if an item was removed, false otherwise.
/// DRY principle: Unifies 4 duplicated blocks of code.
fn render_removable_items<F>(ui: &mut egui::Ui, items: &mut Vec<u32>, name_resolver: F) -> bool
where
    F: Fn(u32) -> String,
{
    let mut to_remove: Option<usize> = None;
    ui.horizontal_wrapped(|ui| {
        for (i, &id) in items.iter().enumerate() {
            let name = name_resolver(id);
            if ui.button(format!("{} ×", name)).clicked() {
                to_remove = Some(i);
            }
        }
    });
    if let Some(i) = to_remove {
        items.remove(i);
        return true;
    }
    false
}

/// Draws the right-side filters panel.
/// State is passed in by mutable references and updated in-place.
pub fn draw_filters_panel(
    ctx: &egui::Context,
    sort: &mut Sorting,
    date_limit: &mut DateLimit,
    include_logic: &mut TagLogic,
    include_tags: &mut Vec<u32>,
    _exclude_mode: &mut Vec<u32>,
    exclude_tags: &mut Vec<u32>,
    include_prefixes: &mut Vec<u32>,
    exclude_prefixes: &mut Vec<u32>,
    search_mode: &mut SearchMode,
    query: &mut String,
    library_only: &mut bool,
) -> (bool, bool, bool, bool) {
    let mut changed_now: bool = false;
    let mut settings_clicked: bool = false;
    let mut logs_clicked: bool = false;
    let mut about_clicked: bool = false;
    egui::SidePanel::right("filters_panel")
        .frame(
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(30, 30, 30))
                .inner_margin(10.0),
        )
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(RichText::new(crate::localization::translate("filters-title")).strong());
            ui.separator();

            // SORTING
            if segmented_panel(ui, "filters-sorting", sort) {
                changed_now = true;
            }

            ui.separator();

            // DATE LIMIT
            let values: Vec<DateLimit> = DateLimit::iter().collect();
            if let Some(new_limit) = discrete_slider(
                ui,
                crate::localization::translate("filters-date-limit").as_str(),
                date_limit,
                &values,
            ) {
                *date_limit = new_limit;
                changed_now = true;
            }

            ui.separator();

            // SEARCH
            if let Some(new_mode) = mode_switch_small(
                ui,
                crate::localization::translate("filters-search").as_str(),
                search_mode,
            ) {
                *search_mode = new_mode;
                changed_now = true;
            }
            let _ = search_with_mode(ui, query);

            ui.separator();

            // TAGS (MAX 10) with OR/AND logic
            if let Some(new_mode) = mode_switch_small(
                ui,
                crate::localization::translate_with(
                    "filters-include-tags-header",
                    &[("max", crate::ui_constants::MAX_FILTER_ITEMS_STR.to_string())],
                )
                .as_str(),
                include_logic,
            ) {
                *include_logic = new_mode;
                changed_now = true;
            }
            if let Some(id) = tags_picker(
                ui,
                "include_tags",
                crate::localization::translate("filters-select-tag-include").as_str(),
            ) {
                if include_tags.len() < crate::ui_constants::MAX_FILTER_ITEMS
                    && !include_tags.contains(&id)
                {
                    include_tags.push(id);
                    // Clear main text query when picking a tag
                    query.clear();
                    changed_now = true;
                }
            }
            if render_removable_items(ui, include_tags, crate::tags::get_tag_name_by_id) {
                changed_now = true;
            }

            ui.separator();

            // EXCLUDE TAGS (MAX 10)
            ui.label(
                RichText::new(crate::localization::translate_with(
                    "filters-exclude-tags-header",
                    &[("max", crate::ui_constants::MAX_FILTER_ITEMS_STR.to_string())],
                ))
                .weak(),
            );
            if let Some(id) = tags_picker(
                ui,
                "exclude_tags",
                crate::localization::translate("filters-select-tag-exclude").as_str(),
            ) {
                if exclude_tags.len() < crate::ui_constants::MAX_FILTER_ITEMS
                    && !exclude_tags.contains(&id)
                {
                    exclude_tags.push(id);
                    // Clear main text query when picking a tag
                    query.clear();
                    changed_now = true;
                }
            }
            if render_removable_items(ui, exclude_tags, crate::tags::get_tag_name_by_id) {
                changed_now = true;
            }
            ui.separator();

            // PREFIXES (MAX 10)
            ui.label(
                RichText::new(crate::localization::translate_with(
                    "filters-include-prefixes-header",
                    &[("max", crate::ui_constants::MAX_FILTER_ITEMS_STR.to_string())],
                ))
                .weak(),
            );
            if let Some(id) = prefixes_picker(
                ui,
                "include_prefixes",
                crate::localization::translate("filters-select-prefix-include").as_str(),
            ) {
                if include_prefixes.len() < crate::ui_constants::MAX_FILTER_ITEMS
                    && !include_prefixes.contains(&id)
                {
                    include_prefixes.push(id);
                    changed_now = true;
                }
            }
            if render_removable_items(ui, include_prefixes, crate::tags::get_prefix_name_by_id) {
                changed_now = true;
            }

            ui.separator();

            // EXCLUDE PREFIXES (MAX 10)
            ui.label(
                RichText::new(crate::localization::translate_with(
                    "filters-exclude-prefixes-header",
                    &[("max", crate::ui_constants::MAX_FILTER_ITEMS_STR.to_string())],
                ))
                .weak(),
            );
            if let Some(id) = prefixes_picker(
                ui,
                "exclude_prefixes",
                crate::localization::translate("filters-select-prefix-exclude").as_str(),
            ) {
                if exclude_prefixes.len() < crate::ui_constants::MAX_FILTER_ITEMS
                    && !exclude_prefixes.contains(&id)
                {
                    exclude_prefixes.push(id);
                    changed_now = true;
                }
            }
            if render_removable_items(ui, exclude_prefixes, crate::tags::get_prefix_name_by_id) {
                changed_now = true;
            }

            ui.add_space(crate::ui_constants::spacing::MEDIUM);
            ui.with_layout(Layout::bottom_up(egui::Align::LEFT), |ui| {
                if ui
                    .button(crate::localization::translate("common-logs"))
                    .clicked()
                {
                    logs_clicked = true;
                }
                if ui
                    .button(crate::localization::translate("common-about"))
                    .clicked()
                {
                    about_clicked = true;
                }
                if ui
                    .button(crate::localization::translate("common-settings"))
                    .clicked()
                {
                    settings_clicked = true;
                }
                // Library toggle above Settings
                let label = if *library_only {
                    crate::localization::translate("filters-library-on")
                } else {
                    crate::localization::translate("filters-library")
                };
                if ui.button(label).clicked() {
                    *library_only = !*library_only;
                }
            });
        });

    (changed_now, settings_clicked, logs_clicked, about_clicked)
}
