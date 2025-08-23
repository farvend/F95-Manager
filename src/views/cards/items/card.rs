use eframe::egui::{self, Color32, RichText, Rounding, Stroke};

use crate::parser::F95Thread;
use crate::app::settings::{hide_thread, is_thread_hidden, is_pending_download, remove_pending_download, open_in_browser, delete_downloaded_game, downloaded_game_folder, reveal_in_file_manager};
// use crate::views::cards::items::cover_hover::CoverHover;
use super::cover_hover::draw_cover;
use super::tags_panel::draw_tags_panel;
use super::meta_row::draw_meta_row;

/// Hover info returned by thread_card so the caller can lazy-load screenshots.
pub struct CardHover {
    pub hovered: bool,
    pub hovered_line: Option<usize>,
    pub download_clicked: bool
}

/// Fixed-width card resembling F95 tiles.
/// Strictly constrained to `width` so rows form a proper grid.
/// - `cover`: main cover texture (optional)
/// - `screens`: already loaded screenshots for this thread by index (optional, sparse via Option)
pub fn thread_card(
    ui: &mut egui::Ui,
    t: &F95Thread,
    width: f32,
    cover_tex: Option<&egui::TextureHandle>,
    screens: Option<&[Option<egui::TextureHandle>]>,
    download_progress: Option<f32>,
) -> CardHover {
    let rounding = Rounding::same(8.0);
    let fill = Color32::from_rgb(36, 36, 36);
    let stroke = Stroke::new(1.0, Color32::from_rgb(64, 64, 64));

    // Hard limit the card width inside the row.
    ui.set_min_width(width);
    ui.set_max_width(width);

    let mut hovered_line: Option<usize> = None;
    let mut hovered_any = false;

    // If tags panel was open on previous frame, make bottom corners square to merge seamlessly.
    let open_id = egui::Id::new(("card_tags_open", t.thread_id));
    let was_open = ui
        .ctx()
        .memory(|m| m.data.get_temp::<bool>(open_id))
        .unwrap_or(false);
    let card_rounding = if was_open {
        Rounding {
            nw: 8.0,
            ne: 8.0,
            sw: 0.0,
            se: 0.0,
        }
    } else {
        rounding
    };
    let mut download_clicked = false;

    let frame_out = egui::Frame::none()
        .fill(fill)
        .stroke(stroke)
        .rounding(card_rounding)
        .inner_margin(egui::Margin::symmetric(8.0, 8.0))
        .show(ui, |ui| {
            // Constrain inner content to card width minus inner margins (8 + 8).
            let inner_w = width - 16.0;
            ui.set_width(inner_w);

            // Cover + markers (handles hover index and screenshot swap if available)
            let cover_hover = draw_cover(ui, t, inner_w, cover_tex, screens, download_progress);
            hovered_any |= cover_hover.hovered;
            hovered_line = cover_hover.hovered_line;
            download_clicked |= cover_hover.download_clicked;

            // Title (after cover and markers)
            // Use a fixed post-cover gap to avoid data-driven layout hacks.
            // Combined with markers area height from draw_cover this ensures consistent spacing.
            // Fixed gap after cover; independent of data and width.
            ui.add_space(20.0);
            ui.label(RichText::new(&t.title).heading().color(Color32::from_rgb(230, 230, 230)));

            // Creator under title (no background) to match main page layout
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("by {}", t.creator))
                    .small()
                    .color(Color32::from_rgb(180, 180, 180)),
            );
            ui.add_space(4.0);

            // Stats plaque (meta row) on a dark rounded background, below the creator
            egui::Frame::none()
                .fill(Color32::from_rgba_premultiplied(28, 28, 28, 180))
                .rounding(Rounding::same(6.0))
                .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                .show(ui, |ui| {
                    // Meta row: time (date), likes, views, rating — single line
                    draw_meta_row(ui, t);
                });
        });

    hovered_any |= frame_out.response.hovered();

    // Right-click context menu (ПКМ)
    frame_out.response.context_menu(|ui| {
        let thread_id = t.thread_id.get();
        let is_hidden = is_thread_hidden(thread_id);
        let is_downloaded = downloaded_game_folder(thread_id).is_some();
        let is_pending = is_pending_download(thread_id);
        let is_downloading = download_progress.is_some();

        // Hide (if not already hidden)
        if !is_hidden {
            if ui.button("Hide").clicked() {
                hide_thread(thread_id);
                ui.ctx().request_repaint();
                ui.close_menu();
            }
        }

        // Open thread in default browser
        if ui.button("Open in F95").clicked() {
            let url = t.thread_id.get_page().0.to_string();
            open_in_browser(&url);
            ui.close_menu();
        }

        // Remove pending entry (not downloading, not downloaded)
        if is_pending && !is_downloading && !is_downloaded {
            if ui.button("Remove from Library").clicked() {
                remove_pending_download(thread_id);
                ui.ctx().request_repaint();
                ui.close_menu();
            }
        }

        // If downloaded: allow deleting and opening folder
        if is_downloaded {
            if ui.button("Delete").clicked() {
                delete_downloaded_game(thread_id);
                ui.ctx().request_repaint();
                ui.close_menu();
            }
            if ui.button("Open folder").clicked() {
                if let Some(folder) = downloaded_game_folder(thread_id) {
                    reveal_in_file_manager(&folder);
                }
                ui.close_menu();
            }
        }
    });

    // Floating tags drop-down below the card: absolute area so it doesn't push layout.
    let card_rect = frame_out.response.rect;
    let (_is_open, area_hovered) =
        draw_tags_panel(ui, t, card_rect, hovered_any, fill, stroke, rounding);
    hovered_any |= area_hovered;

    CardHover {
        hovered: hovered_any,
        hovered_line,
        download_clicked
    }
}
