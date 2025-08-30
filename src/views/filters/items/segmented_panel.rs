use eframe::egui::{Align2, Color32, FontId, Rect, RichText, Rounding, Stroke, Ui, Vec2};
use strum::{EnumCount, IntoEnumIterator};

/// Stateless segmented panel:
/// - Header row: name on the left, current value on the right.
/// - Below: clickable segments for all enum variants with icons from `alternative_name()`.
/// Returns Some(new_variant) when user clicked a segment this frame.
pub fn segmented_panel<T>(ui: &mut Ui, name: &str, current: &mut T) -> bool
where
    T: IntoEnumIterator
        + EnumCount
        + crate::views::filters::EnumWithAlternativeNames
        + crate::views::filters::LocalizableName
        + PartialEq
        + Clone
        + ToString,
{
    // Header: label left, current value right
    ui.horizontal(|ui| {
        ui.add(eframe::egui::Label::new(RichText::new(crate::localization::translate(name)).weak()).selectable(false));
        ui.with_layout(eframe::egui::Layout::right_to_left(eframe::egui::Align::Center), |ui| {
            ui.add(eframe::egui::Label::new(RichText::new(crate::localization::translate(current.loc_key()))).selectable(false));
        });
    });

    let count = T::COUNT as usize;

    // Visuals
    let available_width = ui.available_width();
    let height = (ui.spacing().interact_size.y * 1.4).clamp(28.0, 40.0);
    let rounding = Rounding::same(6.0);
    let border_color = Color32::from_gray(80);
    let bg = Color32::from_rgb(30, 30, 30);
    let accent = Color32::from_rgb(210, 85, 85);

    let (container_rect, _) =
        ui.allocate_exact_size(Vec2::new(available_width, height), eframe::egui::Sense::hover());
    let painter = ui.painter();
    painter.rect(container_rect, rounding, bg, Stroke::new(1.0, border_color));

    let seg_w = container_rect.width() / count as f32;

    let mut changed = false;

    for (i, variant) in T::iter().enumerate() {
        let seg_min = container_rect.min + Vec2::new(i as f32 * seg_w, 0.0);
        let seg_rect = Rect::from_min_size(seg_min, Vec2::new(seg_w, container_rect.height()));
        let is_selected = *current == variant;

        // Vertical separator line
        let x = seg_rect.min.x;
        painter.line_segment(
            [
                eframe::egui::pos2(x, seg_rect.min.y + 4.0),
                eframe::egui::pos2(x, seg_rect.max.y - 4.0),
            ],
            Stroke::new(1.0, Color32::from_gray(60)),
        );

        // Interactivity
        let id = ui.id().with("segmented_panel").with(i as i64);
        let response = ui
            .interact(seg_rect, id, eframe::egui::Sense::click())
            .on_hover_cursor(eframe::egui::CursorIcon::PointingHand);

        // Background highlight for selected/hovered
        if is_selected {
            painter.rect(
                seg_rect.shrink2(Vec2::new(2.0, 2.0)),
                Rounding::same(4.0),
                Color32::from_rgb(45, 45, 45),
                Stroke::NONE,
            );
        } else if response.hovered() {
            painter.rect(
                seg_rect.shrink2(Vec2::new(2.0, 2.0)),
                Rounding::same(4.0),
                Color32::from_rgba_premultiplied(255, 255, 255, 6),
                Stroke::NONE,
            );
        }

        // Icon/text for the variant
        let icon_color = if is_selected {
            accent
        } else {
            Color32::from_rgb(214, 120, 120)
        };
        painter.text(
            seg_rect.center(),
            Align2::CENTER_CENTER,
            variant.alternative_name(),
            FontId::proportional(16.0),
            icon_color,
        );

        if response.clicked() {
            if *current != variant {
                *current = variant;
                changed = true;
            }
        }
    }

    changed
}
