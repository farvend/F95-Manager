use std::collections::HashSet;
use std::time::{Duration, Instant};

use eframe::egui;

use super::{about_ui, errors_ui, logs_ui, settings, update_ui, NoLagApp};
use crate::parser::F95Thread;
use crate::types::TagLogic;
use crate::views::filters::draw_filters_panel;

/// Grid layout parameters for the card display
struct GridLayout {
    cols: usize,
    left_pad: f32,
    gap: f32,
    card_w: f32,
}

/// Calculate grid layout based on available width
fn calculate_grid_layout(avail_w: f32) -> GridLayout {
    let card_w = crate::ui_constants::CARD_WIDTH;
    let gap = crate::ui_constants::CARD_GAP;

    let mut cols = ((avail_w + gap) / (card_w + gap)).floor() as usize;
    if cols == 0 {
        cols = 1;
    }
    let row_w = (cols as f32) * card_w + ((cols - 1) as f32) * gap;
    let left_pad = ((avail_w - row_w) / 2.0).max(0.0);

    GridLayout {
        cols,
        left_pad,
        gap,
        card_w,
    }
}

/// Render error state with red error message
fn render_error_state(ui: &mut egui::Ui, err: &str) {
    ui.vertical_centered(|ui| {
        ui.colored_label(
            egui::Color32::RED,
            crate::localization::translate_with("error-prefix", &[("err", err.to_string())]),
        );
    });
}

/// Render loading state with spinner
fn render_loading_state(ui: &mut egui::Ui) {
    ui.add_space(crate::ui_constants::spacing::XLARGE);
    ui.vertical_centered(|ui| {
        ui.add(egui::Spinner::new());
        ui.label(crate::localization::translate("loading"));
    });
}

/// Build display data by filtering threads based on library/hidden status
fn build_display_data(
    app: &NoLagApp,
    data: Vec<F95Thread>,
    hidden: &HashSet<u64>,
) -> Vec<F95Thread> {
    if app.filters.library_only {
        // Persisted completed downloads
        let downloaded_ids: HashSet<u64> = settings::with_settings(|st| {
            st.downloaded_games
                .iter()
                .filter(|g| settings::game_folder_exists(&g.folder))
                .map(|g| g.thread_id)
                .collect()
        });
        // In-progress downloads (runtime-only)
        let downloading_ids: HashSet<u64> = app.downloads.keys().copied().collect();
        // Persisted pending/incomplete downloads
        let pending_ids: HashSet<u64> =
            settings::with_settings(|st| st.pending_downloads.iter().copied().collect());

        let in_library = |id: u64| {
            downloaded_ids.contains(&id)
                || downloading_ids.contains(&id)
                || pending_ids.contains(&id)
        };

        data.into_iter()
            .filter(|t| in_library(t.thread_id.get()))
            .filter(|t| !hidden.contains(&t.thread_id.get()))
            .collect()
    } else {
        data.into_iter()
            .filter(|t| !hidden.contains(&t.thread_id.get()))
            .collect()
    }
}

