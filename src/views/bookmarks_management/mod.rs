use crate::app::settings::store::{
    create_bookmark, delete_bookmark, get_bookmarks, save_settings_to_disk, update_bookmark,
    Bookmark, APP_SETTINGS,
};
use eframe::egui::{self, Color32, Rounding, Vec2};
use lazy_static::lazy_static;
use std::sync::RwLock;

lazy_static! {
    pub static ref BOOKMARKS_MGMT_OPEN: RwLock<bool> = RwLock::new(false);
}

pub fn open_bookmarks_management() {
    *BOOKMARKS_MGMT_OPEN.write().unwrap() = true;
}

pub fn draw_bookmarks_management_viewport(ctx: &egui::Context) {
    if !*BOOKMARKS_MGMT_OPEN.read().unwrap() {
        return;
    }

    let viewport_id = egui::ViewportId::from_hash_of("bookmarks_mgmt_window");
    ctx.show_viewport_immediate(
        viewport_id,
        egui::ViewportBuilder::default()
            .with_title(crate::localization::translate("bookmarks-mgmt-title"))
            .with_inner_size([400.0, 500.0])
            .with_resizable(true),
        move |ctx, _class| {
            egui::CentralPanel::default().show(ctx, |ui| {
                draw_bookmarks_management_content(ui);
            });

            if ctx.input(|i| i.viewport().close_requested()) {
                *BOOKMARKS_MGMT_OPEN.write().unwrap() = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        },
    );
}

fn draw_bookmarks_management_content(ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        ui.heading(crate::localization::translate("bookmarks-mgmt-title"));
        ui.add_space(crate::ui_constants::spacing::MEDIUM);

        ui.group(|ui| {
            let mut visible_on_cover = {
                let st = APP_SETTINGS.read().unwrap();
                st.bookmarks_visible_on_cover
            };
            let mut default_color = {
                let st = APP_SETTINGS.read().unwrap();
                st.default_bookmark_color
            };

            ui.horizontal(|ui| {
                ui.label(crate::localization::translate(
                    "bookmarks-mgmt-visible-limit",
                ));
                if ui
                    .add(egui::Slider::new(&mut visible_on_cover, 1..=5))
                    .changed()
                {
                    APP_SETTINGS.write().unwrap().bookmarks_visible_on_cover = visible_on_cover;
                    save_settings_to_disk();
                }
            });

            ui.horizontal(|ui| {
                ui.label(crate::localization::translate(
                    "bookmarks-mgmt-default-color",
                ));
                let mut color_f32 = [
                    default_color[0] as f32 / 255.0,
                    default_color[1] as f32 / 255.0,
                    default_color[2] as f32 / 255.0,
                ];
                if ui.color_edit_button_rgb(&mut color_f32).changed() {
                    default_color = [
                        (color_f32[0] * 255.0) as u8,
                        (color_f32[1] * 255.0) as u8,
                        (color_f32[2] * 255.0) as u8,
                    ];
                    APP_SETTINGS.write().unwrap().default_bookmark_color = default_color;
                    save_settings_to_disk();
                }
            });
        });

        ui.add_space(crate::ui_constants::spacing::MEDIUM);

        let bookmarks = get_bookmarks();
        if bookmarks.is_empty() {
            ui.label(crate::localization::translate(
                "bookmarks-mgmt-no-bookmarks",
            ));
        } else {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for bookmark in bookmarks {
                    draw_bookmark_row(ui, bookmark);
                }
            });
        }

        ui.add_space(crate::ui_constants::spacing::MEDIUM);

        let creating_id = egui::Id::new("bookmarks_mgmt_creating");
        let is_creating = ui
            .memory(|m| m.data.get_temp::<bool>(creating_id))
            .unwrap_or(false);

        if is_creating {
            ui.group(|ui| {
                let mut emoji = ui
                    .memory(|m| m.data.get_temp::<String>(egui::Id::new("mgmt_new_emoji")))
                    .unwrap_or_default();
                let mut label = ui
                    .memory(|m| m.data.get_temp::<String>(egui::Id::new("mgmt_new_label")))
                    .unwrap_or_default();
                let mut color = ui
                    .memory(|m| {
                        m.data
                            .get_temp::<Option<[u8; 3]>>(egui::Id::new("mgmt_new_color"))
                    })
                    .unwrap_or(None);

                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut emoji)
                            .hint_text("🔖")
                            .desired_width(30.0),
                    );
                    ui.add(egui::TextEdit::singleline(&mut label).hint_text("Label"));

                    let mut color_f32 = color
                        .map(|c| {
                            [
                                c[0] as f32 / 255.0,
                                c[1] as f32 / 255.0,
                                c[2] as f32 / 255.0,
                            ]
                        })
                        .unwrap_or([1.0, 1.0, 1.0]);
                    if ui.color_edit_button_rgb(&mut color_f32).changed() {
                        color = Some([
                            (color_f32[0] * 255.0) as u8,
                            (color_f32[1] * 255.0) as u8,
                            (color_f32[2] * 255.0) as u8,
                        ]);
                    }

                    if ui
                        .button(crate::localization::translate("bookmarks-mgmt-save-btn"))
                        .clicked()
                        && !label.is_empty()
                    {
                        let final_emoji = if emoji.is_empty() {
                            "🔖".to_string()
                        } else {
                            emoji.clone()
                        };
                        create_bookmark(final_emoji, label.clone(), color);
                        ui.memory_mut(|m| {
                            m.data.insert_temp(creating_id, false);
                            m.data
                                .insert_temp(egui::Id::new("mgmt_new_emoji"), String::new());
                            m.data
                                .insert_temp(egui::Id::new("mgmt_new_label"), String::new());
                            m.data.insert_temp(
                                egui::Id::new("mgmt_new_color"),
                                None::<Option<[u8; 3]>>,
                            );
                        });
                    }
                    if ui
                        .button(crate::localization::translate("settings-cancel"))
                        .clicked()
                    {
                        ui.memory_mut(|m| m.data.insert_temp(creating_id, false));
                    }
                });

                ui.memory_mut(|m| {
                    m.data.insert_temp(egui::Id::new("mgmt_new_emoji"), emoji);
                    m.data.insert_temp(egui::Id::new("mgmt_new_label"), label);
                    m.data.insert_temp(egui::Id::new("mgmt_new_color"), color);
                });
            });
        } else {
            if ui
                .button(crate::localization::translate("bookmarks-mgmt-add-btn"))
                .clicked()
            {
                ui.memory_mut(|m| m.data.insert_temp(creating_id, true));
            }
        }
    });
}

