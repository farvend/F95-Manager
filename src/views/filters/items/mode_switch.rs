use eframe::egui::{
    self, text::LayoutJob, Color32, FontId, PointerButton, RichText, Sense, TextFormat, Ui,
};
use strum::{EnumCount, IntoEnumIterator};

use crate::views::filters::EnumWithAlternativeNames;

/// Stateless header-like mode switcher:
/// - Left: title (weak)
/// - Right: clickable enum variants in uppercase, separated by " / "
/// Returns Some(new_mode) if user changed it this frame.
pub fn mode_switch<T>(ui: &mut Ui, name: &str, current: &T) -> Option<T>
where
    T: IntoEnumIterator + EnumCount + PartialEq + Clone + ToString,
{
    let mut changed_to: Option<T> = None;

    ui.horizontal(|ui| {
        // Title on the left (weak)
        ui.add(egui::Label::new(RichText::new(name).weak()).selectable(false));

        // Modes on the right
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let variants: Vec<T> = T::iter().collect();
            if variants.is_empty() {
                return;
            }

            // Styling
            let accent = Color32::from_rgb(210, 85, 85);
            let inactive = Color32::from_gray(140);
            let slash_col = Color32::from_rgb(214, 120, 120);
            let font = FontId::proportional(14.0);

            // Build multi-style text: "CREATOR / TITLE"
            let mut job = LayoutJob::default();
            for (i, v) in variants.iter().enumerate() {
                let is_active = *v == *current;
                let color = if is_active { accent } else { inactive };
                let txt = v.to_string().to_uppercase();

                job.append(
                    &txt,
                    0.0,
                    TextFormat {
                        font_id: font.clone(),
                        color,
                        ..Default::default()
                    },
                );
                if i + 1 < variants.len() {
                    job.append(
                        " / ",
                        0.0,
                        TextFormat {
                            font_id: font.clone(),
                            color: slash_col,
                            ..Default::default()
                        },
                    );
                }
            }

            let response = ui
                .add(egui::Label::new(job).sense(Sense::click()).selectable(false))
                .on_hover_cursor(eframe::egui::CursorIcon::PointingHand);

            // Click to cycle
            if response.clicked_by(PointerButton::Primary) {
                let idx = variants.iter().position(|x| x == current).unwrap_or(0);
                let next = (idx + 1) % variants.len();
                changed_to = Some(variants[next].clone());
            } else if response.clicked_by(PointerButton::Secondary) {
                let idx = variants.iter().position(|x| x == current).unwrap_or(0);
                let prev = (idx + variants.len() - 1) % variants.len();
                changed_to = Some(variants[prev].clone());
            }
        });
    });

    changed_to
}

pub fn mode_switch_small<T>(ui: &mut Ui, name: &str, current: &T) -> Option<T>
where
    T: IntoEnumIterator + EnumCount + PartialEq + Clone + EnumWithAlternativeNames,
{
    let mut changed_to: Option<T> = None;

    ui.horizontal(|ui| {
        // Title on the left (weak)
        ui.add(egui::Label::new(RichText::new(name).weak()).selectable(false));

        // Modes on the right (smaller font)
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let variants: Vec<T> = T::iter().collect();
            if variants.is_empty() {
                return;
            }

            // Smaller styling
            let accent = Color32::from_rgb(210, 85, 85);
            let inactive = Color32::from_gray(140);
            let slash_col = Color32::from_rgb(214, 120, 120);
            let font = FontId::proportional(12.0);

            let mut job = LayoutJob::default();
            for (i, v) in variants.iter().enumerate() {
                let is_active = *v == *current;
                let color = if is_active { accent } else { inactive };
                let txt = v.alternative_name().to_uppercase();

                job.append(
                    &txt,
                    0.0,
                    TextFormat {
                        font_id: font.clone(),
                        color,
                        ..Default::default()
                    },
                );
                if i + 1 < variants.len() {
                    job.append(
                        " / ",
                        0.0,
                        TextFormat {
                            font_id: font.clone(),
                            color: slash_col,
                            ..Default::default()
                        },
                    );
                }
            }

            let response = ui
                .add(egui::Label::new(job).sense(Sense::click()).selectable(false))
                .on_hover_cursor(eframe::egui::CursorIcon::PointingHand);

            // Click to cycle (primary forward, secondary backward)
            if response.clicked_by(PointerButton::Primary) {
                let idx = variants.iter().position(|x| x == current).unwrap_or(0);
                let next = (idx + 1) % variants.len();
                changed_to = Some(variants[next].clone());
            } else if response.clicked_by(PointerButton::Secondary) {
                let idx = variants.iter().position(|x| x == current).unwrap_or(0);
                let prev = (idx + variants.len() - 1) % variants.len();
                changed_to = Some(variants[prev].clone());
            }
        });
    });

    changed_to
}
