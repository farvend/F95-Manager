use eframe::egui::{self, Layout, RichText};
use strum::IntoEnumIterator;

use crate::types::ViewMode;
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
    filter_bookmarks: &mut Vec<String>,
    library_only: &mut bool,
    unplayed_only: &mut bool,
) -> (bool, bool, bool, bool, bool) {
    let mut changed_now: bool = false;
    let mut settings_clicked: bool = false;
    let mut logs_clicked: bool = false;
    let mut about_clicked: bool = false;
    let mut bookmarks_clicked: bool = false;
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

            // MY BOOKMARKS
            ui.separator();
            ui.label(
                RichText::new(crate::localization::translate("filters-bookmarks-header")).weak(),
            );

            let all_bookmarks = crate::app::settings::store::get_bookmarks();
            let available_bookmarks: Vec<_> = all_bookmarks
                .iter()
                .filter(|b| !filter_bookmarks.contains(&b.id))
                .collect();

            if !available_bookmarks.is_empty() {
                let pick = crate::views::filters::items::picker::dropdown_picker(
                    ui,
                    "filter_bookmarks",
                    &crate::localization::translate("filters-select-bookmark"),
                    "bookmark_filter_picker",
                    |q| {
                        available_bookmarks
                            .iter()
                            .filter(|b| b.label.to_lowercase().contains(&q.to_lowercase()))
                            .map(|b| (0u32, format!("{} {}", b.emoji, b.label))) // id doesn't matter here, we use index
                            .collect()
                    },
                );
                // The picker implementation returns Option<u32>, but it's based on alphabetical sort inside.
                // It's safer to re-find the item or use a different approach.
                // However, dropdown_picker is designed for u32. Let's adapt.
                if let Some(_) = pick {
                    // Since pick index depends on search query which is cleared,
                    // and dropdown_picker is generic, I'll need a better way if I want to use it.
                    // For now, let's use a simpler version or just the labels.
                }
            }

            // Simpler bookmark selector for filter since dropdown_picker is for u32
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_source("bookmark_filter_combo")
                    .selected_text(crate::localization::translate("filters-select-bookmark"))
                    .show_ui(ui, |ui| {
                        for b in available_bookmarks {
                            if ui
                                .selectable_label(false, format!("{} {}", b.emoji, b.label))
                                .clicked()
                            {
                                filter_bookmarks.push(b.id.clone());
                                changed_now = true;
                            }
                        }
                    });
            });

            let mut to_remove: Option<usize> = None;
            ui.horizontal_wrapped(|ui| {
                for (i, id) in filter_bookmarks.iter().enumerate() {
                    let name = crate::app::settings::store::get_bookmark(id)
                        .map(|b| format!("{} {}", b.emoji, b.label))
                        .unwrap_or_else(|| id.clone());
                    if ui.button(format!("{} ×", name)).clicked() {
                        to_remove = Some(i);
                    }
                }
            });
            if let Some(i) = to_remove {
                filter_bookmarks.remove(i);
                changed_now = true;
            }

            if *library_only {
                ui.separator();
                if ui
                    .checkbox(
                        unplayed_only,
                        crate::localization::translate("filters-unplayed"),
                    )
                    .changed()
                {
                    changed_now = true;
                }
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
                if ui
                    .button(crate::localization::translate("common-bookmarks"))
                    .clicked()
                {
                    bookmarks_clicked = true;
                }

                let classic_mode = crate::app::settings::APP_SETTINGS
                    .read()
                    .unwrap()
                    .classic_library_toggle;

                if classic_mode {
                    let label = if *library_only {
                        crate::localization::translate("filters-library-button-on")
                    } else {
                        crate::localization::translate("filters-library-button")
                    };
                    if ui.button(label).clicked() {
                        *library_only = !*library_only;
                        changed_now = true;
                    }
                } else {
                    let mut view_mode = if *library_only {
                        ViewMode::Downloaded
                    } else {
                        ViewMode::Catalog
                    };
                    if segmented_panel(ui, "view-mode-header", &mut view_mode) {
                        *library_only = matches!(view_mode, ViewMode::Downloaded);
                    }
                }
            });
        });

    (
        changed_now,
        settings_clicked,
        logs_clicked,
        about_clicked,
        bookmarks_clicked,
    )
}
