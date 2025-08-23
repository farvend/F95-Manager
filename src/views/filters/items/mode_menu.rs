use eframe::egui::{
    pos2, Area, Color32, Frame, Id, Order, Rounding, Sense, Stroke, Ui, Vec2, TextEdit, ScrollArea,
};
use strum::{EnumCount, IntoEnumIterator};
use crate::views::filters::EnumWithAlternativeNames;

use super::mode_switch::mode_switch_small;

/// Stateless panel with title and dropdown-like menu.
/// - Left: title.
/// - Right: clickable mode (cycles through enum variants).
/// - Below: rectangular clickable area opening a popup with enum items to pick.
/// Returns: (mode_change, item_pick)
pub fn enum_menu_panel<M, I>(
    ui: &mut Ui,
    name: &str,
    mode: &M,
    selected: &Option<I>,
    placeholder: &str,
) -> (Option<M>, Option<I>)
where
    M: IntoEnumIterator
        + EnumCount
        + EnumWithAlternativeNames
        + PartialEq
        + Clone,
    I: IntoEnumIterator
        + EnumCount
        + EnumWithAlternativeNames
        + PartialEq
        + Clone,
{
    // Visual constants in segmented_panel style
    let rounding = Rounding::same(6.0);
    let border_color = Color32::from_gray(80);
    let container_bg = Color32::from_rgb(30, 30, 30);
    let hover_bg = Color32::from_rgba_premultiplied(255, 255, 255, 6);
    let accent = Color32::from_rgb(210, 85, 85);

    // Top row: name on the left, small switcher on the right
    let mut mode_change: Option<M> = None;
    if let Some(new_mode) = mode_switch_small(ui, name, mode) {
        mode_change = Some(new_mode);
    }

    // Dropdown container
    let available_width = ui.available_width();
    let height = (ui.spacing().interact_size.y * 1.4).clamp(28.0, 40.0);
    let (container_rect, response) =
        ui.allocate_exact_size(Vec2::new(available_width, height), Sense::click());
    let response = response.on_hover_cursor(eframe::egui::CursorIcon::PointingHand);
    let painter = ui.painter();

    // Background and border
    painter.rect(
        container_rect,
        rounding,
        container_bg,
        Stroke::new(1.0, border_color),
    );

    // Hover highlight
    if response.hovered() {
        painter.rect(
            container_rect.shrink2(Vec2::new(2.0, 2.0)),
            Rounding::same(4.0),
            hover_bg,
            Stroke::NONE,
        );
    }
    // release painter before mutably borrowing ui
    let _ = painter;

    // Search/edit field in the field itself (not in dropdown)
    // Use a global Id independent of Ui hierarchy so popup can read it too
    let search_id: Id = Id::new(("tags_search", name));
    let mut q = ui
        .memory(|m| m.data.get_temp::<String>(search_id))
        .unwrap_or_default();

    // Place TextEdit inside the container (leave a bit of space for the arrow on the right)
    let inner_rect = container_rect.shrink2(Vec2::new(12.0, 6.0));
    let arrow_space = 18.0;
    let edit_rect = eframe::egui::Rect::from_min_max(
        inner_rect.min,
        eframe::egui::pos2(inner_rect.max.x - arrow_space, inner_rect.max.y),
    );
    let mut edit_response: Option<eframe::egui::Response> = None;
    ui.allocate_ui_at_rect(edit_rect, |ui| {
        let r = ui.add_sized(
            [edit_rect.width(), ui.spacing().interact_size.y],
            TextEdit::singleline(&mut q).hint_text(placeholder).frame(false),
        );
        edit_response = Some(r);
    });
    ui.memory_mut(|m| {
        m.data.insert_temp(search_id, q.clone());
    });

    // Arrow clickable area (we'll draw the caret after we know open/closed state)
    let cx = container_rect.right() - 14.0;
    let cy = container_rect.center().y + 1.0;
    let w = 8.0;
    let h = 5.0;
    let arrow_rect = eframe::egui::Rect::from_center_size(pos2(cx, cy), Vec2::new(18.0, 16.0));
    let arrow_resp = ui
        .interact(
            arrow_rect,
            ui.id().with("mode_menu_arrow").with(name),
            Sense::click(),
        )
        .on_hover_cursor(eframe::egui::CursorIcon::PointingHand);

    // Open/close popup state (global Id independent of Ui hierarchy)
    let popup_id: Id = Id::new(("mode_menu_popup", name));
    let mut is_open = ui
        .memory(|m| m.data.get_temp::<bool>(popup_id))
        .unwrap_or(false);

    // Arrow toggles. Click on empty area of the container toggles open/close.
    // Clicking the text field keeps it open and focuses it.
    let edit_id_opt = edit_response.as_ref().map(|r| r.id);
    if arrow_resp.clicked() {
        is_open = !is_open;
    } else if response.clicked() {
        if is_open {
            // close when dropdown is open and user clicks non-text area
            is_open = false;
        } else {
            // open and focus the input
            is_open = true;
            if let Some(id) = edit_id_opt {
                ui.memory_mut(|m| m.request_focus(id));
            }
        }
    }
    if let Some(r) = &edit_response {
        if r.clicked() || r.has_focus() || r.changed() {
            is_open = true;
        }
    }
    ui.memory_mut(|m| {
        m.data.insert_temp(popup_id, is_open);
    });

    // Draw caret and active border depending on open state
    let col = if is_open { Color32::from_gray(230) } else { Color32::from_gray(200) };
    let painter = ui.painter();
    if is_open {
        // Upwards caret '^'
        painter.line_segment(
            [pos2(cx - w * 0.5, cy + h * 0.5), pos2(cx, cy - h * 0.5)],
            Stroke::new(1.5, col),
        );
        painter.line_segment(
            [pos2(cx + w * 0.5, cy + h * 0.5), pos2(cx, cy - h * 0.5)],
            Stroke::new(1.5, col),
        );
        // Accent border while open
        painter.rect_stroke(container_rect, rounding, Stroke::new(1.0, accent));
    } else {
        // Downwards caret 'v'
        painter.line_segment(
            [pos2(cx - w * 0.5, cy - h * 0.5), pos2(cx, cy + h * 0.5)],
            Stroke::new(1.5, col),
        );
        painter.line_segment(
            [pos2(cx + w * 0.5, cy - h * 0.5), pos2(cx, cy + h * 0.5)],
            Stroke::new(1.5, col),
        );
    }

    // Popup with enum items I
    let mut pick: Option<I> = None;
    if is_open {
        let popup_pos = pos2(container_rect.left(), container_rect.bottom() + 4.0);
        let popup_width = container_rect.width();

        let inner = Area::new(popup_id)
            .order(Order::Foreground)
            .fixed_pos(popup_pos)
            .show(ui.ctx(), |ui| {
                Frame::default()
                    .fill(Color32::from_rgb(28, 28, 28))
                    .stroke(Stroke::new(1.0, border_color))
                    .rounding(Rounding::same(6.0))
                    .show(ui, |ui| {
                        ui.set_min_width(popup_width);

                        // Use query typed in the field above (captured from the outer scope)
                        let ql = q.to_lowercase();

                        // Scrollable list of items with full-width hitboxes
                        ScrollArea::vertical()
                            .max_height(240.0)
                            .show(ui, |ui| {
                                ui.set_width(popup_width - 8.0);
                                for variant in I::iter() {
                                    let name = variant.alternative_name();
                                    if !ql.is_empty() && !name.to_lowercase().contains(&ql) {
                                        continue;
                                    }
                                    let is_selected =
                                        selected.as_ref().map_or(false, |v| v == &variant);

                                    let row_height = ui.spacing().interact_size.y * 1.2;
                                    let (row_rect, row_resp) = ui.allocate_exact_size(
                                        Vec2::new(ui.available_width(), row_height),
                                        Sense::click(),
                                    );
                                    let row_p = ui.painter();

                                    // Background: selected or hover
                                    if is_selected {
                                        row_p.rect(
                                            row_rect.shrink2(Vec2::new(2.0, 2.0)),
                                            Rounding::same(4.0),
                                            Color32::from_rgb(45, 45, 45),
                                            Stroke::NONE,
                                        );
                                    } else if row_resp.hovered() {
                                        row_p.rect(
                                            row_rect.shrink2(Vec2::new(2.0, 2.0)),
                                            Rounding::same(4.0),
                                            hover_bg,
                                            Stroke::NONE,
                                        );
                                    }

                                    // Text left-aligned with padding
                                    row_p.text(
                                        pos2(row_rect.left() + 8.0, row_rect.center().y),
                                        eframe::egui::Align2::LEFT_CENTER,
                                        name,
                                        eframe::egui::FontId::proportional(14.0),
                                        if is_selected { accent } else { Color32::from_gray(210) },
                                    );

                                    let row_resp = row_resp.on_hover_cursor(eframe::egui::CursorIcon::PointingHand);
                                    if row_resp.clicked() {
                                        pick = Some(variant.clone());
                                        ui.memory_mut(|m| {
                                            m.data.insert_temp(popup_id, false);
                                        });
                                    }
                                }
                            });
                    });
            });
        let _ = inner;

        // Close when clicking anywhere outside the input container and the popup
        let popup_rect = inner.response.rect;
        let clicked_outside = ui.input(|i| {
            i.pointer.any_click()
                && i.pointer
                    .latest_pos()
                    .map_or(false, |p| !popup_rect.contains(p) && !container_rect.contains(p))
        });
        if clicked_outside {
            ui.memory_mut(|m| {
                m.data.insert_temp(popup_id, false);
            });
        }
    }

    (mode_change, pick)
}
