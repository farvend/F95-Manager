use eframe::egui::{self, Color32, Rounding, Stroke, Vec2};

use crate::parser::F95Thread;
use crate::tags::TAGS;

const TAGS_ROUNDING: f32 = 4.;

//// Renders the floating tags panel below the card without affecting layout.
/// - Opens only while `base_hovered` (card hover) is true; moving cursor onto the panel closes it.
/// - Persists only last base hover state for one-frame rounding in the card.
/// - Draws only side + bottom borders to visually merge with the card.
/// Returns: (is_open, area_hovered)
pub fn draw_tags_panel(
    ui: &mut egui::Ui,
    t: &F95Thread,
    card_rect: egui::Rect,
    base_hovered: bool,
    fill: Color32,
    stroke: Stroke,
    rounding: Rounding,
) -> (bool, bool) {
    let open_id = egui::Id::new(("card_tags_open", t.thread_id));
    let mut is_open = base_hovered;
    let mut area_hovered = false;

    egui::Area::new(egui::Id::new(("card_tags", t.thread_id)))
        .order(egui::Order::Foreground)
        .interactable(false)
        .fixed_pos(egui::pos2(card_rect.left(), card_rect.bottom()))
        .show(ui.ctx(), |ui| {
            if is_open {
                ui.set_min_width(card_rect.width());
                ui.set_max_width(card_rect.width());

                let inner = egui::Frame::none()
                    .fill(fill)
                    .stroke(Stroke::NONE) // no top stroke
                    .rounding(Rounding {
                        nw: 0.0,
                        ne: 0.0,
                        sw: rounding.sw.max(12.0),
                        se: rounding.se.max(12.0),
                    })
                    .inner_margin(egui::Margin::symmetric(8.0, 8.0))
                    .show(ui, |ui| {
                        // Layout as fixed-width chips (pills) in wrapped rows
                        let inner_w = ui.available_width();
                        let gap = 5.;
                        // Dynamic sizin.: compute width per tag from text length
                        let pad_x = 5.;
                        let chip_h = 16.;
                        let max_chip_w = inner_w;

                        ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                        ui.horizontal_wrapped(|ui| {
                            for id in &t.tags {
                                let font = egui::FontId::proportional(13.0);
                                let text_color = Color32::from_rgb(245, 245, 245);

                                let text = TAGS.tags
                                    .get(&id.to_string())
                                    .cloned()
                                    .unwrap_or_else(|| id.to_string());

                                // Measure text and compute chip width dynamically with padding
                                let galley = ui.painter().layout_no_wrap(text.clone(), font.clone(), text_color);
                                let mut chip_w = galley.size().x + 2.0 * pad_x;
                                chip_w = chip_w.clamp(32.0, max_chip_w);

                                let (_wid, rect) = ui.allocate_space(Vec2::new(chip_w, chip_h));
                                let p = ui.painter_at(rect);

                                let hovered = ui.input(|i| i.pointer.hover_pos()).map_or(false, |p| rect.contains(p));
                                let bg = if hovered {
                                    Color32::from_rgb(210, 90, 90)
                                } else {
                                    Color32::from_rgb(190, 70, 70)
                                };
                                let border = Color32::from_rgb(180, 80, 80);

                                p.rect_filled(rect, Rounding::same(TAGS_ROUNDING), bg);
                                p.rect_stroke(
                                    rect,
                                    Rounding::same(TAGS_ROUNDING),
                                    Stroke::new(1.5, border),
                                );
                                p.text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    text,
                                    font,
                                    text_color,
                                );
                            }
                        });
                    });

                // Side + bottom borders to match the card border
                let r = inner.response.rect;
                let pp = ui.painter();
                pp.line_segment(
                    [egui::pos2(card_rect.left(), card_rect.bottom()), egui::pos2(card_rect.left(), r.bottom())],
                    stroke,
                );
                pp.line_segment(
                    [egui::pos2(card_rect.right(), card_rect.bottom()), egui::pos2(card_rect.right(), r.bottom())],
                    stroke,
                );
                pp.line_segment(
                    [egui::pos2(card_rect.left(), r.bottom()), egui::pos2(card_rect.right(), r.bottom())],
                    stroke,
                );

                let mouse_pos = ui.input(|i| i.pointer.hover_pos());
                area_hovered = mouse_pos.map_or(false, |p| inner.response.rect.contains(p));
            }
        });

    // Persist only base hover state for previous-frame rounding in card.rs
    ui.ctx().memory_mut(|m| {
        m.data.insert_temp(open_id, base_hovered);
    });

    (is_open, area_hovered)
}
