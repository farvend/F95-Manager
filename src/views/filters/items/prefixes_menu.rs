use eframe::egui::{
    pos2, Area, Color32, Frame, Id, Order, Rounding, Sense, Stroke, Ui, Vec2, TextEdit, ScrollArea,
};

use crate::tags::TAGS;

/// Dynamic prefixes picker (for F95 prefixes) with inline search and dropdown popup.
/// Returns Some(prefix_id) when user picks a prefix; otherwise None.
/// Currently lists prefixes for the "games" category (which is what the app queries).
pub fn prefixes_picker(ui: &mut Ui, key: &str, placeholder: &str) -> Option<u32> {
    // Visual constants consistent with tags picker
    let rounding = Rounding::same(6.0);
    let border_color = Color32::from_gray(80);
    let container_bg = Color32::from_rgb(30, 30, 30);
    let hover_bg = Color32::from_rgba_premultiplied(255, 255, 255, 6);
    let accent = Color32::from_rgb(210, 85, 85);

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
    let _ = painter;

    // Search state
    let search_id: Id = Id::new(("prefixes_picker_search", key));
    let mut q = ui
        .memory(|m| m.data.get_temp::<String>(search_id))
        .unwrap_or_default();

    // Inline TextEdit inside the container (reserve a bit of space for the arrow)
    let inner_rect = container_rect.shrink2(Vec2::new(12.0, 6.0));
    let arrow_space = 18.0;
    let edit_rect = eframe::egui::Rect::from_min_max(
        inner_rect.min,
        pos2(inner_rect.max.x - arrow_space, inner_rect.max.y),
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

    // Arrow clickable area and caret drawing
    let cx = container_rect.right() - 14.0;
    let cy = container_rect.center().y + 1.0;
    let w = 8.0;
    let h = 5.0;
    let arrow_rect = eframe::egui::Rect::from_center_size(pos2(cx, cy), Vec2::new(18.0, 16.0));
    let arrow_resp = ui
        .interact(
            arrow_rect,
            ui.id().with("prefixes_picker_arrow").with(key),
            Sense::click(),
        )
        .on_hover_cursor(eframe::egui::CursorIcon::PointingHand);

    // Open/close popup state
    let popup_id: Id = Id::new(("prefixes_picker_popup", key));
    let mut is_open = ui
        .memory(|m| m.data.get_temp::<bool>(popup_id))
        .unwrap_or(false);

    if arrow_resp.clicked() {
        is_open = !is_open;
    } else if response.clicked() {
        if is_open {
            // close when open and clicking container (non-input area)
            is_open = false;
        } else {
            // open and focus the input
            is_open = true;
            if let Some(id) = edit_response.as_ref().map(|r| r.id) {
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

    // Popup with dynamic prefixes list
    let mut pick: Option<u32> = None;
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

                        let ql = q.to_lowercase();

                        // Build and sort items by name from TAGS.prefixes.games
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
                        items.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));

                        ScrollArea::vertical()
                            .max_height(240.0)
                            .show(ui, |ui| {
                                ui.set_width(popup_width - 8.0);
                                for (id, name) in items {
                                    let row_height = ui.spacing().interact_size.y * 1.2;
                                    let (row_rect, row_resp) = ui.allocate_exact_size(
                                        Vec2::new(ui.available_width(), row_height),
                                        Sense::click(),
                                    );
                                    let row_p = ui.painter();

                                    if row_resp.hovered() {
                                        row_p.rect(
                                            row_rect.shrink2(Vec2::new(2.0, 2.0)),
                                            Rounding::same(4.0),
                                            hover_bg,
                                            Stroke::NONE,
                                        );
                                    }

                                    row_p.text(
                                        pos2(row_rect.left() + 8.0, row_rect.center().y),
                                        eframe::egui::Align2::LEFT_CENTER,
                                        &name,
                                        eframe::egui::FontId::proportional(14.0),
                                        Color32::from_gray(210),
                                    );

                                    let row_resp = row_resp.on_hover_cursor(eframe::egui::CursorIcon::PointingHand);
                                    if row_resp.clicked() {
                                        pick = Some(id);
                                        ui.memory_mut(|m| {
                                            m.data.insert_temp(popup_id, false);
                                        });
                                    }
                                }
                            });
                    });
            });

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

    pick
}
