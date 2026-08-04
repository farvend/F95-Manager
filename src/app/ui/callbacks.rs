use super::*;

pub(super) fn wire_callbacks(
    ui: &MainWindow,
    state: SharedState,
    settings_window: Rc<SettingsWindow>,
    logs_window: Rc<LogsWindow>,
    about_window: Rc<AboutWindow>,
    errors_window: Rc<ErrorsWindow>,
    bookmarks_window: Rc<BookmarksWindow>,
) {
    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_login(move |username, password| {
        let username = username.to_string();
        let password = password.to_string();
        let use_environment = username.trim().is_empty() && password.is_empty();
        if !use_environment && (username.trim().is_empty() || password.is_empty()) {
            if let Some(ui) = weak.upgrade() {
                ui.set_auth_error(tr("auth-please-enter-credentials"));
            }
            return;
        }
        if let Some(ui) = weak.upgrade() {
            ui.set_auth_busy(true);
            ui.set_auth_error("".into());
        }
        let weak = weak.clone();
        let state = callback_state.clone();
        crate::app::rt().spawn(async move {
            let result = if use_environment {
                crate::app::config::login_from_env_and_store().await
            } else {
                crate::app::config::login_and_store(username, password).await
            };
            let state_for_load = state.clone();
            let _ = weak.upgrade_in_event_loop(move |ui| {
                ui.set_auth_busy(false);
                match result {
                    Ok(()) => {
                        ui.set_authenticated(true);
                        ui.set_auth_password("".into());
                        preload_library_data(state_for_load.clone(), ui.as_weak());
                        if state_for_load
                            .lock()
                            .map(|state| state.library_mode)
                            .unwrap_or(false)
                        {
                            load_library(state_for_load.clone(), ui.as_weak());
                        } else {
                            load_catalog(1, String::new(), state_for_load.clone(), ui.as_weak());
                        }
                    }
                    Err(error) => ui.set_auth_error(error.into()),
                }
            });
        });
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_use_cookies(move |cookies, username| {
        let cookies = cookies.trim().to_string();
        if cookies.is_empty() {
            if let Some(ui) = weak.upgrade() {
                ui.set_auth_error(tr("auth-please-paste-cookies"));
            }
            return;
        }
        {
            let mut config = crate::app::config::APP_CONFIG.write().unwrap();
            config.cookies = Some(cookies);
            if !username.trim().is_empty() {
                config.username = Some(username.to_string());
            }
        }
        crate::app::config::save_config_to_disk();
        if let Some(ui) = weak.upgrade() {
            ui.set_authenticated(true);
            ui.set_auth_error("".into());
            preload_library_data(callback_state.clone(), weak.clone());
            if callback_state
                .lock()
                .map(|state| state.library_mode)
                .unwrap_or(false)
            {
                load_library(callback_state.clone(), weak.clone());
            } else {
                load_catalog(1, String::new(), callback_state.clone(), weak.clone());
            }
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_refresh_catalog(move |query| {
        let library = callback_state
            .lock()
            .map(|mut state| {
                state.query = query.to_string();
                state.library_mode
            })
            .unwrap_or(false);
        if library {
            load_library(callback_state.clone(), weak.clone());
        } else {
            load_catalog(1, query.to_string(), callback_state.clone(), weak.clone());
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_previous_page(move || {
        let (page, query, library) = callback_state
            .lock()
            .map(|state| {
                (
                    state.page.saturating_sub(1).max(1),
                    state.query.clone(),
                    state.library_mode,
                )
            })
            .unwrap_or((1, String::new(), false));
        if !library {
            load_catalog(page, query, callback_state.clone(), weak.clone());
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_next_page(move || {
        let (page, query, library) = callback_state
            .lock()
            .map(|state| {
                (
                    (state.page + 1).min(state.total_pages.max(1)),
                    state.query.clone(),
                    state.library_mode,
                )
            })
            .unwrap_or((1, String::new(), false));
        if !library {
            load_catalog(page, query, callback_state.clone(), weak.clone());
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_toggle_library(move || {
        let switch_to_library = callback_state
            .lock()
            .map(|mut state| {
                state.library_mode = !state.library_mode;
                state.library_mode
            })
            .unwrap_or(false);
        if switch_to_library {
            load_library(callback_state.clone(), weak.clone());
        } else {
            let (page, query) = callback_state
                .lock()
                .map(|state| (state.page, state.query.clone()))
                .unwrap_or((1, String::new()));
            load_catalog(page, query, callback_state.clone(), weak.clone());
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_toggle_bookmark(move |bookmark_id| {
        let bookmark_id = bookmark_id.to_string();
        if let Ok(mut state) = callback_state.lock() {
            if !state.selected_bookmarks.remove(&bookmark_id) {
                state.selected_bookmarks.insert(bookmark_id);
            }
            state.library_mode = true;
            if let Ok(mut settings) = crate::app::settings::APP_SETTINGS.write() {
                settings.filter_bookmarks = state.selected_bookmarks.iter().cloned().collect();
            }
        }
        crate::app::settings::save_settings_to_disk();
        let state_for_ui = callback_state.clone();
        let _ = weak.upgrade_in_event_loop(move |ui| {
            ui.set_library_mode(true);
            update_bookmarks(&ui, &state_for_ui);
        });
        load_library(callback_state.clone(), weak.clone());
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_layout_width_changed(move |width| {
        let (ui_scale, card_scale) = weak
            .upgrade()
            .map(|ui| (ui.get_ui_scale_percent(), ui.get_card_scale_percent()))
            .unwrap_or((100, 100));
        let scale = ui_scale as f32 / 100.0;
        let card_width = 320.0 * scale * card_scale as f32 / 100.0;
        let gap = 10.0 * scale;
        let available = (width - 24.0 * scale).max(card_width);
        let columns = ((available + gap) / (card_width + gap).max(1.0))
            .floor()
            .max(1.0) as usize;
        let changed = callback_state
            .lock()
            .map(|mut state| {
                if state.columns == columns {
                    false
                } else {
                    state.columns = columns;
                    true
                }
            })
            .unwrap_or(false);
        if changed {
            let state_for_ui = callback_state.clone();
            let _ = weak.upgrade_in_event_loop(move |ui| rebuild_cards(&ui, &state_for_ui));
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_image_needed(move |id| {
        if let Ok(id) = id.parse::<u64>() {
            request_screens(id, callback_state.clone(), weak.clone());
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_primary_action(move |id| {
        if let Ok(id) = id.parse::<u64>() {
            let installed = callback_state.lock().ok().is_some_and(|state| {
                state
                    .cards
                    .iter()
                    .any(|card| card.id == id && card.installed)
            });
            if installed {
                crate::app::settings::run_downloaded_game(id);
                let (library_mode, unplayed_only) = callback_state
                    .lock()
                    .map(|state| (state.library_mode, state.unplayed_only))
                    .unwrap_or((false, false));
                // Launching only changes the "unplayed" state. Replacing the
                // complete card model here resets Slint's scroll position, so
                // only refilter when the active filter can remove this card.
                if library_mode && unplayed_only {
                    load_library(callback_state.clone(), weak.clone());
                } else if let Some(ui) = weak.upgrade() {
                    rebuild_cards(&ui, &callback_state);
                }
            } else {
                let should_start = callback_state.lock().ok().is_some_and(|state| {
                    state
                        .downloads
                        .get(&id)
                        .is_none_or(|job| matches!(job.progress, Progress::Error(_)))
                });
                if should_start {
                    if let Some(card) = callback_state
                        .lock()
                        .ok()
                        .and_then(|state| state.cards.iter().find(|card| card.id == id).cloned())
                    {
                        cache_card_metadata(&card);
                    }
                    let receiver = crate::game_download::create_download_task(
                        crate::parser::game_info::ThreadId(id).get_page(),
                    );
                    if let Ok(mut state) = callback_state.lock() {
                        state.downloads.insert(
                            id,
                            DownloadJob {
                                progress: Progress::Unknown,
                                link_choices: Vec::new(),
                            },
                        );
                    }
                    crate::app::settings::record_pending_download(id);
                    if let Some(ui) = weak.upgrade() {
                        update_card_download(&ui, &callback_state, id);
                    }
                    watch_download(id, receiver, callback_state.clone(), weak.clone());
                }
            }
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_choose_download_link(move |id, link_id| {
        let (Ok(id), Ok(index)) = (id.parse::<u64>(), link_id.parse::<usize>()) else {
            return;
        };
        let link = callback_state.lock().ok().and_then(|mut state| {
            let job = state.downloads.get_mut(&id)?;
            if index >= job.link_choices.len() {
                return None;
            }
            let link = job.link_choices[index].clone();
            job.link_choices.clear();
            job.progress = Progress::Unknown;
            Some(link)
        });
        if let Some(link) = link {
            let receiver = crate::game_download::create_download_from_link(link);
            watch_download(id, receiver, callback_state.clone(), weak.clone());
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_cancel_download_link(move |id| {
        let Ok(id) = id.parse::<u64>() else { return };
        if let Ok(mut state) = callback_state.lock() {
            state.downloads.remove(&id);
        }
        crate::app::settings::remove_pending_download(id);
        if let Some(ui) = weak.upgrade() {
            update_card_download(&ui, &callback_state, id);
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_remove_pending(move |id| {
        let Ok(id) = id.parse::<u64>() else { return };
        let removable = callback_state
            .lock()
            .map(|state| !state.downloads.contains_key(&id))
            .unwrap_or(false);
        if !removable {
            return;
        }
        crate::app::settings::remove_pending_download(id);
        if let Ok(mut state) = callback_state.lock() {
            if state.library_mode {
                state.cards.retain(|card| card.id != id);
            }
        }
        if let Some(ui) = weak.upgrade() {
            rebuild_cards(&ui, &callback_state);
        }
    });

    ui.on_open_thread(move |id| {
        if let Ok(id) = id.parse::<u64>() {
            crate::app::settings::open_in_browser(&format!("https://f95zone.to/threads/{id}/"));
        }
    });

    let callback_state = state.clone();
    ui.on_open_folder(move |id| {
        if let Ok(id) = id.parse::<u64>() {
            if let Some(folder) = callback_state.lock().ok().and_then(|state| {
                state
                    .cards
                    .iter()
                    .find(|card| card.id == id)
                    .and_then(|card| card.folder.clone())
            }) {
                crate::app::settings::reveal_in_file_manager(&folder);
            }
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_hide_game(move |id| {
        if let Ok(id) = id.parse::<u64>() {
            crate::app::settings::hide_thread(id);
            if let Ok(mut state) = callback_state.lock() {
                state.cards.retain(|card| card.id != id);
            }
            let state_for_ui = callback_state.clone();
            let _ = weak.upgrade_in_event_loop(move |ui| rebuild_cards(&ui, &state_for_ui));
        }
    });

    let weak = ui.as_weak();
    ui.on_prepare_game_bookmarks(move |id| {
        if let Ok(id) = id.parse::<u64>() {
            if let Some(ui) = weak.upgrade() {
                ui.set_context_game_bookmarks(game_bookmark_model(id));
            }
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_toggle_game_bookmark(move |id, bookmark_id| {
        let Ok(id) = id.parse::<u64>() else { return };
        let bookmark_id = bookmark_id.to_string();
        if let Ok(mut settings) = crate::app::settings::APP_SETTINGS.write() {
            if let Some(game) = settings
                .downloaded_games
                .iter_mut()
                .find(|game| game.thread_id == id)
            {
                if game.bookmark_ids.iter().any(|item| item == &bookmark_id) {
                    game.bookmark_ids.retain(|item| item != &bookmark_id);
                } else {
                    game.bookmark_ids.push(bookmark_id);
                }
            }
        }
        crate::app::settings::save_settings_to_disk();
        if let Some(ui) = weak.upgrade() {
            ui.set_context_game_bookmarks(game_bookmark_model(id));
            rebuild_cards(&ui, &callback_state);
            update_bookmarks(&ui, &callback_state);
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_refresh_game(move |id| {
        let Ok(id) = id.parse::<u64>() else { return };
        let weak = weak.clone();
        let state = callback_state.clone();
        crate::app::rt().spawn(async move {
            match crate::parser::game_info::thread_meta::fetch_thread_meta(id).await {
                Ok(meta) => {
                    let (cache_dir, folder) = {
                        let cache_dir = crate::app::settings::APP_SETTINGS
                            .read()
                            .map(|settings| settings.cache_dir.clone())
                            .unwrap_or_else(|_| PathBuf::from("cache"));
                        let card = state.lock().ok().and_then(|state| {
                            state
                                .cards
                                .iter()
                                .find(|card| card.id == id)
                                .map(|card| card.folder.clone())
                        });
                        (cache_dir, card.flatten())
                    };
                    let thread = F95Thread {
                        thread_id: crate::parser::game_info::ThreadId(id),
                        title: meta.title,
                        creator: meta.creator,
                        version: meta.version,
                        cover: meta.cover,
                        screens: meta.screens,
                        tags: meta.tag_ids,
                        views: 0,
                        likes: 0,
                        prefixes: meta.prefix_ids,
                        rating: 0.0,
                        date: String::new(),
                        watched: false,
                        ignored: false,
                        is_new: false,
                        ts: 0,
                    };
                    if let Err(error) =
                        crate::app::fetch_helpers::save_to_cache(&cache_dir, id, &thread)
                    {
                        log::warn!("Failed to save refreshed cache for {id}: {error}");
                    }
                    let cached_cover = cache_dir.join(id.to_string()).join("cover.png");
                    let refreshed = card_from_thread(
                        thread,
                        true,
                        folder,
                        cached_cover.exists().then_some(cached_cover),
                    );
                    if let Ok(mut state) = state.lock() {
                        if let Some(card) = state.cards.iter_mut().find(|card| card.id == id) {
                            *card = refreshed;
                        }
                        state.loaded_images.remove(&id);
                        state.loading_images.remove(&id);
                        state.loaded_screen_images.retain(|(game, _)| *game != id);
                        state.loaded_screens.remove(&id);
                        state.loading_screens.remove(&id);
                    }
                    let state_for_ui = state.clone();
                    request_all_covers(state.clone(), weak.clone());
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        UI_IMAGE_CACHE.with(|cache| {
                            let mut cache = cache.borrow_mut();
                            cache.covers.remove(&id);
                            cache.screens.retain(|(game, _), _| *game != id);
                        });
                        rebuild_cards(&ui, &state_for_ui);
                    });
                }
                Err(error) => {
                    append_error(format!("Refresh failed for thread {id}: {error}"), &weak)
                }
            }
        });
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_delete_game(move |id| {
        let Ok(id) = id.parse::<u64>() else { return };
        let weak = weak.clone();
        let state = callback_state.clone();
        crate::app::rt().spawn(async move {
            let _ = tokio::task::spawn_blocking(move || {
                crate::app::settings::delete_downloaded_game(id)
            })
            .await;
            if let Ok(mut state) = state.lock() {
                state.cards.retain(|card| card.id != id);
                state.loaded_images.remove(&id);
                state.loading_images.remove(&id);
                state.loaded_screen_images.retain(|(game, _)| *game != id);
            }
            let state_for_ui = state.clone();
            let _ = weak.upgrade_in_event_loop(move |ui| {
                rebuild_cards(&ui, &state_for_ui);
                update_bookmarks(&ui, &state_for_ui);
            });
        });
    });

    let current_settings = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| settings.clone())
        .unwrap_or_default();
    settings_window.set_ui_scale_percent(current_settings.ui_scale_percent as f32);
    settings_window.set_card_scale_percent(current_settings.card_scale_percent as f32);
    settings_window.set_image_cache_games(current_settings.image_cache_games.into());
    settings_window.set_cache_directory(
        current_settings
            .cache_dir
            .to_string_lossy()
            .to_string()
            .into(),
    );
    settings_window.set_temp_directory(
        current_settings
            .temp_dir
            .to_string_lossy()
            .to_string()
            .into(),
    );
    settings_window.set_games_directory(
        current_settings
            .extract_dir
            .to_string_lossy()
            .to_string()
            .into(),
    );
    settings_window.set_custom_launch(current_settings.custom_launch.clone().into());
    settings_window.set_log_to_file(current_settings.log_to_file);
    settings_window.set_show_unplayed(current_settings.show_unplayed_badge);
    settings_window.set_classic_library(current_settings.classic_library_toggle);
    settings_window.set_language(match current_settings.language {
        crate::localization::LanguageChoice::English => LanguageChoice::English,
        crate::localization::LanguageChoice::Russian => LanguageChoice::Russian,
        crate::localization::LanguageChoice::Automatic => LanguageChoice::Automatic,
    });
    settings_window.set_loading_indicator(match current_settings.loading_anim {
        crate::app::settings::store::LoadingAnim::CircleBottomRight => {
            LoadingIndicatorChoice::CircleBottomRight
        }
        _ => LoadingIndicatorChoice::BottomBar,
    });
    settings_window.set_bookmarks_visible(current_settings.bookmarks_visible_on_cover.into());
    settings_window.set_bookmark_red(current_settings.default_bookmark_color[0].into());
    settings_window.set_bookmark_green(current_settings.default_bookmark_color[1].into());
    settings_window.set_bookmark_blue(current_settings.default_bookmark_color[2].into());
    let settings_filters = Rc::new(RefCell::new(SettingsFilterState::from_settings(
        &current_settings,
    )));
    update_settings_filter_models(&settings_window, &settings_filters.borrow());

    let bookmarks_for_open = bookmarks_window.clone();
    settings_window.on_open_bookmarks(move || {
        update_bookmark_editor(&bookmarks_for_open);
        show_and_focus(bookmarks_for_open.as_ref());
    });

    let bookmarks_for_new = bookmarks_window.clone();
    bookmarks_window.on_create_new(move || {
        bookmarks_for_new.set_selected_id("".into());
        bookmarks_for_new.set_emoji("".into());
        bookmarks_for_new.set_label("".into());
    });

    let bookmarks_for_select = bookmarks_window.clone();
    bookmarks_window.on_select(move |id| {
        let id = id.to_string();
        if let Some(bookmark) = crate::app::settings::get_bookmark(&id) {
            let color = bookmark.color.unwrap_or([60, 120, 200]);
            bookmarks_for_select.set_selected_id(bookmark.id.into());
            bookmarks_for_select.set_emoji(bookmark.emoji.into());
            bookmarks_for_select.set_label(bookmark.label.into());
            bookmarks_for_select.set_red(color[0].into());
            bookmarks_for_select.set_green(color[1].into());
            bookmarks_for_select.set_blue(color[2].into());
        }
    });

    let bookmarks_for_save = bookmarks_window.clone();
    let main_for_bookmarks = ui.as_weak();
    let state_for_bookmarks = state.clone();
    bookmarks_window.on_save(move |id, emoji, label, red, green, blue| {
        let label = label.trim().to_string();
        if label.is_empty() {
            return;
        }
        let color = Some([
            red.clamp(0, 255) as u8,
            green.clamp(0, 255) as u8,
            blue.clamp(0, 255) as u8,
        ]);
        let saved_id = if id.is_empty() {
            crate::app::settings::create_bookmark(emoji.to_string(), label, color)
        } else {
            crate::app::settings::update_bookmark(id.as_str(), emoji.to_string(), label, color);
            id.to_string()
        };
        bookmarks_for_save.set_selected_id(saved_id.into());
        update_bookmark_editor(&bookmarks_for_save);
        if let Some(main) = main_for_bookmarks.upgrade() {
            update_bookmarks(&main, &state_for_bookmarks);
            rebuild_cards(&main, &state_for_bookmarks);
        }
    });

    let bookmarks_for_delete = bookmarks_window.clone();
    let main_for_delete_bookmark = ui.as_weak();
    let state_for_delete_bookmark = state.clone();
    bookmarks_window.on_delete(move |id| {
        crate::app::settings::delete_bookmark(id.as_str());
        bookmarks_for_delete.set_selected_id("".into());
        bookmarks_for_delete.set_emoji("".into());
        bookmarks_for_delete.set_label("".into());
        update_bookmark_editor(&bookmarks_for_delete);
        if let Some(main) = main_for_delete_bookmark.upgrade() {
            update_bookmarks(&main, &state_for_delete_bookmark);
            rebuild_cards(&main, &state_for_delete_bookmark);
        }
    });

    let settings_for_query = settings_window.clone();
    settings_window.on_settings_suggestion_query(move |query, kind| {
        settings_for_query.set_filter_suggestion_kind(kind);
        let lookup_kind = if matches!(kind, 2 | 3 | 5) { 2 } else { 0 };
        settings_for_query
            .set_filter_suggestions(matching_suggestions(query.as_str(), lookup_kind));
    });

    let filters_for_add = settings_filters.clone();
    let settings_for_add = settings_window.clone();
    settings_window.on_add_settings_filter(move |id, kind| {
        let Ok(id) = id.parse::<u32>() else { return };
        let kind = kind.clamp(0, 5) as usize;
        let mut filters = filters_for_add.borrow_mut();
        let values = &mut filters.values[kind];
        let limit_reached = kind < 4 && values.len() >= 10;
        if !limit_reached && !values.contains(&id) {
            values.push(id);
        }
        update_settings_filter_models(&settings_for_add, &filters);
        settings_for_add.set_filter_suggestions(empty_suggestions());
        settings_for_add.set_filter_suggestion_kind(-1);
    });

    let filters_for_remove = settings_filters.clone();
    let settings_for_remove = settings_window.clone();
    settings_window.on_remove_settings_filter(move |id, kind| {
        let Ok(id) = id.parse::<u32>() else { return };
        let kind = kind.clamp(0, 5) as usize;
        let mut filters = filters_for_remove.borrow_mut();
        filters.values[kind].retain(|value| *value != id);
        update_settings_filter_models(&settings_for_remove, &filters);
    });

    let settings_for_directory = settings_window.clone();
    settings_window.on_choose_directory(move |kind| {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            let path = path.to_string_lossy().to_string().into();
            match kind {
                0 => settings_for_directory.set_temp_directory(path),
                1 => settings_for_directory.set_games_directory(path),
                _ => settings_for_directory.set_cache_directory(path),
            }
        }
    });

    let main_weak = ui.as_weak();
    let main_for_preview = ui.as_weak();
    settings_window.on_preview_scale(move |ui_scale, card_scale| {
        let ui_scale = ui_scale.clamp(50.0, 200.0).round() as i32;
        let card_scale = card_scale.clamp(50.0, 200.0).round() as i32;
        if let Some(main) = main_for_preview.upgrade() {
            main.set_ui_scale_percent(ui_scale);
            main.set_card_scale_percent(card_scale);
            main.global::<AppTheme>().set_scale(ui_scale as f32 / 100.0);
            main.global::<AppTheme>()
                .set_card_scale(card_scale as f32 / 100.0);
            main.global::<AppTheme>()
                .set_card_width(320.0 * ui_scale as f32 / 100.0 * card_scale as f32 / 100.0);
        }
    });

    let settings_for_save = settings_window.clone();
    let logs_for_language = logs_window.clone();
    let about_for_language = about_window.clone();
    let errors_for_language = errors_window.clone();
    let bookmarks_for_language = bookmarks_window.clone();
    let filters_for_save = settings_filters.clone();
    let state_for_settings_save = state.clone();
    let pending_migration = Rc::new(RefCell::new(None::<(PathBuf, PathBuf)>));
    let pending_migration_for_save = pending_migration.clone();
    settings_window.on_save_settings(
        move |ui_scale,
              card_scale,
              image_cache_games,
              temp_directory,
              games_directory,
              cache_directory,
              custom_launch,
              log_to_file,
              show_unplayed,
              classic_library,
              language,
              loading,
              bookmarks_visible,
              bookmark_red,
              bookmark_green,
              bookmark_blue| {
            let ui_scale = ui_scale.clamp(50.0, 200.0) as u16;
            let card_scale = card_scale.clamp(50.0, 200.0) as u16;
            let requested_extract_dir = PathBuf::from(games_directory.as_str());
            let (old_extract_dir, has_installed_games) = crate::app::settings::APP_SETTINGS
                .read()
                .map(|settings| {
                    (
                        settings.extract_dir.clone(),
                        !settings.downloaded_games.is_empty(),
                    )
                })
                .unwrap_or_default();
            let needs_migration = has_installed_games && requested_extract_dir != old_extract_dir;
            let selected_language = match language {
                LanguageChoice::English => crate::localization::LanguageChoice::English,
                LanguageChoice::Russian => crate::localization::LanguageChoice::Russian,
                LanguageChoice::Automatic => crate::localization::LanguageChoice::Automatic,
            };
            if let Ok(mut settings) = crate::app::settings::APP_SETTINGS.write() {
                settings.ui_scale_percent = ui_scale;
                settings.card_scale_percent = card_scale;
                settings.image_cache_games = image_cache_games.clamp(1, 100) as u16;
                settings.temp_dir = PathBuf::from(temp_directory.as_str());
                settings.extract_dir = if needs_migration {
                    old_extract_dir.clone()
                } else {
                    requested_extract_dir.clone()
                };
                settings.cache_dir = PathBuf::from(cache_directory.as_str());
                settings.custom_launch = custom_launch.to_string();
                settings.log_to_file = log_to_file;
                settings.show_unplayed_badge = show_unplayed;
                settings.classic_library_toggle = classic_library;
                let filters = filters_for_save.borrow();
                settings.startup_tags = filters.values[0].clone();
                settings.startup_exclude_tags = filters.values[1].clone();
                settings.startup_prefixes = filters.values[2].clone();
                settings.startup_exclude_prefixes = filters.values[3].clone();
                settings.warn_tags = filters.values[4].clone();
                settings.warn_prefixes = filters.values[5].clone();
                settings.bookmarks_visible_on_cover = bookmarks_visible.clamp(1, 5) as u8;
                settings.default_bookmark_color = [
                    bookmark_red.clamp(0, 255) as u8,
                    bookmark_green.clamp(0, 255) as u8,
                    bookmark_blue.clamp(0, 255) as u8,
                ];
                settings.language = selected_language;
                settings.loading_anim = match loading {
                    LoadingIndicatorChoice::CircleBottomRight => {
                        crate::app::settings::store::LoadingAnim::CircleBottomRight
                    }
                    LoadingIndicatorChoice::BottomBar => {
                        crate::app::settings::store::LoadingAnim::BottomBar
                    }
                };
            }
            crate::logger::set_file_logging_enabled(log_to_file);
            crate::app::settings::save_settings_to_disk();
            if needs_migration {
                *pending_migration_for_save.borrow_mut() =
                    Some((old_extract_dir, requested_extract_dir));
                settings_for_save.set_migration_message("".into());
                settings_for_save.set_migration_confirm_visible(true);
            }
            let _ = crate::localization::set_language_choice(selected_language);
            if let Some(main) = main_weak.upgrade() {
                update_all_translations(
                    &main,
                    &settings_for_save,
                    &logs_for_language,
                    &about_for_language,
                    &errors_for_language,
                    &bookmarks_for_language,
                );
                settings_for_save.set_saved_message(tr("common-saved"));
                main.set_ui_scale_percent(ui_scale.into());
                main.set_card_scale_percent(card_scale.into());
                main.set_classic_library(classic_library);
                main.global::<AppTheme>().set_scale(ui_scale as f32 / 100.0);
                main.global::<AppTheme>()
                    .set_card_scale(card_scale as f32 / 100.0);
                main.global::<AppTheme>()
                    .set_card_width(320.0 * ui_scale as f32 / 100.0 * card_scale as f32 / 100.0);
                prune_screen_cache(&main, &state_for_settings_save);
                rebuild_cards(&main, &state_for_settings_save);
            }
        },
    );

    let pending_migration_for_confirm = pending_migration.clone();
    let settings_for_migration = settings_window.clone();
    settings_window.on_confirm_migration(move |confirmed| {
        if !confirmed {
            *pending_migration_for_confirm.borrow_mut() = None;
            settings_for_migration.set_migration_confirm_visible(false);
            return;
        }
        let Some((old_dir, new_dir)) = pending_migration_for_confirm.borrow_mut().take() else {
            settings_for_migration.set_migration_confirm_visible(false);
            return;
        };
        settings_for_migration.set_migration_running(true);
        let entries = crate::app::settings::APP_SETTINGS
            .read()
            .map(|settings| {
                settings
                    .downloaded_games
                    .iter()
                    .map(|game| (game.thread_id, game.folder.clone(), game.exe_path.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let weak_settings = settings_for_migration.as_weak();
        crate::app::rt().spawn_blocking(move || {
            let moved =
                crate::app::settings::migrate::migrate_installed_games(&old_dir, &new_dir, entries);
            if let Ok(mut settings) = crate::app::settings::APP_SETTINGS.write() {
                settings.extract_dir = new_dir;
                for (id, folder, exe_path) in moved {
                    if let Some(game) = settings
                        .downloaded_games
                        .iter_mut()
                        .find(|game| game.thread_id == id)
                    {
                        game.folder = folder;
                        game.exe_path = exe_path;
                    }
                }
            }
            crate::app::settings::save_settings_to_disk();
            let _ = weak_settings.upgrade_in_event_loop(|window| {
                window.set_migration_running(false);
                window.set_migration_confirm_visible(false);
                window.set_saved_message(tr("common-saved"));
            });
        });
    });

    let settings_for_open = settings_window.clone();
    ui.on_open_settings(move || {
        settings_for_open.set_saved_message("".into());
        show_and_focus(settings_for_open.as_ref());
    });
    let about_for_open = about_window.clone();
    ui.on_open_about(move || {
        show_and_focus(about_for_open.as_ref());
    });
    let logs_for_refresh = logs_window.clone();
    logs_window.on_refresh(move || refresh_logs(&logs_for_refresh));
    let logs_for_clear = logs_window.clone();
    logs_window.on_clear(move || {
        crate::logger::clear();
        refresh_logs(&logs_for_clear);
    });
    logs_window.on_copy(move || {
        let text = crate::logger::get_all().join("\n");
        if let Err(error) = clipboard_win::set_clipboard_string(&text) {
            log::warn!("Failed to copy logs: {error}");
        }
    });
    let logs_for_open = logs_window.clone();
    ui.on_open_logs(move || {
        refresh_logs(&logs_for_open);
        show_and_focus(logs_for_open.as_ref());
    });

    let errors_for_open = errors_window.clone();
    ui.on_open_errors(move || {
        refresh_errors(&errors_for_open);
        show_and_focus(errors_for_open.as_ref());
    });
    let errors_for_clear = errors_window.clone();
    let main_for_errors = ui.as_weak();
    errors_window.on_clear(move || clear_errors(&errors_for_clear, &main_for_errors));
    errors_window.on_copy(copy_errors);

    let weak = ui.as_weak();
    ui.on_suggestion_query(move |query, kind| {
        let suggestions = matching_suggestions(query.as_str(), if kind >= 2 { 2 } else { kind });
        if let Some(ui) = weak.upgrade() {
            match kind {
                0 => ui.set_include_tag_suggestions(suggestions),
                1 => ui.set_exclude_tag_suggestions(suggestions),
                2 => ui.set_prefix_suggestions(suggestions),
                _ => ui.set_exclude_prefix_suggestions(suggestions),
            }
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_filter_chosen(move |id, kind| {
        let Ok(id) = id.parse::<u32>() else { return };
        let (page, query) = callback_state
            .lock()
            .map(|mut state| {
                let target = match kind {
                    0 => &mut state.include_tags,
                    1 => &mut state.exclude_tags,
                    2 => &mut state.prefixes,
                    _ => &mut state.exclude_prefixes,
                };
                if !target.contains(&id) && target.len() < 10 {
                    target.push(id);
                }
                (1, state.query.clone())
            })
            .unwrap_or((1, String::new()));
        if let Some(ui) = weak.upgrade() {
            update_selected_filters(&ui, &callback_state);
        }
        let library = callback_state
            .lock()
            .map(|state| state.library_mode)
            .unwrap_or(false);
        if library {
            load_library(callback_state.clone(), weak.clone());
        } else {
            load_catalog(page, query, callback_state.clone(), weak.clone());
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_remove_filter(move |id, kind| {
        let Ok(id) = id.parse::<u32>() else { return };
        let query = callback_state
            .lock()
            .map(|mut state| {
                let target = match kind {
                    0 => &mut state.include_tags,
                    1 => &mut state.exclude_tags,
                    2 => &mut state.prefixes,
                    _ => &mut state.exclude_prefixes,
                };
                target.retain(|value| *value != id);
                state.query.clone()
            })
            .unwrap_or_default();
        if let Some(ui) = weak.upgrade() {
            update_selected_filters(&ui, &callback_state);
        }
        let library = callback_state
            .lock()
            .map(|state| state.library_mode)
            .unwrap_or(false);
        if library {
            load_library(callback_state.clone(), weak.clone());
        } else {
            load_catalog(1, query, callback_state.clone(), weak.clone());
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_sort_chosen(move |index| {
        let query = callback_state
            .lock()
            .map(|mut state| {
                state.sorting = sorting_from_index(index);
                state.query.clone()
            })
            .unwrap_or_default();
        if let Some(ui) = weak.upgrade() {
            ui.set_sort_index(index.clamp(0, 4));
        }
        let library = callback_state
            .lock()
            .map(|state| state.library_mode)
            .unwrap_or(false);
        if library {
            load_library(callback_state.clone(), weak.clone());
        } else {
            load_catalog(1, query, callback_state.clone(), weak.clone());
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_date_chosen(move |index| {
        let index = index.clamp(0, 8);
        let query = callback_state
            .lock()
            .map(|mut state| {
                state.date_limit = date_limit_from_index(index);
                state.query.clone()
            })
            .unwrap_or_default();
        if let Some(ui) = weak.upgrade() {
            ui.set_date_index(index);
            ui.set_date_label(date_limit_label(index).into());
        }
        let library = callback_state
            .lock()
            .map(|state| state.library_mode)
            .unwrap_or(false);
        if library {
            load_library(callback_state.clone(), weak.clone());
        } else {
            load_catalog(1, query, callback_state.clone(), weak.clone());
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_filter_mode_chosen(move |kind, value| {
        if let Ok(mut state) = callback_state.lock() {
            if kind == 0 {
                state.include_logic = if value == 1 {
                    TagLogic::And
                } else {
                    TagLogic::Or
                };
            } else {
                state.search_mode = if value == 1 {
                    SearchMode::Creator
                } else {
                    SearchMode::Title
                };
            }
        }
        let (library, query) = callback_state
            .lock()
            .map(|state| (state.library_mode, state.query.clone()))
            .unwrap_or((false, String::new()));
        if library {
            load_library(callback_state.clone(), weak.clone());
        } else {
            load_catalog(1, query, callback_state.clone(), weak.clone());
        }
    });

    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_toggle_unplayed(move || {
        if let Ok(mut state) = callback_state.lock() {
            state.unplayed_only = !state.unplayed_only;
            if let Some(ui) = weak.upgrade() {
                ui.set_unplayed_only(state.unplayed_only);
            }
        }
        load_library(callback_state.clone(), weak.clone());
    });
}
