use eframe::egui::{self, Color32, Label, RichText, Rounding, Sense, Stroke, Vec2};

use crate::{parser::F95Thread, views::cards::items::card::CardHover};
use crate::parser::game_info::link::DownloadLink;
use crate::app::settings as app_settings;

/// Hover info for the cover area (image + markers).
// pub struct CoverHover {
//     pub hovered: bool,
//     pub hovered_line: Option<usize>,
//     pub download_clicked: bool,
// }

/// Draws the cover image with 16:9 ratio across `inner_w` width,
/// shows a version badge, and renders hover markers under the image.
/// - If a screenshot for the hovered marker is preloaded, it is shown instead of the cover.
pub fn draw_cover(
    ui: &mut egui::Ui,
    thread: &F95Thread,
    inner_w: f32,
    cover: Option<&egui::TextureHandle>,
    screens: Option<&[Option<egui::TextureHandle>]>,
    progress: Option<crate::game_download::Progress>,
    link_choices: Option<&[DownloadLink]>,
) -> CardHover {
    let cover_h = inner_w * 9.0 / 16.0;
    let (cover_rect, _cover_resp) =
        ui.allocate_exact_size(Vec2::new(inner_w, cover_h), Sense::hover());

    // Reserve small area under the image for markers (shown on hover).
    // Also serves as a consistent bottom padding below the cover image.
    let markers_h = 12.0;
    let (markers_rect, _markers_resp) =
        ui.allocate_exact_size(Vec2::new(inner_w, markers_h), Sense::hover());

    let pointer = ui.input(|i| i.pointer.hover_pos());
    let over_cover = pointer.map_or(false, |p| cover_rect.contains(p));
    let over_markers = pointer.map_or(false, |p| markers_rect.contains(p));
    let mut hovered = over_cover || over_markers;
    let mut hovered_line: Option<usize> = None;
    let mut download_clicked: bool = false;

    let n = thread.screens.len();
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));

    // If hovered over image or markers, compute nearest marker index by mouse X.
    if n > 0 {
        let mouse_x = pointer.and_then(|p| {
            if cover_rect.contains(p) || markers_rect.contains(p) {
                Some(p.x)
            } else {
                None
            }
        });
        if let Some(mx) = mouse_x {
            let rel_x = (mx - cover_rect.min.x).clamp(0.0, inner_w);
            let seg_w = inner_w / (n as f32);
            let mut idx = (rel_x / seg_w).floor() as usize;
            if idx >= n {
                idx = n - 1;
            }
            hovered_line = Some(idx);
        }
    }

    // Draw cover or an already loaded screenshot corresponding to hovered marker.
    let p = ui.painter_at(cover_rect);
    let mut drew_image = false;
    if let Some(idx) = hovered_line {
        if let Some(s) = screens {
            if let Some(Some(tex)) = s.get(idx) {
                p.image(tex.id(), cover_rect, uv, Color32::WHITE);
                drew_image = true;
            }
        }
    }
    if !drew_image {
        if let Some(tex) = cover {
            p.image(tex.id(), cover_rect, uv, Color32::WHITE);
        } else {
            p.rect_filled(cover_rect, Rounding::same(6.0), Color32::from_rgb(58, 58, 58));
        }
    }

    // Version badge on the image (top-right), width = text width + padding (clamped to image)
    let font_id = egui::TextStyle::Small.resolve(ui.style()).clone();
    let text_color = Color32::from_rgb(200, 200, 200);
    let text_w = ui.fonts(|f| {
        let galley = f.layout_no_wrap(thread.version.clone(), font_id.clone(), text_color);
        galley.rect.width()
    });
    let pad_x = 8.0f32;
    let badge_w = (text_w + pad_x * 2.0).clamp(36.0, inner_w - 16.0);
    let badge_h = 20.0f32;
    let badge_rect = egui::Rect::from_min_max(
        egui::pos2(cover_rect.max.x - 8.0 - badge_w, cover_rect.min.y + 8.0),
        egui::pos2(cover_rect.max.x - 8.0, cover_rect.min.y + 8.0 + badge_h),
    );
    p.rect_filled(badge_rect, Rounding::same(4.0), Color32::from_rgb(54, 54, 54));
    // Clip text to the badge rect and truncate with ellipsis
    ui.allocate_ui_at_rect(badge_rect, |ui| {
        ui.centered_and_justified(|ui| {
            ui.add(
                egui::Label::new(RichText::new(&thread.version).small().color(text_color))
                    .truncate(true)
                    .wrap(false),
            );
        });
    });
 
    // Engine badge (bottom-left) and warnings counter (red square)
    let pad = 8.0;
    let badge_h = 18.0;
    let y0 = cover_rect.max.y - pad - badge_h;
    let mut next_x = cover_rect.min.x + pad;

    // Resolve engine name from prefixes (Engine group)
    let engine_name = crate::views::cards::items::cover_helpers::resolve_engine_name(thread);

    if let Some(name) = &engine_name {
        let font_id = egui::TextStyle::Small.resolve(ui.style()).clone();
        let text_color = Color32::from_rgb(200, 200, 200);
        let text_w = ui.fonts(|f| {
            f.layout_no_wrap(name.clone(), font_id.clone(), text_color).rect.width()
        });
        let pad_x = 8.0f32;
        let w = text_w + pad_x * 2.0;
        let engine_rect = egui::Rect::from_min_max(
            egui::pos2(next_x, y0),
            egui::pos2(next_x + w, y0 + badge_h),
        );
        let painter = ui.painter_at(engine_rect);
        painter.rect_filled(engine_rect, Rounding::same(4.0), Color32::from_rgb(54, 54, 54));
        painter.rect_stroke(engine_rect, Rounding::same(4.0), Stroke::new(1.0, Color32::from_gray(60)));
        ui.allocate_ui_at_rect(engine_rect, |ui| {
            ui.centered_and_justified(|ui| {
                ui.add(
                    egui::Label::new(RichText::new(name.clone()).color(text_color))
                        //.truncate(true)
                        .wrap(false),
                );
            });
        });
        next_x = engine_rect.max.x + 6.0;
    } else {
        //ui.allocate_space(Vec2 { x: next_x, y: cover_h - 30. });
        let w = text_w + pad_x * 2.0;
        let engine_rect = egui::Rect::from_min_max(
            egui::pos2(next_x, y0),
            egui::pos2(w, y0 + badge_h),
        );
        ui.allocate_rect(engine_rect, Sense::hover());
    }

    // Collect warnings (tags + prefixes) and show counter if any
    let (warn_tag_names, warn_pref_names) = crate::views::cards::items::cover_helpers::collect_warnings(thread);
    let warn_count = warn_tag_names.len() + warn_pref_names.len();
    if warn_count > 0 {
        let size = egui::vec2(badge_h, badge_h);
        let warn_rect = egui::Rect::from_min_size(egui::pos2(next_x, y0), size);
        ui.expand_to_include_rect(warn_rect);
        let warn_resp = ui
            .interact(
                warn_rect,
                ui.id().with(("warn_badge", thread.thread_id)),
                Sense::hover(),
            )
            .on_hover_cursor(eframe::egui::CursorIcon::PointingHand);
        let painter = ui.painter_at(warn_rect);
        painter.rect_filled(warn_rect, Rounding::same(4.0), Color32::from_rgb(170, 40, 40));
        painter.rect_stroke(warn_rect, Rounding::same(4.0), Stroke::new(1.0, Color32::from_gray(40)));
        painter.text(
            warn_rect.center(),
            eframe::egui::Align2::CENTER_CENTER,
            warn_count.to_string(),
            eframe::egui::FontId::proportional(12.0),
            Color32::WHITE,
        );

        // Custom overlay plaque above the warning square (sticky while hovered)
        let mut lines: Vec<String> = Vec::new();
        if !warn_tag_names.is_empty() {
            lines.push("Tags:".to_string());
            for n in &warn_tag_names {
                lines.push(format!(" • {}", n));
            }
        }
        if !warn_pref_names.is_empty() {
            if !lines.is_empty() { lines.push("".into()); }
            lines.push("Prefixes:".to_string());
            for n in &warn_pref_names {
                lines.push(format!(" • {}", n));
            }
        }
        // Persist open while hovering square or the plaque itself
        let popup_id: egui::Id = egui::Id::new(("warn_overlay", thread.thread_id));
        let mut is_open = ui.memory(|m| m.data.get_temp::<bool>(popup_id)).unwrap_or(false);
        // Also open when pointer is over the square (more reliable than Response::hovered in some layouts)
        let pointer_pos_now = ui.input(|i| i.pointer.hover_pos());
        let over_square_now = pointer_pos_now.map_or(false, |p| warn_rect.contains(p));
        if warn_resp.hovered() || over_square_now {
            is_open = true;
        }
        if is_open {
            let popup_pos = egui::pos2(warn_rect.min.x, warn_rect.min.y - 6.0);
            let inner = egui::Area::new(popup_id)
                .order(egui::Order::Foreground)
                .fixed_pos(popup_pos)
                .show(ui.ctx(), |ui| {
                    egui::Frame::default()
                        .fill(Color32::from_rgb(28, 28, 28))
                        .stroke(Stroke::new(1.0, Color32::from_gray(60)))
                        .rounding(Rounding::same(6.0))
                        .inner_margin(4.)
                        .show(ui, |ui| {
                            //ui.set_min_width(220.0);
                            for (i, line) in lines.iter().enumerate() {
                                if line.is_empty() {
                                    ui.add_space(4.0);
                                } else {
                                    let mut text = RichText::new(line.clone()).color(Color32::from_gray(220));
                                    if line == "Tags:" || line == "Prefixes:" {
                                        text = text.strong();
                                    }
                                    ui.label(text);
                                }
                                if i + 1 < lines.len() { /* keep compact */ }
                            }
                        });
                });
            let pointer_pos = ui.input(|i| i.pointer.hover_pos());
            let over_square = pointer_pos.map_or(false, |p| warn_rect.contains(p));
            let overlay_rect = inner.response.rect;
            let over_overlay = pointer_pos.map_or(false, |p| overlay_rect.contains(p));
            if !(over_square || over_overlay) {
                is_open = false;
            }
        }
        ui.memory_mut(|m| {
            m.data.insert_temp(popup_id, is_open);
        });
    }

    // Markers (small horizontal dashes) under the image: show only on hover.
    if hovered && n > 0 {
        let painter = ui.painter_at(markers_rect);
        let seg_w = inner_w / (n as f32);
        let dash_len = seg_w - 0.5;
        let y = markers_rect.center().y;
        let col_inactive = Color32::from_rgb(110, 110, 110);
        let col_active = Color32::from_rgb(220, 220, 220);

        for i in 0..n {
            let cx = markers_rect.min.x + (i as f32 + 0.5) * seg_w;
            let color = if hovered_line == Some(i) {
                col_active
            } else {
                col_inactive
            };
            let thick = if hovered_line == Some(i) { 2.0 } else { 1.5 };
            painter.line_segment(
                [
                    egui::pos2(cx - dash_len / 2.0, y),
                    egui::pos2(cx + dash_len / 2.0, y),
                ],
                Stroke::new(thick, color),
            );
        }
    }

    // Download/Run overlay: always register hit-test; paint only when hovered
    let btn_size = egui::vec2(24.0, 24.0);
    let btn_rect = egui::Rect::from_min_size(
        egui::pos2(cover_rect.min.x + 8.0, cover_rect.min.y + 8.0),
        btn_size,
    );
    // ensure UI area includes button for interaction even if outside normal layout
    ui.expand_to_include_rect(btn_rect);
    let resp = ui
        .interact(
            btn_rect,
            ui.id().with(("dl_btn", thread.thread_id)),
            Sense::click(),
        )
        .on_hover_cursor(eframe::egui::CursorIcon::PointingHand);

    if resp.hovered() {
        hovered = true;
    }

    let is_downloaded = app_settings::downloaded_game_folder(thread.thread_id.get())
        .map(|p| p.is_dir())
        .unwrap_or(false);
    let icon = if is_downloaded { "▶" } else { "⬇" };

    if over_cover || resp.hovered() {
        let p = ui.painter_at(btn_rect);
        let bg = if resp.hovered() { Color32::from_gray(72) } else { Color32::from_gray(60) };
        p.rect_filled(btn_rect, Rounding::same(4.0), bg);
        p.rect_stroke(btn_rect, Rounding::same(4.0), Stroke::new(1.0, Color32::from_gray(100)));
        p.text(
            btn_rect.center(),
            eframe::egui::Align2::CENTER_CENTER,
            icon,
            eframe::egui::FontId::proportional(16.0),
            Color32::from_gray(230),
        );
    }

    if resp.clicked() {
        log::info!("cover button clicked for thread {}", thread.thread_id.get());
        if is_downloaded {
            app_settings::run_downloaded_game(thread.thread_id.get());
        } else {
            download_clicked = true;
        }
    } else {
        // manual fallback in case interact misses the click due to overlapping paints
        let manual_click = ui.input(|i| {
            i.pointer.primary_clicked()
                && i.pointer.hover_pos().map_or(false, |pos| btn_rect.contains(pos))
        });
        if manual_click {
            log::info!("manual cover button click for thread {}", thread.thread_id.get());
            if is_downloaded {
                app_settings::run_downloaded_game(thread.thread_id.get());
            } else {
                download_clicked = true;
            }
        }
    }

    // Resolve progress error (if any) to show error badge
    let mut selected_link: Option<DownloadLink> = None;
    let download_error: Option<&str> = match &progress {
        Some(crate::game_download::Progress::Error(s)) => Some(s.as_str()),
        _ => None,
    };

    // Select Link badge (shown when backend requests link selection)
    if let Some(links) = link_choices {
        let font_id = egui::TextStyle::Small.resolve(ui.style()).clone();
        let text_color = Color32::WHITE;
        let label = "SELECT LINK";
        let text_w = ui.fonts(|f| {
            f.layout_no_wrap(label.to_string(), font_id.clone(), text_color)
                .rect
                .width()
        });
        let badge_h = 18.0f32;
        let pad_x = 12.0f32;
        let w = text_w + pad_x * 2.0;
        let pad = 8.0f32;
        let sel_rect = egui::Rect::from_min_max(
            egui::pos2(cover_rect.max.x - pad - w, cover_rect.max.y - pad - badge_h),
            egui::pos2(cover_rect.max.x - pad, cover_rect.max.y - pad),
        );
        ui.expand_to_include_rect(sel_rect);
        let painter = ui.painter_at(sel_rect);
        painter.rect_filled(sel_rect, Rounding::same(4.0), Color32::from_rgb(60, 120, 200));
        painter.rect_stroke(
            sel_rect,
            Rounding::same(4.0),
            Stroke::new(1.0, Color32::from_gray(40)),
        );
        ui.allocate_ui_at_rect(sel_rect, |ui| {
            ui.centered_and_justified(|ui| {
                ui.add(
                    egui::Label::new(RichText::new(label).color(text_color))
                        .truncate(true)
                        .wrap(false),
                );
            });
        });
        // Interactive hover area
        let sel_resp = ui
            .interact(
                sel_rect,
                ui.id().with(("dl_select_badge", thread.thread_id)),
                Sense::hover(),
            )
            .on_hover_cursor(eframe::egui::CursorIcon::PointingHand);
        // Keep overlay open while hovering badge or overlay itself
        let over_sel_now = ui
            .input(|i| i.pointer.hover_pos())
            .map_or(false, |p| sel_rect.contains(p));
        let popup_id: egui::Id = egui::Id::new(("dl_select_overlay", thread.thread_id));
        let mut is_open = ui.memory(|m| m.data.get_temp::<bool>(popup_id)).unwrap_or(false);
        if sel_resp.hovered() || over_sel_now {
            is_open = true;
        }
        if is_open {
            let popup_pos = egui::pos2(sel_rect.min.x, sel_rect.min.y - 6.0);
            let inner = egui::Area::new(popup_id)
                .order(egui::Order::Foreground)
                .fixed_pos(popup_pos)
                .show(ui.ctx(), |ui| {
                    egui::Frame::default()
                        .fill(Color32::from_rgb(28, 28, 28))
                        .stroke(Stroke::new(1.0, Color32::from_gray(60)))
                        .rounding(Rounding::same(6.0))
                        .inner_margin(6.0)
                        .show(ui, |ui| {
                            ui.set_max_width(250.);
                            for link in links.iter() {
                                let label = match link {
                                    crate::parser::game_info::link::DownloadLink::Direct(d) => {
                                        let last = d.path
                                            .iter()
                                            .intersperse(&"/".to_string())
                                            .cloned()
                                            .collect::<String>();
                                        format!("{}/{}", d.hosting.to_string(), last)
                                    }
                                    crate::parser::game_info::link::DownloadLink::Masked(u) => {
                                        format!("{}{}", u.domain().unwrap(), u.path())
                                    }
                                };
                                ui.horizontal(|ui| {
                                    if ui.add(
                                        Label::new(
                                            RichText::new(label)
                                        ).truncate(true)
                                        .selectable(true)
                                        .sense(Sense::click())
                                        .selectable(false)
                                    ).on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked() {
                                        selected_link = Some(link.clone());
                                    }
                                    //ui.label(RichText::new(label).color(Color32::from_gray(220)));
                                    // if ui.button("Download").clicked() {
                                    //     selected_link = Some(link.clone());
                                    // }
                                });
                            }
                        });
                });
            let pointer_pos = ui.input(|i| i.pointer.hover_pos());
            let over_badge = pointer_pos.map_or(false, |p| sel_rect.contains(p));
            let overlay_rect = inner.response.rect;
            let over_overlay = pointer_pos.map_or(false, |p| overlay_rect.contains(p));
            if !(over_badge || over_overlay) {
                is_open = false;
            }
        }
        ui.memory_mut(|m| {
            m.data.insert_temp(popup_id, is_open);
        });
    }

    // Error badge shown when download/unzip error occurs
    if let Some(err) = download_error {
        let font_id = egui::TextStyle::Small.resolve(ui.style()).clone();
        let text_color = Color32::WHITE;
        let label = "ERROR";
        let text_w = ui.fonts(|f| {
            f.layout_no_wrap(label.to_string(), font_id.clone(), text_color)
                .rect
                .width()
        });
        let badge_h = 18.0f32;
        let pad_x = 12.0f32;
        let w = text_w + pad_x * 2.0;
        let pad = 8.0f32;
        let err_rect = egui::Rect::from_min_max(
            egui::pos2(cover_rect.max.x - pad - w, cover_rect.max.y - pad - badge_h),
            egui::pos2(cover_rect.max.x - pad, cover_rect.max.y - pad),
        );
        ui.expand_to_include_rect(err_rect);
        let painter = ui.painter_at(err_rect);
        painter.rect_filled(err_rect, Rounding::same(4.0), Color32::from_rgb(170, 40, 40));
        painter.rect_stroke(
            err_rect,
            Rounding::same(4.0),
            Stroke::new(1.0, Color32::from_gray(40)),
        );
        ui.allocate_ui_at_rect(err_rect, |ui| {
            ui.centered_and_justified(|ui| {
                ui.add(
                    egui::Label::new(RichText::new(label).color(text_color))
                        .truncate(true)
                        .wrap(false),
                );
            });
        });
        // Create interactive hit-test area and cursor for hover
        let err_resp = ui
            .interact(
                err_rect,
                ui.id().with(("dl_error_badge", thread.thread_id)),
                Sense::hover(),
            )
            .on_hover_cursor(eframe::egui::CursorIcon::PointingHand);
        // Hover detection for error badge (both response and raw-rect check)
        let over_err_now = ui
            .input(|i| i.pointer.hover_pos())
            .map_or(false, |p| err_rect.contains(p));
        let popup_id: egui::Id = egui::Id::new(("dl_error_overlay", thread.thread_id));
        let mut is_open = ui.memory(|m| m.data.get_temp::<bool>(popup_id)).unwrap_or(false);
        if err_resp.hovered() || over_err_now {
            is_open = true;
        }
        if is_open {
            let popup_pos = egui::pos2(err_rect.min.x, err_rect.min.y - 6.0);
            let inner = egui::Area::new(popup_id)
                .order(egui::Order::Foreground)
                .fixed_pos(popup_pos)
                .show(ui.ctx(), |ui| {
                    egui::Frame::default()
                        .fill(Color32::from_rgb(28, 28, 28))
                        .stroke(Stroke::new(1.0, Color32::from_gray(60)))
                        .rounding(Rounding::same(6.0))
                        .inner_margin(6.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(err).color(Color32::from_gray(220)))
                                    .wrap(true),
                            );
                        });
                });
            // keep open while hovering badge or overlay; close otherwise
            let pointer_pos = ui.input(|i| i.pointer.hover_pos());
            let over_badge = pointer_pos.map_or(false, |p| err_rect.contains(p));
            let overlay_rect = inner.response.rect;
            let over_overlay = pointer_pos.map_or(false, |p| overlay_rect.contains(p));
            if !(over_badge || over_overlay) {
                is_open = false;
            }
        }
        ui.memory_mut(|m| { m.data.insert_temp(popup_id, is_open); });
    }

    // Thin download progress line at the very bottom of the cover image
    match progress {
        Some(crate::game_download::Progress::Pending(mut dp)) => {
            dp = dp.clamp(0.0, 1.0);
            let thickness = 2.0;
            let x0 = cover_rect.min.x;
            let x1 = x0 + cover_rect.width() * dp;
            let y1 = cover_rect.max.y;
            let y0 = y1 - thickness;
            let line_rect = egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1));
            let color = if download_error.is_some() {
                Color32::from_rgb(180, 40, 40)
            } else {
                ui.visuals().selection.bg_fill
            };
            ui.painter_at(cover_rect).rect_filled(line_rect, Rounding::same(0.0), color);
        }
        Some(crate::game_download::Progress::Unknown) => {
            // Animated right-moving blue-to-transparent gradient with blue dominant
            draw_unknown_progress_bar(ui, cover_rect);
        }
        _ => {}
    }

    CardHover { hovered, hovered_line, download_clicked, selected_link }
}

