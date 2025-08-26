// About viewport (separate OS window) with GitHub link.

use eframe::egui;
use lazy_static::lazy_static;
use std::sync::RwLock;

lazy_static! {
    static ref ABOUT_OPEN: RwLock<bool> = RwLock::new(false);
}

pub fn open_about() {
    if let Ok(mut v) = ABOUT_OPEN.write() {
        *v = true;
    }
}

pub fn is_open() -> bool {
    ABOUT_OPEN.read().map(|g| *g).unwrap_or(false)
}

pub fn draw_about_viewport(ctx: &egui::Context) {
    // Only draw if opened; also keep one extra frame if closing to let OS process Close.
    let is_open = ABOUT_OPEN.read().map(|g| *g).unwrap_or(false);
    if !is_open {
        if let Ok(mut v) = ABOUT_OPEN.write() {
            *v = false;
        }
        return;
    }

    let viewport_id = egui::ViewportId::from_hash_of("about_window");

    ctx.show_viewport_immediate(
        viewport_id,
        egui::ViewportBuilder::default()
            .with_title("About F95 Manager")
            .with_inner_size([420.0, 220.0])
            .with_resizable(false),
        move |ctx, _class| {
            // If user clicked the OS close (X), mark as closed and ensure viewport closes.
            if ctx.input(|i| i.viewport().close_requested()) {
                if let Ok(mut v) = ABOUT_OPEN.write() {
                    *v = false;
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                ctx.request_repaint();
                return;
            }

            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("F95 Manager");
                ui.add_space(4.0);
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.add_space(8.0);
                ui.hyperlink_to(
                    "Source code and updates",
                    "https://github.com/farvend/F95-Manager",
                );
                ui.add_space(8.0);
                ui.hyperlink_to(
                    "F95 thread",
                    "https://f95zone.to/threads/267483/",
                );
            });
        },
    );
}
