use std::time::{Duration, Instant};

use eframe::egui;

use super::{NoLagApp, about_ui, errors_ui, logs_ui, settings, update_ui};
use crate::types::TagLogic;
use crate::views::filters::draw_filters_panel;

pub(super) fn update_main(app: &mut NoLagApp, ctx: &egui::Context) {
    // Обработка входящих сообщений (список тредов, обложки/скриншоты) вынесена в модуль fetch
    app.poll_incoming(ctx);

    // Poll active downloads and update progress
    app.poll_downloads(ctx);

    // Ensure covers for currently displayed items are scheduled (idempotent)
    app.schedule_cover_downloads(ctx);

    // Первый автозапуск загрузки
    // Не перезапускать автоматически при наличии ошибки (например, 429), чтобы не было бесконечного цикла запросов
    if app.net.last_result.is_none() && app.net.last_error.is_none() && !app.net.loading {
        if app.filters.library_only {
            // Если приложение стартует в режиме Library — сразу запускаем параллельную подзагрузку
            app.start_prefetch_library(ctx);
        } else {
            // Стартуем обычный список
            app.start_fetch(ctx);
            // И параллельно сразу же подгружаем библиотеку в фоне
            if !app.net.lib_started {
                app.start_prefetch_library(ctx);
            }
        }
    } else {
        // Гарантируем, что фоновая подзагрузка библиотеки запущена один раз
        if !app.net.lib_started {
            app.start_prefetch_library(ctx);
        }
    }

    // Правая панель — фильтры
    let prev_query = app.filters.query.clone();
    let (apply, open_settings_btn, open_logs_btn, open_about_btn) = draw_filters_panel(
        ctx,
        &mut app.filters.sort,
        &mut app.filters.date_limit,
        &mut app.filters.include_logic,
        &mut app.filters.include_tags,
        &mut app.filters.exclude_mode,
        &mut app.filters.exclude_tags,
        &mut app.filters.include_prefixes,
        &mut app.filters.exclude_prefixes,
        &mut app.filters.search_mode,
        &mut app.filters.query,
        &mut app.filters.library_only,
    );
    if apply {
        // Немедленно перезапустить поиск при изменении фильтров (кроме текста)
        app.page = 1;
        app.filters.search_due_at = None;
        if app.filters.library_only {
            app.start_fetch_library(ctx);
        } else {
            app.start_fetch(ctx);
        }
    }
    // Debounce text query changes: run search after last edit
    let query_changed = app.filters.query != prev_query;
    if query_changed {
        if apply {
            // Filters changed this frame and already triggered immediate fetch; skip debounce
            app.filters.search_due_at = None;
        } else {
            app.page = 1;
            let debounce = Duration::from_millis(crate::ui_constants::SEARCH_DEBOUNCE_MS);
            app.filters.search_due_at = Some(Instant::now() + debounce);
            // Wake up after the debounce interval to fire the search
            ctx.request_repaint_after(debounce);
        }
    }
    if open_settings_btn {
        settings::open_settings();
        ctx.request_repaint();
    }
    if open_logs_btn {
        logs_ui::open_logs();
        ctx.request_repaint();
    }
    if open_about_btn {
        about_ui::open_about();
        ctx.request_repaint();
    }
    // When filters changed this frame, auto-save selected tags if enabled in settings
    if apply {
        let do_autosave = settings::with_settings(|s| s.autosave_selected_tags);
        if do_autosave {
            let need_save = settings::with_settings_mut(|st| {
                let mut changed = false;
                if st.startup_tags != app.filters.include_tags {
                    st.startup_tags = app.filters.include_tags.clone();
                    changed = true;
                }
                if st.startup_exclude_tags != app.filters.exclude_tags {
                    st.startup_exclude_tags = app.filters.exclude_tags.clone();
                    changed = true;
                }
                changed
            });
            if need_save {
                settings::save_settings_to_disk();
            }
        }
    }
    // Trigger new fetch when Library mode toggles
    if app.filters.last_library_only != app.filters.library_only {
        app.filters.last_library_only = app.filters.library_only;
        if app.filters.library_only {
            // Если фоновые данные уже есть — мгновенно показываем их
            if let Some(msg) = &app.net.lib_result {
                app.net.last_result = Some(msg.clone());
                app.net.last_error = None;
                app.net.loading = false;
                // Immediately schedule cover downloads for the freshly shown Library data
                app.schedule_cover_downloads(ctx);
                ctx.request_repaint();
            } else {
                // Обеспечим запуск фоновой загрузки и покажем спиннер
                if !app.net.lib_started {
                    app.start_prefetch_library(ctx);
                }
                app.net.last_result = None;
                app.net.last_error = None;
                app.net.loading = true;
            }
        } else {
            app.start_fetch(ctx);
        }
    }

    // Run debounced query fetch if deadline passed
    if let Some(due) = app.filters.search_due_at {
        if Instant::now() >= due {
            app.filters.search_due_at = None;
            if app.filters.library_only {
                app.start_fetch_library(ctx);
            } else {
                app.start_fetch(ctx);
            }
        }
    }

    // Центральная панель — сетка карточек
    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let avail_w = ui.available_width().floor();
                let card_w = crate::ui_constants::CARD_WIDTH;
                let gap = crate::ui_constants::CARD_GAP;

                let mut cols = ((avail_w + gap) / (card_w + gap)).floor() as usize;
                if cols == 0 {
                    cols = 1;
                }
                let row_w = (cols as f32) * card_w + ((cols - 1) as f32) * gap;
                let left_pad = ((avail_w - row_w) / 2.0).max(0.0);

                if let Some(err) = &app.net.last_error {
                    ui.vertical_centered(|ui| {
                        ui.colored_label(
                            egui::Color32::RED,
                            crate::localization::translate_with(
                                "error-prefix",
                                &[("err", err.clone())],
                            ),
                        );
                    });
                } else if app.net.loading && app.net.last_result.is_none() {
                    ui.add_space(crate::ui_constants::spacing::XLARGE);
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Spinner::new());
                        ui.label(crate::localization::translate("loading"));
                    });
                } else if app.net.last_result.is_some() {
                    // Clone data so we don't hold an immutable borrow of `app` across a call
                    // that needs `&mut self` (draw_threads_grid).
                    let data_cloned = {
                        let msg = app.net.last_result.as_ref().unwrap();
                        msg.data.clone()
                    };
                    // Build a set of hidden thread_ids and filter them out from rendering
                    let hidden: std::collections::HashSet<u64> =
                        settings::with_settings(|st| st.hidden_threads.iter().copied().collect());

                    // When Library mode is ON, show downloaded AND in-progress games; always ignore hidden ones
                    let mut display_data: Vec<crate::parser::F95Thread> =
                        if app.filters.library_only {
                            // Persisted completed downloads
                            let downloaded_ids: std::collections::HashSet<u64> =
                                settings::with_settings(|st| {
                                    st.downloaded_games
                                        .iter()
                                        .filter(|g| settings::game_folder_exists(&g.folder))
                                        .map(|g| g.thread_id)
                                        .collect()
                                });
                            // In-progress downloads (runtime-only)
                            let downloading_ids: std::collections::HashSet<u64> =
                                app.downloads.keys().copied().collect();
                            // Persisted pending/incomplete downloads (from previous sessions or failed attempts)
                            let pending_ids: std::collections::HashSet<u64> =
                                settings::with_settings(|st| {
                                    st.pending_downloads.iter().copied().collect()
                                });
                            let in_library = |id: u64| {
                                downloaded_ids.contains(&id)
                                    || downloading_ids.contains(&id)
                                    || pending_ids.contains(&id)
                            };

                            data_cloned
                                .into_iter()
                                .filter(|t| in_library(t.thread_id.get()))
                                .filter(|t| !hidden.contains(&t.thread_id.get()))
                                .collect()
                        } else {
                            data_cloned
                                .into_iter()
                                .filter(|t| !hidden.contains(&t.thread_id.get()))
                                .collect()
                        };

                    // Apply client-side filters and sorting in Library mode
                    if app.filters.library_only {
                        // Text query (Title or Creator)
                        let q = app.filters.query.to_lowercase();
                        let use_query = !q.trim().is_empty();

                        display_data.retain(|t| {
                            // Query
                            if use_query {
                                let hay = t.title.to_lowercase();
                                if !hay.contains(&q) {
                                    return false;
                                }
                            }

                            // Include tags with OR/AND logic
                            if !app.filters.include_tags.is_empty() {
                                let has = |id: &u32| t.tags.contains(id);
                                let ok = match app.filters.include_logic {
                                    TagLogic::And => app.filters.include_tags.iter().all(has),
                                    TagLogic::Or => app.filters.include_tags.iter().any(has),
                                };
                                if !ok {
                                    return false;
                                }
                            }

                            // Exclude tags
                            if !app.filters.exclude_tags.is_empty()
                                && app
                                    .filters
                                    .exclude_tags
                                    .iter()
                                    .any(|id| t.tags.contains(id))
                            {
                                return false;
                            }
                            true
                        });
                    }

                    app.draw_threads_grid(ui, ctx, &display_data, cols, left_pad, gap, card_w);

                    // Bottom controls: pagination in normal mode, summary in Library mode
                    ui.add_space(crate::ui_constants::spacing::MEDIUM);
                    ui.vertical_centered(|ui| {
                        if app.filters.library_only {
                            let installed_count = settings::with_settings(|st| {
                                st.downloaded_games
                                    .iter()
                                    .filter(|g| settings::game_folder_exists(&g.folder))
                                    .count()
                            });
                            ui.label(crate::localization::translate_with(
                                "library-summary",
                                &[
                                    ("shown", display_data.len().to_string()),
                                    ("installed", installed_count.to_string()),
                                ],
                            ));
                        } else {
                            let (cur, total) = {
                                let msg = app.net.last_result.as_ref().unwrap();
                                (msg.pagination.page, msg.pagination.total)
                            };
                            ui.horizontal(|ui| {
                                let prev_enabled = cur > 1;
                                if ui
                                    .add_enabled(prev_enabled, egui::Button::new("◀"))
                                    .clicked()
                                {
                                    app.page = cur.saturating_sub(1);
                                    app.start_fetch(ctx);
                                }
                                ui.label(crate::localization::translate_with(
                                    "pagination-page",
                                    &[("cur", cur.to_string()), ("total", total.to_string())],
                                ));
                                let next_enabled = cur < total;
                                if ui
                                    .add_enabled(next_enabled, egui::Button::new("▶"))
                                    .clicked()
                                {
                                    app.page = cur + 1;
                                    app.start_fetch(ctx);
                                }
                            });
                        }
                    });
                }
            });
    });

    // Floating Update + Errors overlay
    let bottom_offset = update_ui::draw_update_notice(ctx);
    errors_ui::draw_errors_button(ctx, bottom_offset);
    errors_ui::draw_errors_viewport(ctx);

    // Logs window (separate OS viewport)
    logs_ui::draw_logs_viewport(ctx);

    // About window (separate OS viewport)
    about_ui::draw_about_viewport(ctx);

    // Settings window (separate OS viewport)
    settings::draw_settings_viewport(ctx);
}
