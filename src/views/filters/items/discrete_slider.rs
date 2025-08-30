use eframe::egui::{self, pos2, Color32, Rect, RichText, Rounding, Stroke, Ui, Vec2};

/// Stateless discrete slider that can only take values from a provided list.
/// Header row: name on the left, active value on the right.
/// Below: a track with a draggable thumb that snaps to the nearest allowed value.
/// Returns Some(new_value) if changed by user interaction this frame.
pub fn discrete_slider<T>(ui: &mut Ui, name: &str, current: &T, values: &[T]) -> Option<T>
where
    T: PartialEq + Clone + ToString + crate::views::filters::LocalizableName,
{
    // Header: label left, current value right
    ui.horizontal(|ui| {
        ui.add(egui::Label::new(RichText::new(name).weak()).selectable(false));
        ui.with_layout(
            eframe::egui::Layout::right_to_left(eframe::egui::Align::Center),
            |ui| {
                ui.add(egui::Label::new(RichText::new(crate::localization::translate(current.loc_key()))).selectable(false));
            },
        );
    });

    let count = values.len();
    if count == 0 {
        return None;
    }
    let current_idx = values.iter().position(|v| *v == *current).unwrap_or(0);

    // Visual constants (match segmented_panel vibe)
    let available_width = ui.available_width();
    let height = (ui.spacing().interact_size.y * 1.4).clamp(28.0, 40.0);
    let rounding = Rounding::same(6.0);
    let border_color = Color32::from_gray(80);
    let container_bg = Color32::from_rgb(30, 30, 30);
    let track_bg = Color32::from_rgb(25, 25, 25);
    let track_border = Color32::from_gray(60);
    let thumb_fill = Color32::from_rgb(52, 52, 52);
    let thumb_outline = Color32::from_gray(50);
    let accent = Color32::from_rgb(210, 85, 85);

    let (container_rect, _) =
        ui.allocate_exact_size(Vec2::new(available_width, height), eframe::egui::Sense::hover());
    let painter = ui.painter();
    painter.rect(
        container_rect,
        rounding,
        container_bg,
        Stroke::new(1.0, border_color),
    );

    // Track in the middle of the container
    let track_height = 8.0f32;
    let track_margin_h = 16.0f32; // left/right margin inside container
    let track_rect = Rect::from_min_max(
        pos2(
            container_rect.min.x + track_margin_h,
            container_rect.center().y - track_height * 0.5,
        ),
        pos2(
            container_rect.max.x - track_margin_h,
            container_rect.center().y + track_height * 0.5,
        ),
    );
    painter.rect(
        track_rect,
        Rounding::same(track_height * 0.5),
        track_bg,
        Stroke::new(1.0, track_border),
    );

    // Thumb size and position according to current index
    let thumb_size = Vec2::new(26.0, (height - 10.0).clamp(18.0, 28.0));
    let t_cur = if count > 1 {
        current_idx as f32 / (count as f32 - 1.0)
    } else {
        0.0
    };
    let thumb_x = egui::lerp(track_rect.left()..=track_rect.right(), t_cur);
    let thumb_center = pos2(thumb_x, container_rect.center().y);
    let mut thumb_rect = Rect::from_center_size(thumb_center, thumb_size);

    // Interact on the whole container (track + thumb) so both drag/click will work
    let id = ui.id().with("discrete_slider").with(name.to_string()); // unique per name
    let response = ui
        .interact(
            container_rect,
            id,
            eframe::egui::Sense::click_and_drag(),
        )
        .on_hover_cursor(eframe::egui::CursorIcon::PointingHand);

    // Compute new value on click/drag
    let mut changed_to: Option<T> = None;
    if (response.clicked() || response.dragged()) && count > 1 {
        if let Some(pointer) = response.interact_pointer_pos() {
            // Convert pointer.x on track to nearest step
            let x = pointer.x.clamp(track_rect.left(), track_rect.right());
            let t = if track_rect.width() > 0.0 {
                (x - track_rect.left()) / track_rect.width()
            } else {
                0.0
            }
            .clamp(0.0, 1.0);

            let new_idx = (t * (count as f32 - 1.0)).round() as usize;
            if new_idx != current_idx {
                changed_to = Some(values[new_idx].clone());
            }
            // Update thumb preview while dragging (visual only this frame)
            let new_x =
                egui::lerp(track_rect.left()..=track_rect.right(), new_idx as f32 / (count as f32 - 1.0));
            thumb_rect = Rect::from_center_size(pos2(new_x, thumb_center.y), thumb_size);
        }
    }

    // Hover feedback
    let hovered = response.hovered();
    let thumb_fill_col = if hovered {
        Color32::from_rgb(
            thumb_fill.r().saturating_add(6),
            thumb_fill.g().saturating_add(6),
            thumb_fill.b().saturating_add(6),
        )
    } else {
        thumb_fill
    };

    // Draw thumb
    painter.rect(
        thumb_rect,
        Rounding::same(4.0),
        thumb_fill_col,
        Stroke::new(1.0, thumb_outline),
    );

    // Two vertical grip lines (like on the screenshot)
    let grip_top = thumb_rect.center_top().y + 6.0;
    let grip_bottom = thumb_rect.center_bottom().y - 6.0;
    let grip1_x = thumb_rect.center().x - 3.0;
    let grip2_x = thumb_rect.center().x + 3.0;
    let grip_col = Color32::from_gray(80);
    painter.line_segment([pos2(grip1_x, grip_top), pos2(grip1_x, grip_bottom)], Stroke::new(1.0, grip_col));
    painter.line_segment([pos2(grip2_x, grip_top), pos2(grip2_x, grip_bottom)], Stroke::new(1.0, grip_col));

    // Optional: small ticks for steps (subtle)
    if count > 1 {
        let tick_col = Color32::from_rgba_premultiplied(255, 255, 255, 18);
        for i in 0..count {
            let ti = i as f32 / (count as f32 - 1.0);
            let tx = egui::lerp(track_rect.left()..=track_rect.right(), ti);
            let y1 = track_rect.center().y - 5.0;
            let y2 = track_rect.center().y + 5.0;
            painter.line_segment([pos2(tx, y1), pos2(tx, y2)], Stroke::new(1.0, tick_col));
        }
    }

    // Active accent on track up to thumb (subtle fill)
    if count > 1 {
        let active_rect = Rect::from_min_max(
            pos2(track_rect.left(), track_rect.top()),
            pos2(thumb_rect.center().x, track_rect.bottom()),
        );
        painter.rect(
            active_rect,
            Rounding::same(track_height * 0.5),
            Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 26),
            Stroke::NONE,
        );
    }

    changed_to
}
