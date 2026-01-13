use eframe::egui::{self, Color32, Rounding, Stroke};

/// Sticky hover overlay: opens while pointer is over trigger_rect or overlay itself.
/// Persists via memory temp bool keyed by (ns, "overlay", id).
pub fn show_sticky_overlay<F>(
    ui: &mut egui::Ui,
    trigger_rect: egui::Rect,
    id_ns: (&'static str, u64),
    y_offset: f32,
    inner_margin: f32,
    overlay_ui: F,
) where
    F: FnOnce(&mut egui::Ui),
{
    let popup_id: egui::Id = egui::Id::new((id_ns.0, "overlay", id_ns.1));
    let mut is_open = ui
        .memory(|m| m.data.get_temp::<bool>(popup_id))
        .unwrap_or(false);

    // Open when pointer is over the trigger rect
    let pointer_pos_now = ui.input(|i| i.pointer.hover_pos());
    let over_trigger_now = pointer_pos_now.map_or(false, |p| trigger_rect.contains(p));
    if over_trigger_now {
        is_open = true;
    }

    if is_open {
        let popup_pos = egui::pos2(trigger_rect.min.x, trigger_rect.min.y - y_offset);
        let inner = egui::Area::new(popup_id)
            .order(egui::Order::Foreground)
            .fixed_pos(popup_pos)
            .show(ui.ctx(), |ui| {
                egui::Frame::default()
                    .fill(Color32::from_rgb(28, 28, 28))
                    .stroke(Stroke::new(1.0, Color32::from_gray(60)))
                    .rounding(Rounding::same(crate::ui_constants::card::STATS_ROUNDING))
                    .inner_margin(inner_margin)
                    .show(ui, |ui| overlay_ui(ui));
            });
        let pointer_pos = ui.input(|i| i.pointer.hover_pos());
        let over_overlay = pointer_pos.map_or(false, |p| inner.response.rect.contains(p));
        if !(over_trigger_now || over_overlay) {
            is_open = false;
        }
    }

    ui.memory_mut(|m| {
        m.data.insert_temp(popup_id, is_open);
    });
}

/// Common popup area with consistent styling (Area + Frame + width),
/// returns Area::show inner response so callers can use `inner.response.rect`.
pub fn show_popup_area<F>(
    ui: &egui::Ui,
    popup_id: egui::Id,
    pos: egui::Pos2,
    popup_width: f32,
    border_color: egui::Color32,
    rounding: egui::Rounding,
    content: F,
) -> egui::InnerResponse<egui::InnerResponse<()>>
where
    F: FnOnce(&mut egui::Ui),
{
    egui::Area::new(popup_id)
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ui.ctx(), |ui| {
            egui::Frame::default()
                .fill(Color32::from_rgb(28, 28, 28))
                .stroke(Stroke::new(1.0, border_color))
                .rounding(rounding)
                .show(ui, |ui| {
                    ui.set_min_width(popup_width);
                    content(ui);
                })
        })
}

pub fn clicked_outside(ui: &egui::Ui, avoid_rects: &[egui::Rect]) -> bool {
    ui.input(|i| {
        i.pointer.any_click()
            && i.pointer
                .latest_pos()
                .map_or(false, |p| !avoid_rects.iter().any(|r| r.contains(p)))
    })
}
