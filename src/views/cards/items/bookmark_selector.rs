use crate::app::settings::store::{
    add_bookmark_to_game, create_bookmark, get_bookmarks, get_game_bookmarks,
    remove_bookmark_from_game,
};
use eframe::egui::{self, Color32, RichText, Rounding, Stroke, Vec2};

pub fn draw_bookmark_selector_popup(ui: &mut egui::Ui, thread_id: u64, card_rect: egui::Rect) {
    let popup_id = egui::Id::new(("bookmark_selector_open", thread_id));
    let is_open = ui
        .ctx()
        .memory(|m| m.data.get_temp::<bool>(popup_id))
        .unwrap_or(false);

    if !is_open {
        return;
    }

    let popup_width = 200.0;
    let popup_pos = egui::pos2(
        card_rect.left(),
        card_rect.bottom() + crate::ui_constants::spacing::SMALL,
    );

    let inner = crate::views::ui_helpers::show_popup_area(
        ui,
        egui::Id::new(("bookmark_selector_area", thread_id)),
        popup_pos,
        popup_width,
        Color32::from_gray(80),
        Rounding::same(crate::ui_constants::card::ROUNDING),
        |ui| {
            ui.set_max_width(popup_width - 16.0);
            ui.vertical(|ui| {
                ui.add_space(crate::ui_constants::spacing::SMALL);
                ui.horizontal(|ui| {
                    ui.add_space(crate::ui_constants::spacing::MEDIUM);
                    ui.label(
                        RichText::new(crate::localization::translate("bookmarks-selector-title"))
                            .strong(),
                    );
                });
                ui.add_space(crate::ui_constants::spacing::SMALL);

                let game_bookmarks = get_game_bookmarks(thread_id);
                if !game_bookmarks.is_empty() {
                    for bookmark in &game_bookmarks {
                        ui.horizontal(|ui| {
                            ui.add_space(crate::ui_constants::spacing::MEDIUM);
                            let bg_color = bookmark
                                .color
                                .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                                .unwrap_or(Color32::from_gray(60));

                            let (rect, _resp) =
                                ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                            ui.painter()
                                .rect_filled(rect, Rounding::same(4.0), bg_color);
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                &bookmark.emoji,
                                egui::FontId::proportional(12.0),
                                Color32::WHITE,
                            );

                            ui.label(&bookmark.label);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.add_space(crate::ui_constants::spacing::MEDIUM);
                                    if ui.button("✕").clicked() {
                                        remove_bookmark_from_game(thread_id, &bookmark.id);
                                    }
                                },
                            );
                        });
                    }
                    ui.add_space(crate::ui_constants::spacing::SMALL);
                    ui.separator();
                    ui.add_space(crate::ui_constants::spacing::SMALL);
                }

                // Picker for existing bookmarks
                let all_bookmarks = get_bookmarks();
                let available_bookmarks: Vec<_> = all_bookmarks
                    .into_iter()
                    .filter(|b| !game_bookmarks.iter().any(|gb| gb.id == b.id))
                    .collect();

                if !available_bookmarks.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(crate::ui_constants::spacing::MEDIUM);
                        ui.label(crate::localization::translate(
                            "bookmarks-selector-add-placeholder",
                        ));
                    });
                    ui.add_space(crate::ui_constants::spacing::SMALL);

                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(crate::ui_constants::spacing::MEDIUM);
                        for bookmark in &available_bookmarks {
                            let bg_color = bookmark
                                .color
                                .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                                .unwrap_or(Color32::from_gray(60));

                            let (rect, resp) =
                                ui.allocate_exact_size(Vec2::splat(24.0), egui::Sense::click());

                            ui.painter()
                                .rect_filled(rect, Rounding::same(4.0), bg_color);
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                &bookmark.emoji,
                                egui::FontId::proportional(14.0),
                                Color32::WHITE,
                            );

                            if resp.clicked() {
                                add_bookmark_to_game(thread_id, &bookmark.id);
                            }

                            resp.on_hover_text(&bookmark.label);
                        }
                    });
                    ui.add_space(crate::ui_constants::spacing::SMALL);
                }

                // Create new bookmark form
                let creating_id = egui::Id::new(("bookmark_creating", thread_id));
                let is_creating = ui
                    .memory(|m| m.data.get_temp::<bool>(creating_id))
                    .unwrap_or(false);

                if is_creating {
                    ui.group(|ui| {
                        let mut emoji = ui
                            .memory(|m| {
                                m.data.get_temp::<String>(egui::Id::new((
                                    "new_bookmark_emoji",
                                    thread_id,
                                )))
                            })
                            .unwrap_or_default();
                        let mut label = ui
                            .memory(|m| {
                                m.data.get_temp::<String>(egui::Id::new((
                                    "new_bookmark_label",
                                    thread_id,
                                )))
                            })
                            .unwrap_or_default();

                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut emoji)
                                    .hint_text(crate::localization::translate(
                                        "bookmarks-selector-emoji-placeholder",
                                    ))
                                    .desired_width(30.0),
                            );
                            ui.add(
                                egui::TextEdit::singleline(&mut label)
                                    .hint_text(crate::localization::translate(
                                        "bookmarks-selector-label-placeholder",
                                    ))
                                    .desired_width(100.0),
                            );
                        });

                        // Simple color palette
                        let colors = [
                            [60, 120, 200],
                            [200, 60, 60],
                            [60, 160, 60],
                            [200, 160, 60],
                            [160, 60, 160],
                            [60, 160, 160],
                        ];
                        let selected_color_id = egui::Id::new(("new_bookmark_color", thread_id));
                        let mut selected_color = ui
                            .memory(|m| m.data.get_temp::<Option<[u8; 3]>>(selected_color_id))
                            .unwrap_or(None);

                        ui.horizontal(|ui| {
                            for color in colors {
                                let color32 = Color32::from_rgb(color[0], color[1], color[2]);
                                let (rect, resp) =
                                    ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::click());
                                let stroke = if selected_color == Some(color) {
                                    Stroke::new(2.0, Color32::WHITE)
                                } else {
                                    Stroke::new(1.0, Color32::from_gray(60))
                                };
                                ui.painter()
                                    .rect(rect, Rounding::same(2.0), color32, stroke);
                                if resp.clicked() {
                                    selected_color = Some(color);
                                }
                            }
                        });

                        ui.horizontal(|ui| {
                            if ui
                                .button(crate::localization::translate(
                                    "bookmarks-selector-create-btn",
                                ))
                                .clicked()
                                && !label.is_empty()
                            {
                                let final_emoji = if emoji.is_empty() {
                                    "🔖".to_string()
                                } else {
                                    emoji.clone()
                                };
                                let new_id =
                                    create_bookmark(final_emoji, label.clone(), selected_color);
                                add_bookmark_to_game(thread_id, &new_id);

                                ui.memory_mut(|m| {
                                    m.data.insert_temp(creating_id, false);
                                    m.data.insert_temp(
                                        egui::Id::new(("new_bookmark_emoji", thread_id)),
                                        String::new(),
                                    );
                                    m.data.insert_temp(
                                        egui::Id::new(("new_bookmark_label", thread_id)),
                                        String::new(),
                                    );
                                    m.data
                                        .insert_temp(selected_color_id, None::<Option<[u8; 3]>>);
                                });
                            }
                            if ui
                                .button(crate::localization::translate(
                                    "bookmarks-selector-cancel-btn",
                                ))
                                .clicked()
                            {
                                ui.memory_mut(|m| m.data.insert_temp(creating_id, false));
                            }
                        });

                        ui.memory_mut(|m| {
                            m.data.insert_temp(
                                egui::Id::new(("new_bookmark_emoji", thread_id)),
                                emoji,
                            );
                            m.data.insert_temp(
                                egui::Id::new(("new_bookmark_label", thread_id)),
                                label,
                            );
                            m.data.insert_temp(selected_color_id, selected_color);
                        });
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.add_space(crate::ui_constants::spacing::MEDIUM);
                        if ui
                            .link(crate::localization::translate(
                                "bookmarks-selector-create-new",
                            ))
                            .clicked()
                        {
                            ui.memory_mut(|m| m.data.insert_temp(creating_id, true));
                        }
                    });
                }
                ui.add_space(crate::ui_constants::spacing::SMALL);
            });
        },
    );

    let clicked_outside =
        crate::views::ui_helpers::clicked_outside(ui, &[inner.response.rect, card_rect]);
    if clicked_outside {
        ui.memory_mut(|m| m.data.insert_temp(popup_id, false));
    }
}