fn draw_bookmark_row(ui: &mut egui::Ui, bookmark: Bookmark) {
    let edit_id = egui::Id::new(("mgmt_edit", bookmark.id.clone()));
    let is_editing = ui
        .memory(|m| m.data.get_temp::<bool>(edit_id))
        .unwrap_or(false);

    if is_editing {
        ui.horizontal(|ui| {
            let mut emoji = ui
                .memory(|m| {
                    m.data
                        .get_temp::<String>(egui::Id::new(("edit_emoji", bookmark.id.clone())))
                })
                .unwrap_or(bookmark.emoji.clone());
            let mut label = ui
                .memory(|m| {
                    m.data
                        .get_temp::<String>(egui::Id::new(("edit_label", bookmark.id.clone())))
                })
                .unwrap_or(bookmark.label.clone());
            let mut color = ui
                .memory(|m| {
                    m.data.get_temp::<Option<[u8; 3]>>(egui::Id::new((
                        "edit_color",
                        bookmark.id.clone(),
                    )))
                })
                .unwrap_or(bookmark.color);

            ui.add(egui::TextEdit::singleline(&mut emoji).desired_width(30.0));
            ui.add(egui::TextEdit::singleline(&mut label));

            let mut color_f32 = color
                .map(|c| {
                    [
                        c[0] as f32 / 255.0,
                        c[1] as f32 / 255.0,
                        c[2] as f32 / 255.0,
                    ]
                })
                .unwrap_or([1.0, 1.0, 1.0]);
            if ui.color_edit_button_rgb(&mut color_f32).changed() {
                color = Some([
                    (color_f32[0] * 255.0) as u8,
                    (color_f32[1] * 255.0) as u8,
                    (color_f32[2] * 255.0) as u8,
                ]);
            }

            if ui
                .button(crate::localization::translate("bookmarks-mgmt-save-btn"))
                .clicked()
            {
                update_bookmark(&bookmark.id, emoji.clone(), label.clone(), color);
                ui.memory_mut(|m| m.data.insert_temp(edit_id, false));
            }
            if ui
                .button(crate::localization::translate("settings-cancel"))
                .clicked()
            {
                ui.memory_mut(|m| m.data.insert_temp(edit_id, false));
            }

            ui.memory_mut(|m| {
                m.data
                    .insert_temp(egui::Id::new(("edit_emoji", bookmark.id.clone())), emoji);
                m.data
                    .insert_temp(egui::Id::new(("edit_label", bookmark.id.clone())), label);
                m.data
                    .insert_temp(egui::Id::new(("edit_color", bookmark.id.clone())), color);
            });
        });
    } else {
        ui.horizontal(|ui| {
            let bg_color = bookmark
                .color
                .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                .unwrap_or_else(|| {
                    let default = APP_SETTINGS.read().unwrap().default_bookmark_color;
                    Color32::from_rgb(default[0], default[1], default[2])
                });

            let (rect, _resp) = ui.allocate_exact_size(Vec2::splat(24.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, Rounding::same(4.0), bg_color);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &bookmark.emoji,
                egui::FontId::proportional(14.0),
                Color32::WHITE,
            );

            ui.label(&bookmark.label);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(crate::localization::translate("bookmarks-mgmt-delete-btn"))
                    .clicked()
                {
                    delete_bookmark(&bookmark.id);
                }
                if ui
                    .button(crate::localization::translate("bookmarks-mgmt-edit-btn"))
                    .clicked()
                {
                    ui.memory_mut(|m| {
                        m.data.insert_temp(edit_id, true);
                        m.data.insert_temp(
                            egui::Id::new(("edit_emoji", bookmark.id.clone())),
                            bookmark.emoji.clone(),
                        );
                        m.data.insert_temp(
                            egui::Id::new(("edit_label", bookmark.id.clone())),
                            bookmark.label.clone(),
                        );
                        m.data.insert_temp(
                            egui::Id::new(("edit_color", bookmark.id.clone())),
                            bookmark.color,
                        );
                    });
                }
            });
        });
    }
}
