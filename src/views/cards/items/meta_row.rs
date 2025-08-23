use eframe::egui::{self, Color32, RichText};

use crate::parser::F95Thread;

/// Draws a single-line meta row: date, likes, views, rating.
pub fn draw_meta_row(ui: &mut egui::Ui, t: &F95Thread) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let col = Color32::from_rgb(170, 170, 170);

        ui.label(RichText::new(format!("🕓 {}", t.date)).small().color(col));
        ui.label(RichText::new(format!("👍 {}", t.likes)).small().color(col));
        ui.label(RichText::new(format!("👀 {}", t.views)).small().color(col));
        ui.label(RichText::new(format!("⭐ {:.1}", t.rating)).small().color(col));
    });
}
