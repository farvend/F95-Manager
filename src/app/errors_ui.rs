// Errors viewport (separate OS window) and floating "Errors" button.
// Collects download/unzip errors and lets the user inspect/clear/copy them.

use eframe::egui;
use lazy_static::lazy_static;
use std::collections::VecDeque;
use std::sync::{Mutex, RwLock};

const MAX_ERRORS: usize = 1000;

lazy_static! {
    static ref ERRORS_OPEN: RwLock<bool> = RwLock::new(false);
    static ref ERRORS: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
}

pub(super) fn open_errors() {
    if let Ok(mut w) = ERRORS_OPEN.write() {
        *w = true;
    }
}

pub(super) fn is_open() -> bool {
    ERRORS_OPEN.read().map(|g| *g).unwrap_or(false)
}

pub(super) fn append_error(msg: impl Into<String>) {
    let s = msg.into();
    if let Ok(mut q) = ERRORS.lock() {
        q.push_back(s);
        if q.len() > MAX_ERRORS {
            q.pop_front();
        }
    }
}

pub(super) fn len() -> usize {
    if let Ok(q) = ERRORS.lock() {
        q.len()
    } else {
        0
    }
}

pub(super) fn clear() {
    if let Ok(mut q) = ERRORS.lock() {
        q.clear();
    }
}

fn range_lines(start: usize, end: usize) -> Vec<String> {
    if let Ok(q) = ERRORS.lock() {
        let len = q.len();
        let s = start.min(len);
        let e = end.min(len);
        q.iter()
            .skip(s)
            .take(e.saturating_sub(s))
            .cloned()
            .collect()
    } else {
        vec![]
    }
}

// Floating button in the bottom-right corner of the main window.
// Appears only when there are errors collected.
pub(super) fn draw_errors_button(ctx: &egui::Context) {
    let n = len();
    if n == 0 {
        return;
    }
    egui::Area::new("errors_button_floating".into())
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::Vec2::new(-12.0, -12.0))
        .interactable(true)
        .show(ctx, |ui| {
            let btn = egui::Button::new(format!("Errors ({n})"))
                .fill(egui::Color32::from_rgb(160, 60, 60));
            if ui.add(btn).clicked() {
                open_errors();
                ctx.request_repaint();
            }
        });
}

// Separate OS viewport with the list of errors.
pub(super) fn draw_errors_viewport(ctx: &egui::Context) {
    // Only draw if opened
    let is_open = ERRORS_OPEN.read().map(|g| *g).unwrap_or(false);
    if !is_open {
        return;
    }

    let viewport_id = egui::ViewportId::from_hash_of("errors_window");

    ctx.show_viewport_immediate(
        viewport_id,
        egui::ViewportBuilder::default()
            .with_title("Errors")
            .with_inner_size([800.0, 450.0])
            .with_resizable(true),
        move |ctx, _class| {
            // If user clicked the OS close (X), mark as closed and ensure viewport closes.
            if ctx.input(|i| i.viewport().close_requested()) {
                if let Ok(mut v) = ERRORS_OPEN.write() {
                    *v = false;
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            egui::CentralPanel::default().show(ctx, |ui| {
                // Toolbar
                ui.horizontal(|ui| {
                    if ui.button("Clear").clicked() {
                        clear();
                    }
                    if ui.button("Copy").clicked() {
                        let text = {
                            if let Ok(buf) = ERRORS.lock() {
                                buf.iter().cloned().collect::<Vec<_>>().join("\n")
                            } else {
                                String::new()
                            }
                        };
                        ui.output_mut(|o| o.copied_text = text);
                    }
                    ui.separator();
                    ui.label(format!("{} errors", len()));
                });
                ui.separator();

                // Errors list (virtualized rendering)
                let total = len();
                let row_height = ui.text_style_height(&egui::TextStyle::Monospace) + 2.0;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show_rows(ui, row_height, total, |ui, row_range| {
                        let mut job = egui::text::LayoutJob::default();
                        let lines = range_lines(row_range.start, row_range.end);
                        for line in lines {
                            let mut fmt = egui::TextFormat {
                                color: egui::Color32::from_rgb(230, 140, 140),
                                ..Default::default()
                            };
                            fmt.font_id = egui::FontId::monospace(12.0);
                            job.append(&format!("{line}\n"), 0.0, fmt);
                        }
                        ui.label(job);
                    });
            });
        },
    );
}
