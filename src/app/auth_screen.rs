use eframe::egui::{self, RichText};

use super::state::Screen;
use super::{NoLagApp, about_ui, errors_ui, logs_ui, settings, update_ui};
use crate::app::config as app_config;
use crate::app::rt; // состояние экрана из модуля app::state

pub(super) fn update_auth(app: &mut NoLagApp, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        match app.auth.screen {
            Screen::AuthLogin => {
                ui.add_space(crate::ui_constants::spacing::XLARGE);
                ui.vertical_centered(|ui| {
                    ui.heading(crate::localization::translate("auth-login-title"));
                });
                ui.add_space(crate::ui_constants::spacing::MEDIUM);
                ui.horizontal(|ui| {
                    ui.label(crate::localization::translate("auth-username"));
                    ui.text_edit_singleline(&mut app.auth.login_username);
                });
                ui.horizontal(|ui| {
                    ui.label(crate::localization::translate("auth-password"));
                    let te =
                        egui::TextEdit::singleline(&mut app.auth.login_password).password(true);
                    ui.add(te);
                });
                if let Some(err) = &app.auth.login_error {
                    ui.colored_label(egui::Color32::RED, err);
                }
                ui.add_space(crate::ui_constants::spacing::MEDIUM);
                let login_clicked = ui
                    .add_enabled(
                        !app.auth.login_in_progress,
                        egui::Button::new(crate::localization::translate("auth-login-button")),
                    )
                    .clicked();
                if app.auth.login_in_progress {
                    ui.add_space(crate::ui_constants::spacing::SMALL);
                    ui.add(egui::Spinner::new());
                    ui.label(crate::localization::translate("auth-authorizing"));
                }
                if login_clicked {
                    app.auth.login_error = None;
                    app.auth.login_in_progress = true;
                    let has_manual = !app.auth.login_username.trim().is_empty()
                        && !app.auth.login_password.is_empty();
                    let tx = app.auth.auth_tx.clone();
                    let ctx2 = ctx.clone();
                    if has_manual {
                        let u = app.auth.login_username.clone();
                        let p = app.auth.login_password.clone();
                        rt().spawn(async move {
                            let res = app_config::login_and_store(u, p).await;
                            let _ = tx.send(res);
                            ctx2.request_repaint();
                        });
                    } else {
                        // Если поля пустые — пробуем взять из .env (F95_LOGIN/F95_PASSWORD)
                        rt().spawn(async move {
                            let res = app_config::login_from_env_and_store().await;
                            let _ = tx.send(res);
                            ctx2.request_repaint();
                        });
                    }
                }

                // Keep 12.0 as-is (no exact constant); could be tuned later
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(crate::ui_constants::spacing::MEDIUM);
                ui.label(crate::localization::translate("auth-or-paste-cookies"));
                let te2 =
                    egui::TextEdit::multiline(&mut app.auth.login_cookies_input).desired_rows(3);
                ui.add(te2);
                ui.add_space(crate::ui_constants::spacing::SMALL);
                let use_clicked = ui
                    .add_enabled(
                        !app.auth.login_in_progress,
                        egui::Button::new(crate::localization::translate("auth-use-cookies")),
                    )
                    .clicked();
                if use_clicked {
                    let c = app.auth.login_cookies_input.trim();
                    if c.is_empty() {
                        app.auth.login_error =
                            Some(crate::localization::translate("auth-please-paste-cookies"));
                    } else {
                        {
                            let mut cfg = app_config::APP_CONFIG.write().unwrap();
                            cfg.cookies = Some(c.to_string());
                            if !app.auth.login_username.trim().is_empty() {
                                cfg.username = Some(app.auth.login_username.clone());
                            }
                        }
                        app_config::save_config_to_disk();
                        app.auth.login_error = None;
                        app.auth.screen = Screen::Main;
                        app.page = 1;
                        app.filters.search_due_at = None;
                        app.net.loading = false;
                        app.start_fetch(ctx);
                    }
                }
                ui.add_space(crate::ui_constants::spacing::MEDIUM);
                ui.label(RichText::new(crate::localization::translate("auth-info-needed")).small());
            }
            Screen::Main => {}
        }
    });

    // Отдельные окна/оверлеи доступны всегда
    logs_ui::draw_logs_viewport(ctx);
    let bottom_offset = update_ui::draw_update_notice(ctx);
    errors_ui::draw_errors_button(ctx, bottom_offset);
    errors_ui::draw_errors_viewport(ctx);
    about_ui::draw_about_viewport(ctx);
    settings::draw_settings_viewport(ctx);
}
