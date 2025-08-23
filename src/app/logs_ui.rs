// Logs viewport (separate OS window) with colored levels and utilities.

use eframe::egui;
use lazy_static::lazy_static;
use log::Level;
use std::sync::RwLock;

lazy_static! {
    static ref LOGS_OPEN: RwLock<bool> = RwLock::new(false);
    static ref AUTOSCROLL: RwLock<bool> = RwLock::new(true);
}

pub fn open_logs() {
    if let Ok(mut v) = LOGS_OPEN.write() {
        *v = true;
    }
}

pub fn is_open() -> bool {
    LOGS_OPEN.read().map(|g| *g).unwrap_or(false)
}

pub fn draw_logs_viewport(ctx: &egui::Context) {
    // Only draw if opened
    let is_open = LOGS_OPEN.read().map(|g| *g).unwrap_or(false);
    if !is_open {
        return;
    }

    let viewport_id = egui::ViewportId::from_hash_of("logs_window");

    //ctx.show_viewport_immediate(
    ctx.show_viewport_deferred(
        viewport_id,
        egui::ViewportBuilder::default()
            .with_title("Logs")
            .with_inner_size([800.0, 500.0])
            .with_resizable(true),
        move |ctx, _class| {
            // If user clicked the OS close (X), mark as closed and ensure viewport closes.
            if ctx.input(|i| i.viewport().close_requested()) {
                if let Ok(mut v) = LOGS_OPEN.write() {
                    *v = false;
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            egui::CentralPanel::default().show(ctx, |ui| {
                // Toolbar
                ui.horizontal(|ui| {
                    if ui.button("Clear").clicked() {
                        crate::logger::clear();
                    }
                    if ui.button("Copy").clicked() {
                        let text = crate::logger::get_all().join("\n");
                        ui.output_mut(|o| o.copied_text = text);
                    }
                    // Autoscroll toggle
                    let mut autoscroll = AUTOSCROLL.read().map(|g| *g).unwrap_or(true);
                    if ui.checkbox(&mut autoscroll, "Autoscroll").changed() {
                        if let Ok(mut w) = AUTOSCROLL.write() {
                            *w = autoscroll;
                        }
                    }
                    ui.separator();
                    ui.label(format!("{} lines", crate::logger::len()));
                });
                ui.separator();

                // Logs list (virtualized rendering for performance)
                let stick = AUTOSCROLL.read().map(|g| *g).unwrap_or(true);
                let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
                if stick {
                    scroll = scroll.stick_to_bottom(true);
                }

                let total = crate::logger::len();
                // Approximate row height for monospace text; add small padding
                let row_height = ui.text_style_height(&egui::TextStyle::Monospace) + 2.0;
                // Batch visible lines into a single layout job to reduce per-frame widget count.
                scroll.show_rows(ui, row_height, total, |ui, row_range| {
                    let mut job = egui::text::LayoutJob::default();
                    crate::logger::for_each_range(row_range.start, row_range.end, |e| {
                        let color = color_for_level(e.level);
                        let mut fmt = egui::TextFormat {
                            color,
                            ..Default::default()
                        };
                        fmt.font_id = egui::FontId::monospace(12.0);
                        let line = format!("[{:>5}] {}: {}\n", e.level, e.target, e.msg);
                        job.append(&line, 0.0, fmt);
                    });
                    ui.label(job);
                });
            });
        },
    );
}

fn color_for_level(level: Level) -> egui::Color32 {
    match level {
        Level::Error => egui::Color32::from_rgb(220, 80, 80),
        Level::Warn => egui::Color32::from_rgb(235, 200, 80),
        Level::Info => egui::Color32::from_rgb(200, 200, 200),
        Level::Debug => egui::Color32::from_rgb(120, 180, 255),
        Level::Trace => egui::Color32::from_rgb(160, 160, 160),
    }
}
