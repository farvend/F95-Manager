use eframe::egui::{self as egui, pos2, Color32, Id, Key, Modifiers, Rounding, Sense, Stroke, TextEdit, Ui, Vec2, ScrollArea};

/// Generic dropdown picker with inline search and popup list.
/// Supply a stable `id_prefix` to keep state separated across different pickers,
/// and a function `get_items(query)` that returns a list of (id, label).
/// Returns Some(id) when the user picks an item, otherwise None.
pub fn dropdown_picker<F>(
    ui: &mut Ui,
    key: &str,
    placeholder: &str,
    id_prefix: &'static str,
    get_items: F,
) -> Option<u32>
where
    F: Fn(&str) -> Vec<(u32, String)>,
{
    // Visual constants shared by pickers
    let rounding = Rounding::same(crate::ui_constants::card::STATS_ROUNDING);
    let border_color = Color32::from_gray(80);
    let container_bg = Color32::from_rgb(30, 30, 30);
    let hover_bg = Color32::from_rgba_premultiplied(255, 255, 255, 6);
    let accent = Color32::from_rgb(210, 85, 85);

    // Dropdown container
    let available_width = ui.available_width();
    let height = (ui.spacing().interact_size.y * 1.4).clamp(28.0, 40.0);
    let (container_rect, response) =
        ui.allocate_exact_size(Vec2::new(available_width, height), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
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
            Rounding::same(crate::ui_constants::card::STATS_ROUNDING),
            hover_bg,
            Stroke::NONE,
        );
    }

    // Search state
    let search_id: Id = Id::new((id_prefix, "search", key));
    let mut q = ui
        .memory(|m| m.data.get_temp::<String>(search_id))
        .unwrap_or_default();

    // Selected index for keyboard navigation
    let sel_id: Id = Id::new((id_prefix, "sel", key));
    let mut sel_idx: usize = ui
        .memory(|m| m.data.get_temp::<usize>(sel_id))
        .unwrap_or(0);

    // Inline TextEdit inside the container (reserve a bit of space for the arrow)
    let inner_rect = container_rect.shrink2(Vec2::new(12.0, crate::ui_constants::card::STATS_MARGIN_V));
    let arrow_space = 18.0;
    let edit_rect = egui::Rect::from_min_max(
        inner_rect.min,
        pos2(inner_rect.max.x - arrow_space, inner_rect.max.y),
    );
    let mut edit_response: Option<egui::Response> = None;
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
    let arrow_rect = egui::Rect::from_center_size(pos2(cx, cy), Vec2::new(18.0, 16.0));
    let arrow_resp = ui
        .interact(
            arrow_rect,
            ui.id().with(format!("{}_arrow", id_prefix)).with(key),
            Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);

    // Open/close popup state
    let popup_id: Id = Id::new((id_prefix, "popup", key));
    let mut is_open = ui
        .memory(|m| m.data.get_temp::<bool>(popup_id))
        .unwrap_or(false);

    if arrow_resp.clicked() {
        is_open = !is_open;
        if is_open { sel_idx = 0; }
    } else if response.clicked() {
        if is_open {
            // close when open and clicking container (non-input area)
            is_open = false;
        } else {
            // open and focus the input
            is_open = true;
            sel_idx = 0;
            if let Some(id) = edit_response.as_ref().map(|r| r.id) {
                ui.memory_mut(|m| m.request_focus(id));
            }
        }
    }
    if let Some(r) = &edit_response {
        if r.clicked() || r.has_focus() || r.changed() {
            if r.changed() { sel_idx = 0; }
            is_open = true;
        }
    }
    ui.memory_mut(|m| {
        m.data.insert_temp(popup_id, is_open);
        m.data.insert_temp(sel_id, sel_idx);
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

    // Popup with dynamic list
    let mut pick: Option<u32> = None;
    if is_open {
        let popup_pos = pos2(container_rect.left(), container_rect.bottom() + crate::ui_constants::spacing::SMALL);
        let popup_width = container_rect.width();

        // Build and sort items by name (based on current query)
        let mut items: Vec<(u32, String)> = get_items(&q);
        items.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
        if items.is_empty() { sel_idx = 0; }
        if sel_idx >= items.len() { sel_idx = items.len().saturating_sub(1); }

        // Keyboard navigation while popup is open
        let (down, up, enter, esc) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if down {
            if !items.is_empty() {
                sel_idx = (sel_idx + 1).min(items.len().saturating_sub(1));
            }
        }
        if up {
            if !items.is_empty() {
                sel_idx = sel_idx.saturating_sub(1);
            }
        }
        if enter {
            if !items.is_empty() {
                pick = Some(items[sel_idx].0);
                // Close popup and clear input
                ui.memory_mut(|m| {
                    m.data.insert_temp(popup_id, false);
                    m.data.insert_temp(search_id, String::new());
                    m.data.insert_temp(sel_id, 0usize);
                });
                q.clear();
            }
        }
        if esc {
            // Close popup on Escape without clearing the input
            ui.memory_mut(|m| {
                m.data.insert_temp(popup_id, false);
            });
        }
        // Persist selection index updates
        ui.memory_mut(|m| { m.data.insert_temp(sel_id, sel_idx); });

        let inner = crate::views::ui_helpers::show_popup_area(
            ui,
            popup_id,
            popup_pos,
            popup_width,
            border_color,
            rounding,
            |ui| {
                ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        ui.set_width(popup_width - crate::ui_constants::spacing::MEDIUM);
                        for (i, (id, name)) in items.iter().enumerate() {
                            let row_height = ui.spacing().interact_size.y * 1.2;
                            let (row_rect, row_resp) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), row_height),
                                Sense::click(),
                            );
                            let row_p = ui.painter();

                            // Highlight hovered or keyboard-selected row
                            if row_resp.hovered() || i == sel_idx {
                                row_p.rect(
                                    row_rect.shrink2(Vec2::new(2.0, 2.0)),
                                    Rounding::same(crate::ui_constants::card::STATS_ROUNDING),
                                    hover_bg,
                                    Stroke::NONE,
                                );
                            }

                            row_p.text(
                                pos2(row_rect.left() + crate::ui_constants::spacing::MEDIUM, row_rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                name,
                                egui::FontId::proportional(14.0),
                                Color32::from_gray(210),
                            );

                            let row_resp = row_resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                            if row_resp.hovered() {
                                // sync keyboard selection with mouse hover for intuitiveness
                                ui.memory_mut(|m| { m.data.insert_temp(sel_id, i); });
                                sel_idx = i;
                            }
                            if row_resp.clicked() {
                                pick = Some(*id);
                                // Close popup and clear input
                                ui.memory_mut(|m| {
                                    m.data.insert_temp(popup_id, false);
                                    m.data.insert_temp(search_id, String::new());
                                    m.data.insert_temp(sel_id, 0usize);
                                });
                            }
                        }
                    });
            }
        );

        // Close when clicking anywhere outside the input container and the popup
        let popup_rect = inner.response.rect;
        let clicked_outside = crate::views::ui_helpers::clicked_outside(ui, &[popup_rect, container_rect]);
        if clicked_outside {
            ui.memory_mut(|m| {
                m.data.insert_temp(popup_id, false);
            });
        }
    }

    pick
}