fn draw_unknown_progress_bar(ui: &mut egui::Ui, cover_rect: egui::Rect) {
    // Same color as normal progress; smooth pulse (fade in/out).
    let thickness = 2.0;
    let y1 = cover_rect.max.y;
    let y0 = y1 - thickness;
    let line_rect = egui::Rect::from_min_max(
        egui::pos2(cover_rect.min.x, y0),
        egui::pos2(cover_rect.max.x, y1),
    );

    // Ensure continuous animation
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));

    let painter = ui.painter_at(cover_rect);

    // Base color: same as for normal progress
    let base = ui.visuals().selection.bg_fill;

    // Pulse alpha between 30% .. 100% of the base alpha with 1 Hz
    let t: f32 = ui.input(|i| i.time) as f32;
    let freq_hz = 1.0f32;
    let s = 0.5 + 0.5 * (t * std::f32::consts::TAU * freq_hz).sin(); // 0..1
    let alpha_scale = s; // 0.0..1.0

    // Compose color with same RGB and pulsating alpha
    let r = base.r();
    let g = base.g();
    let b = base.b();
    let a = ((base.a() as f32) * alpha_scale).round().clamp(0.0, 255.0) as u8;
    let color = Color32::from_rgba_premultiplied(r, g, b, a);

    painter.rect_filled(line_rect, Rounding::same(0.0), color);
}