/// Apply client-side filters (query, tags) in Library mode
fn apply_library_filters(app: &NoLagApp, display_data: &mut Vec<F95Thread>) {
    if !app.filters.library_only {
        return;
    }

    let q = app.filters.query.to_lowercase();
    let use_query = !q.trim().is_empty();

    display_data.retain(|t| {
        // Query filter
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

/// Render bottom controls: library summary or pagination
fn render_bottom_controls(
    ui: &mut egui::Ui,
    app: &mut NoLagApp,
    ctx: &egui::Context,
    display_count: usize,
) {
    ui.add_space(crate::ui_constants::spacing::MEDIUM);
    ui.vertical_centered(|ui| {
        if app.filters.library_only {
            render_library_summary(ui, display_count);
        } else {
            render_pagination(ui, app, ctx);
        }
    });
}

/// Render library mode summary (shown/installed counts)
fn render_library_summary(ui: &mut egui::Ui, display_count: usize) {
    let installed_count = settings::with_settings(|st| {
        st.downloaded_games
            .iter()
            .filter(|g| settings::game_folder_exists(&g.folder))
            .count()
    });
    ui.label(crate::localization::translate_with(
        "library-summary",
        &[
            ("shown", display_count.to_string()),
            ("installed", installed_count.to_string()),
        ],
    ));
}

/// Render pagination controls (prev/next buttons with page info)
fn render_pagination(ui: &mut egui::Ui, app: &mut NoLagApp, ctx: &egui::Context) {
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

/// Render the main content area (threads grid or status messages)
fn render_central_content(ui: &mut egui::Ui, app: &mut NoLagApp, ctx: &egui::Context) {
    let layout = calculate_grid_layout(ui.available_width().floor());

    if let Some(err) = &app.net.last_error {
        render_error_state(ui, err);
    } else if app.net.loading && app.net.last_result.is_none() {
        render_loading_state(ui);
    } else if app.net.last_result.is_some() {
        // Clone data to avoid borrow conflicts with draw_threads_grid
        let data_cloned = app.net.last_result.as_ref().unwrap().data.clone();

        // Build hidden threads set
        let hidden: HashSet<u64> =
            settings::with_settings(|st| st.hidden_threads.iter().copied().collect());

        // Filter threads for display
        let mut display_data = build_display_data(app, data_cloned, &hidden);

        // Apply client-side filters in Library mode
        apply_library_filters(app, &mut display_data);

        // Draw the threads grid
        app.draw_threads_grid(
            ui,
            ctx,
            &display_data,
            layout.cols,
            layout.left_pad,
            layout.gap,
            layout.card_w,
        );

        // Bottom controls
        render_bottom_controls(ui, app, ctx, display_data.len());
    }
}

/// Poll incoming messages, downloads, and schedule cover downloads
fn poll_app_state(app: &mut NoLagApp, ctx: &egui::Context) {
    app.poll_incoming(ctx);
    app.poll_downloads(ctx);
    app.schedule_cover_downloads(ctx);
}

/// Handle initial fetch on app startup
/// Не перезапускать автоматически при наличии ошибки (например, 429)
fn handle_initial_fetch(app: &mut NoLagApp, ctx: &egui::Context) {
    if app.net.last_result.is_none() && app.net.last_error.is_none() && !app.net.loading {
        if app.filters.library_only {
            app.start_prefetch_library(ctx);
        } else {
            app.start_fetch(ctx);
            if !app.net.lib_started {
                app.start_prefetch_library(ctx);
            }
        }
    } else if !app.net.lib_started {
        app.start_prefetch_library(ctx);
    }
}

/// Result of drawing the filters panel
struct FiltersPanelResult {
    apply: bool,
    open_settings: bool,
    open_logs: bool,
    open_about: bool,
    prev_query: String,
}

/// Draw filters panel and return interaction results
fn draw_filters(app: &mut NoLagApp, ctx: &egui::Context) -> FiltersPanelResult {
    let prev_query = app.filters.query.clone();
    let (apply, open_settings, open_logs, open_about) = draw_filters_panel(
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
    FiltersPanelResult {
        apply,
        open_settings,
        open_logs,
        open_about,
        prev_query,
    }
}

/// Handle filter changes - trigger immediate fetch
fn handle_filter_apply(app: &mut NoLagApp, ctx: &egui::Context) {
    app.page = 1;
    app.filters.search_due_at = None;
    if app.filters.library_only {
        app.start_fetch_library(ctx);
    } else {
        app.start_fetch(ctx);
    }
}

/// Handle query text changes with debounce
fn handle_query_debounce(
    app: &mut NoLagApp,
    ctx: &egui::Context,
    query_changed: bool,
    apply: bool,
) {
    if !query_changed {
        return;
    }

    if apply {
        // Filters changed this frame and already triggered immediate fetch; skip debounce
        app.filters.search_due_at = None;
    } else {
        app.page = 1;
        let debounce = Duration::from_millis(crate::ui_constants::SEARCH_DEBOUNCE_MS);
        app.filters.search_due_at = Some(Instant::now() + debounce);
        ctx.request_repaint_after(debounce);
    }
}

/// Handle panel button clicks (settings, logs, about)
fn handle_panel_buttons(ctx: &egui::Context, result: &FiltersPanelResult) {
    if result.open_settings {
        settings::open_settings();
        ctx.request_repaint();
    }
    if result.open_logs {
        logs_ui::open_logs();
        ctx.request_repaint();
    }
    if result.open_about {
        about_ui::open_about();
        ctx.request_repaint();
    }
}

/// Auto-save selected tags if enabled in settings
fn autosave_selected_tags(app: &NoLagApp) {
    let do_autosave = settings::with_settings(|s| s.autosave_selected_tags);
    if !do_autosave {
        return;
    }

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

/// Handle library mode toggle
fn handle_library_mode_toggle(app: &mut NoLagApp, ctx: &egui::Context) {
    if app.filters.last_library_only == app.filters.library_only {
        return;
    }

    app.filters.last_library_only = app.filters.library_only;

    if app.filters.library_only {
        // Если фоновые данные уже есть — мгновенно показываем их
        if let Some(msg) = &app.net.lib_result {
            app.net.last_result = Some(msg.clone());
            app.net.last_error = None;
            app.net.loading = false;
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

/// Run debounced query fetch if deadline passed
fn run_debounced_fetch(app: &mut NoLagApp, ctx: &egui::Context) {
    let Some(due) = app.filters.search_due_at else {
        return;
    };

    if Instant::now() >= due {
        app.filters.search_due_at = None;
        if app.filters.library_only {
            app.start_fetch_library(ctx);
        } else {
            app.start_fetch(ctx);
        }
    }
}

/// Draw the central panel with threads grid
fn draw_central_panel(app: &mut NoLagApp, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                render_central_content(ui, app, ctx);
            });
    });
}

/// Draw floating overlays and separate viewports
fn draw_overlays_and_viewports(ctx: &egui::Context) {
    let bottom_offset = update_ui::draw_update_notice(ctx);
    errors_ui::draw_errors_button(ctx, bottom_offset);
    errors_ui::draw_errors_viewport(ctx);
    logs_ui::draw_logs_viewport(ctx);
    about_ui::draw_about_viewport(ctx);
    settings::draw_settings_viewport(ctx);
}

pub(super) fn update_main(app: &mut NoLagApp, ctx: &egui::Context) {
    // 1. Poll state
    poll_app_state(app, ctx);

    // 2. Initial fetch on startup
    handle_initial_fetch(app, ctx);

    // 3. Draw filters panel
    let result = draw_filters(app, ctx);

    // 4. Handle filter changes
    if result.apply {
        handle_filter_apply(app, ctx);
    }

    // 5. Handle query debounce
    let query_changed = app.filters.query != result.prev_query;
    handle_query_debounce(app, ctx, query_changed, result.apply);

    // 6. Handle panel buttons
    handle_panel_buttons(ctx, &result);

    // 7. Auto-save tags if filters changed
    if result.apply {
        autosave_selected_tags(app);
    }

    // 8. Handle library mode toggle
    handle_library_mode_toggle(app, ctx);

    // 9. Run debounced fetch
    run_debounced_fetch(app, ctx);

    // 10. Draw central panel
    draw_central_panel(app, ctx);

    // 11. Draw overlays and viewports
    draw_overlays_and_viewports(ctx);
}
