use crate::parser::{F95Filters, F95Thread};
use crate::types::{DateLimit, Sorting};
use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};

slint::include_modules!();

#[derive(Clone)]
struct CardRecord {
    id: u64,
    title: String,
    creator: String,
    version: String,
    prefix: String,
    date: String,
    likes: String,
    views: String,
    rating: String,
    cover_url: Option<String>,
    screens: Vec<String>,
    tags: Vec<u32>,
    prefixes: Vec<u32>,
    cached_cover: Option<PathBuf>,
    installed: bool,
    folder: Option<PathBuf>,
}

struct ImagePixels {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

struct UiState {
    cards: Vec<CardRecord>,
    columns: usize,
    page: u32,
    total_pages: u32,
    query: String,
    library_mode: bool,
    selected_bookmarks: HashSet<String>,
    include_tags: Vec<u32>,
    exclude_tags: Vec<u32>,
    prefixes: Vec<u32>,
    exclude_prefixes: Vec<u32>,
    sorting: Sorting,
    date_limit: DateLimit,
    loaded_images: HashSet<u64>,
    loading_images: HashSet<u64>,
    loaded_screen_images: HashSet<(u64, usize)>,
    loading_screens: HashSet<u64>,
    loaded_screens: HashSet<u64>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            cards: Vec::new(),
            columns: 3,
            page: 1,
            total_pages: 1,
            query: String::new(),
            library_mode: false,
            selected_bookmarks: HashSet::new(),
            include_tags: Vec::new(),
            exclude_tags: Vec::new(),
            prefixes: Vec::new(),
            exclude_prefixes: Vec::new(),
            sorting: Sorting::Date,
            date_limit: DateLimit::Anytime,
            loaded_images: HashSet::new(),
            loading_images: HashSet::new(),
            loaded_screen_images: HashSet::new(),
            loading_screens: HashSet::new(),
            loaded_screens: HashSet::new(),
        }
    }
}

type SharedState = Arc<Mutex<UiState>>;

#[derive(Default)]
struct UiImageCache {
    covers: HashMap<u64, Image>,
    screens: HashMap<(u64, usize), Image>,
    screen_games: VecDeque<u64>,
}

thread_local! {
    static UI_IMAGE_CACHE: RefCell<UiImageCache> = RefCell::new(UiImageCache::default());
}

#[derive(Default)]
struct SettingsFilterState {
    values: [Vec<u32>; 6],
}

impl SettingsFilterState {
    fn from_settings(settings: &crate::app::settings::AppSettings) -> Self {
        Self {
            values: [
                settings.startup_tags.clone(),
                settings.startup_exclude_tags.clone(),
                settings.startup_prefixes.clone(),
                settings.startup_exclude_prefixes.clone(),
                settings.warn_tags.clone(),
                settings.warn_prefixes.clone(),
            ],
        }
    }
}

pub fn run() -> Result<(), slint::PlatformError> {
    let renderer = std::env::var("F95_RENDERER").unwrap_or_else(|_| "femtovg".to_string());
    if let Err(error) = slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name(renderer.clone().into())
        .select()
    {
        if renderer == "software" {
            return Err(error);
        }
        log::warn!("Slint renderer {renderer} failed ({error}); falling back to software");
        slint::BackendSelector::new()
            .backend_name("winit".into())
            .renderer_name("software".into())
            .select()?;
    }

    let settings = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| settings.clone())
        .unwrap_or_default();
    let selected_bookmarks: HashSet<String> = settings.filter_bookmarks.iter().cloned().collect();
    let start_in_library = !selected_bookmarks.is_empty();
    let state = Arc::new(Mutex::new(UiState {
        library_mode: start_in_library,
        selected_bookmarks,
        include_tags: settings.startup_tags.clone(),
        exclude_tags: settings.startup_exclude_tags.clone(),
        prefixes: settings.startup_prefixes.clone(),
        exclude_prefixes: settings.startup_exclude_prefixes.clone(),
        ..UiState::default()
    }));

    let ui = MainWindow::new()?;
    let settings_window = Rc::new(SettingsWindow::new()?);
    let logs_window = Rc::new(LogsWindow::new()?);
    let about_window = Rc::new(AboutWindow::new()?);
    about_window.set_version(env!("CARGO_PKG_VERSION").into());
    ui.set_ui_scale_percent(settings.ui_scale_percent.into());
    ui.set_card_scale_percent(settings.card_scale_percent.into());
    ui.set_library_mode(start_in_library);
    ui.global::<Theme>()
        .set_scale(settings.ui_scale_percent as f32 / 100.0);
    ui.global::<Theme>()
        .set_card_scale(settings.card_scale_percent as f32 / 100.0);
    ui.global::<Theme>().set_card_width(
        320.0 * settings.ui_scale_percent as f32 / 100.0 * settings.card_scale_percent as f32
            / 100.0,
    );
    update_bookmarks(&ui, &state);
    update_selected_filters(&ui, &state);
    wire_callbacks(
        &ui,
        state.clone(),
        settings_window,
        logs_window,
        about_window,
    );
    preload_library_covers(state.clone(), ui.as_weak());

    if start_in_library {
        load_library(state, ui.as_weak());
    } else {
        load_catalog(1, String::new(), state, ui.as_weak());
    }

    ui.run()
}

fn wire_callbacks(
    ui: &MainWindow,
    state: SharedState,
    settings_window: Rc<SettingsWindow>,
    logs_window: Rc<LogsWindow>,
    about_window: Rc<AboutWindow>,
) {
    let weak = ui.as_weak();
    let callback_state = state.clone();
    ui.on_refresh_catalog(move |query| {
        load_catalog(1, query.to_string(), callback_state.clone(), weak.clone());
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

    let callback_state = state.clone();
    ui.on_primary_action(move |id| {
        if let Ok(id) = id.parse::<u64>() {
            let installed = callback_state
                .lock()
                .ok()
                .and_then(|state| {
                    state
                        .cards
                        .iter()
                        .find(|card| card.id == id)
                        .map(|card| card.installed)
                })
                .unwrap_or(false);
            if installed {
                crate::app::settings::run_downloaded_game(id);
            } else {
                crate::app::settings::open_in_browser(&format!("https://f95zone.to/threads/{id}/"));
            }
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
                    let (cache_dir, folder, prefixes) = {
                        let cache_dir = crate::app::settings::APP_SETTINGS
                            .read()
                            .map(|settings| settings.cache_dir.clone())
                            .unwrap_or_else(|_| PathBuf::from("cache"));
                        let card = state.lock().ok().and_then(|state| {
                            state
                                .cards
                                .iter()
                                .find(|card| card.id == id)
                                .map(|card| (card.folder.clone(), card.prefixes.clone()))
                        });
                        let (folder, prefixes) = card.unwrap_or_default();
                        (cache_dir, folder, prefixes)
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
                        prefixes,
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
                Err(error) => log::error!("Refresh failed for thread {id}: {error}"),
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
    settings_window.set_cache_on_download(current_settings.cache_on_download);
    settings_window.set_log_to_file(current_settings.log_to_file);
    settings_window.set_show_unplayed(current_settings.show_unplayed_badge);
    settings_window.set_classic_library(current_settings.classic_library_toggle);
    settings_window.set_language_index(match current_settings.language {
        Some(crate::localization::SupportedLang::English) => 1,
        Some(crate::localization::SupportedLang::Russian) => 2,
        None => 0,
    });
    settings_window.set_loading_index(match current_settings.loading_anim {
        crate::app::settings::store::LoadingAnim::CircleBottomRight => 1,
        _ => 0,
    });
    settings_window.set_update_index(match current_settings.update_check_frequency {
        crate::app::settings::store::UpdateCheckFrequency::OnStartup => 1,
        crate::app::settings::store::UpdateCheckFrequency::EveryNDays(_) => 2,
        _ => 0,
    });
    settings_window.set_bookmarks_visible(current_settings.bookmarks_visible_on_cover.into());
    settings_window.set_bookmark_red(current_settings.default_bookmark_color[0].into());
    settings_window.set_bookmark_green(current_settings.default_bookmark_color[1].into());
    settings_window.set_bookmark_blue(current_settings.default_bookmark_color[2].into());
    let settings_filters = Rc::new(RefCell::new(SettingsFilterState::from_settings(
        &current_settings,
    )));
    update_settings_filter_models(&settings_window, &settings_filters.borrow());

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
            main.global::<Theme>().set_scale(ui_scale as f32 / 100.0);
            main.global::<Theme>()
                .set_card_scale(card_scale as f32 / 100.0);
            main.global::<Theme>()
                .set_card_width(320.0 * ui_scale as f32 / 100.0 * card_scale as f32 / 100.0);
        }
    });

    let settings_for_save = settings_window.clone();
    let filters_for_save = settings_filters.clone();
    let state_for_settings_save = state.clone();
    settings_window.on_save_settings(
        move |ui_scale,
              card_scale,
              temp_directory,
              games_directory,
              cache_directory,
              custom_launch,
              cache_on_download,
              log_to_file,
              show_unplayed,
              classic_library,
              language,
              loading,
              update,
              bookmarks_visible,
              bookmark_red,
              bookmark_green,
              bookmark_blue| {
            let ui_scale = ui_scale.clamp(50.0, 200.0) as u16;
            let card_scale = card_scale.clamp(50.0, 200.0) as u16;
            if let Ok(mut settings) = crate::app::settings::APP_SETTINGS.write() {
                settings.ui_scale_percent = ui_scale;
                settings.card_scale_percent = card_scale;
                settings.temp_dir = PathBuf::from(temp_directory.as_str());
                settings.extract_dir = PathBuf::from(games_directory.as_str());
                settings.cache_dir = PathBuf::from(cache_directory.as_str());
                settings.custom_launch = custom_launch.to_string();
                settings.cache_on_download = cache_on_download;
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
                settings.language = match language {
                    1 => Some(crate::localization::SupportedLang::English),
                    2 => Some(crate::localization::SupportedLang::Russian),
                    _ => None,
                };
                settings.loading_anim = if loading == 1 {
                    crate::app::settings::store::LoadingAnim::CircleBottomRight
                } else {
                    crate::app::settings::store::LoadingAnim::BottomBar
                };
                settings.update_check_frequency = match update {
                    1 => crate::app::settings::store::UpdateCheckFrequency::OnStartup,
                    2 => crate::app::settings::store::UpdateCheckFrequency::EveryNDays(7),
                    _ => crate::app::settings::store::UpdateCheckFrequency::Manual,
                };
            }
            crate::logger::set_file_logging_enabled(log_to_file);
            crate::app::settings::save_settings_to_disk();
            settings_for_save.set_saved_message("Сохранено".into());
            if let Some(main) = main_weak.upgrade() {
                main.set_ui_scale_percent(ui_scale.into());
                main.set_card_scale_percent(card_scale.into());
                main.global::<Theme>().set_scale(ui_scale as f32 / 100.0);
                main.global::<Theme>()
                    .set_card_scale(card_scale as f32 / 100.0);
                main.global::<Theme>()
                    .set_card_width(320.0 * ui_scale as f32 / 100.0 * card_scale as f32 / 100.0);
                rebuild_cards(&main, &state_for_settings_save);
            }
        },
    );

    let settings_for_open = settings_window.clone();
    ui.on_open_settings(move || {
        settings_for_open.set_saved_message("".into());
        let _ = settings_for_open.show();
    });
    let about_for_open = about_window.clone();
    ui.on_open_about(move || {
        let _ = about_for_open.show();
    });
    let logs_for_refresh = logs_window.clone();
    logs_window.on_refresh(move || refresh_logs(&logs_for_refresh));
    let logs_for_clear = logs_window.clone();
    logs_window.on_clear(move || {
        crate::logger::clear();
        refresh_logs(&logs_for_clear);
    });
    let logs_for_open = logs_window.clone();
    ui.on_open_logs(move || {
        refresh_logs(&logs_for_open);
        let _ = logs_for_open.show();
    });

    let weak = ui.as_weak();
    ui.on_suggestion_query(move |query, kind| {
        let suggestions = matching_suggestions(query.as_str(), kind);
        if let Some(ui) = weak.upgrade() {
            match kind {
                0 => ui.set_include_tag_suggestions(suggestions),
                1 => ui.set_exclude_tag_suggestions(suggestions),
                _ => ui.set_prefix_suggestions(suggestions),
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
                    _ => &mut state.prefixes,
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
        load_catalog(page, query, callback_state.clone(), weak.clone());
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
                    _ => &mut state.prefixes,
                };
                target.retain(|value| *value != id);
                state.query.clone()
            })
            .unwrap_or_default();
        if let Some(ui) = weak.upgrade() {
            update_selected_filters(&ui, &callback_state);
        }
        load_catalog(1, query, callback_state.clone(), weak.clone());
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
        load_catalog(1, query, callback_state.clone(), weak.clone());
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
        load_catalog(1, query, callback_state.clone(), weak.clone());
    });
}

fn sorting_from_index(index: i32) -> Sorting {
    match index {
        1 => Sorting::Likes,
        2 => Sorting::Views,
        3 => Sorting::Title,
        4 => Sorting::Rating,
        _ => Sorting::Date,
    }
}

fn date_limit_from_index(index: i32) -> DateLimit {
    match index {
        1 => DateLimit::Today,
        2 => DateLimit::Days3,
        3 => DateLimit::Days7,
        4 => DateLimit::Days14,
        5 => DateLimit::Days30,
        6 => DateLimit::Days90,
        7 => DateLimit::Days180,
        8 => DateLimit::Days365,
        _ => DateLimit::Anytime,
    }
}

fn date_limit_label(index: i32) -> &'static str {
    match index {
        1 => "СЕГОДНЯ",
        2 => "3 ДНЯ",
        3 => "7 ДНЕЙ",
        4 => "14 ДНЕЙ",
        5 => "30 ДНЕЙ",
        6 => "90 ДНЕЙ",
        7 => "180 ДНЕЙ",
        8 => "365 ДНЕЙ",
        _ => "ЛЮБОЕ ВРЕМЯ",
    }
}

fn update_selected_filters(ui: &MainWindow, state: &SharedState) {
    let Ok(state) = state.lock() else { return };
    let to_model = |ids: &[u32], prefix: bool| {
        let values = ids
            .iter()
            .map(|id| SuggestionData {
                id: id.to_string().into(),
                label: if prefix {
                    crate::tags::get_prefix_name_by_id(*id)
                } else {
                    crate::tags::get_tag_name_by_id(*id)
                }
                .into(),
            })
            .collect::<Vec<_>>();
        ModelRc::from(Rc::new(VecModel::from(values)))
    };
    ui.set_selected_include_tags(to_model(&state.include_tags, false));
    ui.set_selected_exclude_tags(to_model(&state.exclude_tags, false));
    ui.set_selected_prefixes(to_model(&state.prefixes, true));
}

fn empty_suggestions() -> ModelRc<SuggestionData> {
    ModelRc::from(Rc::new(VecModel::<SuggestionData>::default()))
}

fn settings_filter_model(ids: &[u32], prefix: bool) -> ModelRc<SuggestionData> {
    let values = ids
        .iter()
        .map(|id| SuggestionData {
            id: id.to_string().into(),
            label: if prefix {
                crate::tags::get_prefix_name_by_id(*id)
            } else {
                crate::tags::get_tag_name_by_id(*id)
            }
            .into(),
        })
        .collect::<Vec<_>>();
    ModelRc::from(Rc::new(VecModel::from(values)))
}

fn update_settings_filter_models(window: &SettingsWindow, filters: &SettingsFilterState) {
    window.set_startup_tags(settings_filter_model(&filters.values[0], false));
    window.set_startup_exclude_tags(settings_filter_model(&filters.values[1], false));
    window.set_startup_prefixes(settings_filter_model(&filters.values[2], true));
    window.set_startup_exclude_prefixes(settings_filter_model(&filters.values[3], true));
    window.set_warning_tags(settings_filter_model(&filters.values[4], false));
    window.set_warning_prefixes(settings_filter_model(&filters.values[5], true));
}

fn refresh_logs(window: &LogsWindow) {
    window.set_contents(crate::logger::get_all().join("\n").into());
}

fn matching_suggestions(query: &str, kind: i32) -> ModelRc<SuggestionData> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return ModelRc::from(Rc::new(VecModel::<SuggestionData>::default()));
    }
    let mut items = if kind == 2 {
        crate::tags::TAGS
            .prefixes
            .games
            .iter()
            .flat_map(|group| group.prefixes.iter())
            .map(|prefix| (prefix.id.to_string(), prefix.name.clone()))
            .collect::<Vec<_>>()
    } else {
        crate::tags::TAGS
            .tags
            .iter()
            .map(|(id, name)| (id.clone(), name.clone()))
            .collect::<Vec<_>>()
    };
    items.retain(|(_, label)| label.to_lowercase().contains(&needle));
    items.sort_by(|left, right| left.1.cmp(&right.1));
    let items = items
        .into_iter()
        .take(6)
        .map(|(id, label)| SuggestionData {
            id: id.into(),
            label: label.into(),
        })
        .collect::<Vec<_>>();
    ModelRc::from(Rc::new(VecModel::from(items)))
}

fn load_catalog(page: u32, query: String, state: SharedState, weak: slint::Weak<MainWindow>) {
    let _ = weak.upgrade_in_event_loop(|ui| {
        ui.set_library_mode(false);
        ui.set_loading(true);
        ui.set_status_text("Загрузка каталога…".into());
    });
    crate::app::rt().spawn(async move {
        let (include_tags, exclude_tags, prefixes, exclude_prefixes, sorting, date_limit) = state
            .lock()
            .map(|state| {
                (
                    state.include_tags.clone(),
                    state.exclude_tags.clone(),
                    state.prefixes.clone(),
                    state.exclude_prefixes.clone(),
                    state.sorting.clone(),
                    state.date_limit,
                )
            })
            .unwrap_or_default();
        let filters = F95Filters::default()
            .with_category("games")
            .with_search_query(query.clone())
            .with_include_tags(include_tags)
            .with_exclude_tags(exclude_tags)
            .with_prefixes(prefixes)
            .with_noprefixes(exclude_prefixes)
            .with_sort(sorting)
            .with_date_limit(date_limit);
        let result = crate::parser::fetch_list_page(page, &filters).await;
        match result {
            Ok(message) => {
                let installed: HashSet<u64> = crate::app::settings::APP_SETTINGS
                    .read()
                    .map(|settings| {
                        settings
                            .downloaded_games
                            .iter()
                            .map(|game| game.thread_id)
                            .collect()
                    })
                    .unwrap_or_default();
                let hidden: HashSet<u64> = crate::app::settings::APP_SETTINGS
                    .read()
                    .map(|settings| settings.hidden_threads.iter().copied().collect())
                    .unwrap_or_default();
                let cards = message
                    .data
                    .into_iter()
                    .filter(|thread| !hidden.contains(&thread.thread_id.get()))
                    .map(|thread| {
                        let is_installed = installed.contains(&thread.thread_id.get());
                        card_from_thread(thread, is_installed, None, None)
                    })
                    .collect();
                if let Ok(mut state) = state.lock() {
                    state.cards = cards;
                    state.page = message.pagination.page;
                    state.total_pages = message.pagination.total.max(1);
                    state.query = query;
                    state.library_mode = false;
                }
                let state_for_ui = state.clone();
                request_all_covers(state.clone(), weak.clone());
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    ui.set_loading(false);
                    ui.set_status_text("Каталог пуст".into());
                    prune_cover_cache(&state_for_ui);
                    rebuild_cards(&ui, &state_for_ui);
                });
            }
            Err(error) => {
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    ui.set_loading(false);
                    ui.set_status_text(format!("Не удалось загрузить каталог: {error}").into());
                    ui.set_game_rows(empty_rows());
                });
            }
        }
    });
}

fn load_library(state: SharedState, weak: slint::Weak<MainWindow>) {
    let _ = weak.upgrade_in_event_loop(|ui| {
        ui.set_library_mode(true);
        ui.set_loading(true);
        ui.set_status_text("Загрузка библиотеки…".into());
    });
    let selected = state
        .lock()
        .map(|state| state.selected_bookmarks.clone())
        .unwrap_or_default();
    crate::app::rt().spawn(async move {
        let selected_for_fast = selected.clone();
        let cards = tokio::task::spawn_blocking(move || library_cards_fast(&selected_for_fast))
            .await
            .unwrap_or_default();
        if let Ok(mut state) = state.lock() {
            if !state.library_mode || state.selected_bookmarks != selected {
                return;
            }
            state.cards = cards;
            state.library_mode = true;
        }
        let state_for_ui = state.clone();
        request_all_covers(state.clone(), weak.clone());
        let _ = weak.upgrade_in_event_loop(move |ui| {
            ui.set_loading(false);
            ui.set_status_text("Библиотека пуста".into());
            prune_cover_cache(&state_for_ui);
            rebuild_cards(&ui, &state_for_ui);
            update_bookmarks(&ui, &state_for_ui);
        });

        let selected_for_enrich = selected.clone();
        let enriched = tokio::task::spawn_blocking(move || library_cards(&selected_for_enrich))
            .await
            .unwrap_or_default();
        if let Ok(mut state) = state.lock() {
            if !state.library_mode || state.selected_bookmarks != selected {
                return;
            }
            state.cards = enriched;
        }
        request_all_covers(state.clone(), weak.clone());
        let state_for_ui = state.clone();
        let _ = weak.upgrade_in_event_loop(move |ui| {
            rebuild_cards(&ui, &state_for_ui);
        });
    });
}

fn library_cards_fast(selected: &HashSet<String>) -> Vec<CardRecord> {
    let settings = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| settings.clone())
        .unwrap_or_default();
    let mut seen = HashSet::new();
    settings
        .downloaded_games
        .into_iter()
        .filter(|game| {
            selected.is_empty() || game.bookmark_ids.iter().any(|id| selected.contains(id))
        })
        .filter(|game| seen.insert(game.thread_id))
        .map(|game| {
            let cached_cover = settings
                .cache_dir
                .join(game.thread_id.to_string())
                .join("cover.png");
            CardRecord {
                id: game.thread_id,
                title: game
                    .folder
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Игра")
                    .to_string(),
                creator: "Локальная библиотека".to_string(),
                version: String::new(),
                prefix: String::new(),
                date: String::new(),
                likes: "0".to_string(),
                views: "0".to_string(),
                rating: "0.0".to_string(),
                cover_url: None,
                screens: Vec::new(),
                tags: Vec::new(),
                prefixes: Vec::new(),
                cached_cover: cached_cover.exists().then_some(cached_cover),
                installed: true,
                folder: Some(game.folder),
            }
        })
        .collect()
}

fn library_cards(selected: &HashSet<String>) -> Vec<CardRecord> {
    let settings = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| settings.clone())
        .unwrap_or_default();
    let mut seen = HashSet::new();
    settings
        .downloaded_games
        .into_iter()
        .filter(|game| {
            selected.is_empty() || game.bookmark_ids.iter().any(|id| selected.contains(id))
        })
        .filter(|game| seen.insert(game.thread_id))
        .filter(|game| crate::app::settings::game_folder_exists(&game.folder))
        .map(|game| {
            let cached_cover = settings
                .cache_dir
                .join(game.thread_id.to_string())
                .join("cover.png");
            if let Some(thread) =
                crate::app::fetch_helpers::load_from_cache(&settings.cache_dir, game.thread_id)
            {
                card_from_thread(
                    thread,
                    true,
                    Some(game.folder),
                    cached_cover.exists().then_some(cached_cover),
                )
            } else {
                CardRecord {
                    id: game.thread_id,
                    title: game
                        .folder
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Игра")
                        .to_string(),
                    creator: "Локальная библиотека".to_string(),
                    version: String::new(),
                    prefix: String::new(),
                    date: String::new(),
                    likes: "0".to_string(),
                    views: "0".to_string(),
                    rating: "0.0".to_string(),
                    cover_url: None,
                    screens: Vec::new(),
                    tags: Vec::new(),
                    prefixes: Vec::new(),
                    cached_cover: cached_cover.exists().then_some(cached_cover),
                    installed: true,
                    folder: Some(game.folder),
                }
            }
        })
        .collect()
}

fn card_from_thread(
    thread: F95Thread,
    installed: bool,
    folder: Option<PathBuf>,
    cached_cover: Option<PathBuf>,
) -> CardRecord {
    let id = thread.thread_id.get();
    let prefixes = thread.prefixes.clone();
    let screens = thread
        .screens
        .iter()
        .map(|url| crate::parser::normalize_url(url))
        .collect::<Vec<_>>();
    let cover_url = if thread.cover.trim().is_empty() {
        screens.first().cloned()
    } else {
        Some(crate::parser::normalize_url(&thread.cover))
    };
    CardRecord {
        id,
        title: thread.title,
        creator: thread.creator,
        version: thread.version,
        prefix: thread
            .prefixes
            .first()
            .copied()
            .map(crate::tags::get_prefix_name_by_id)
            .unwrap_or_default(),
        date: thread.date,
        likes: thread.likes.to_string(),
        views: thread.views.to_string(),
        rating: format!("{:.1}", thread.rating),
        cover_url,
        screens,
        tags: thread.tags,
        prefixes,
        cached_cover,
        installed,
        folder,
    }
}

fn request_all_covers(state: SharedState, weak: slint::Weak<MainWindow>) {
    let ids = state
        .lock()
        .map(|state| state.cards.iter().map(|card| card.id).collect::<Vec<_>>())
        .unwrap_or_default();
    for id in ids {
        request_cover(id, state.clone(), weak.clone());
    }
}

fn preload_library_covers(state: SharedState, weak: slint::Weak<MainWindow>) {
    let covers = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| {
            let mut seen = HashSet::new();
            settings
                .downloaded_games
                .iter()
                .filter(|game| seen.insert(game.thread_id))
                .filter_map(|game| {
                    let path = settings
                        .cache_dir
                        .join(game.thread_id.to_string())
                        .join("cover.png");
                    path.is_file().then_some((game.thread_id, path))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let target_size = cover_target_size();

    for (id, path) in covers {
        let should_load = state
            .lock()
            .map(|mut state| !state.loaded_images.contains(&id) && state.loading_images.insert(id))
            .unwrap_or(false);
        if !should_load {
            continue;
        }

        let state_for_task = state.clone();
        let weak_for_task = weak.clone();
        crate::app::rt().spawn(async move {
            let Ok(_permit) = library_preload_semaphore().acquire().await else {
                return;
            };
            let decoded = tokio::task::spawn_blocking(move || decode_image_path(path, target_size))
                .await
                .ok()
                .flatten();
            let state_for_ui = state_for_task.clone();
            let _ = weak_for_task.upgrade_in_event_loop(move |ui| {
                finish_cover_decode(&ui, &state_for_ui, id, decoded)
            });
        });
    }
}

fn request_cover(id: u64, state: SharedState, weak: slint::Weak<MainWindow>) {
    let source = {
        let Ok(mut state) = state.lock() else { return };
        if state.loaded_images.contains(&id) || !state.loading_images.insert(id) {
            return;
        }
        let source = state
            .cards
            .iter()
            .find(|card| card.id == id)
            .map(|card| (card.cached_cover.clone(), card.cover_url.clone()));
        if source.is_none() {
            state.loading_images.remove(&id);
        }
        source
    };
    let Some((cached, remote)) = source else {
        return;
    };
    let target_size = cover_target_size();
    crate::app::rt().spawn(async move {
        let Ok(_permit) = cover_semaphore().acquire().await else {
            return;
        };
        let decoded = if let Some(path) = cached {
            tokio::task::spawn_blocking(move || decode_image_path(path, target_size))
                .await
                .ok()
                .flatten()
        } else if let Some(url) = remote {
            crate::parser::fetch_image_f95(&url)
                .await
                .ok()
                .and_then(|(width, height, rgba)| {
                    resize_cover_pixels(width as u32, height as u32, rgba, target_size)
                })
        } else {
            None
        };
        let state_for_ui = state.clone();
        let _ = weak
            .upgrade_in_event_loop(move |ui| finish_cover_decode(&ui, &state_for_ui, id, decoded));
    });
}

fn cover_semaphore() -> &'static Arc<tokio::sync::Semaphore> {
    static SEMAPHORE: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    SEMAPHORE.get_or_init(|| Arc::new(tokio::sync::Semaphore::new(2)))
}

fn library_preload_semaphore() -> &'static Arc<tokio::sync::Semaphore> {
    static SEMAPHORE: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    SEMAPHORE.get_or_init(|| Arc::new(tokio::sync::Semaphore::new(2)))
}

fn request_screens(id: u64, state: SharedState, weak: slint::Weak<MainWindow>) {
    let urls = {
        let Ok(mut state) = state.lock() else { return };
        if state.loaded_screens.contains(&id) || !state.loading_screens.insert(id) {
            return;
        }
        state
            .cards
            .iter()
            .find(|card| card.id == id)
            .map(|card| card.screens.clone())
            .unwrap_or_default()
    };
    if urls.is_empty() {
        if let Ok(mut state) = state.lock() {
            state.loading_screens.remove(&id);
            state.loaded_screens.insert(id);
        }
        return;
    }
    let target_size = cover_target_size();
    let cache_dir = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| settings.cache_dir.clone())
        .unwrap_or_else(|_| PathBuf::from("cache"));
    crate::app::rt().spawn(async move {
        for (index, url) in urls.into_iter().enumerate() {
            let cached = cache_dir
                .join(id.to_string())
                .join(format!("screen_{}.png", index + 1));
            let decoded = if cached.is_file() {
                tokio::task::spawn_blocking(move || decode_image_path(cached, target_size))
                    .await
                    .ok()
                    .flatten()
            } else {
                crate::parser::fetch_image_f95(&url)
                    .await
                    .ok()
                    .and_then(|(width, height, rgba)| {
                        resize_cover_pixels(width as u32, height as u32, rgba, target_size)
                    })
            };
            if let Some(decoded) = decoded {
                let state_for_ui = state.clone();
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    let evicted = cache_screen_image(id, index, &decoded);
                    if let Ok(mut state) = state_for_ui.lock() {
                        state.loaded_screen_images.insert((id, index));
                        for evicted_id in evicted {
                            state
                                .loaded_screen_images
                                .retain(|(screen_id, _)| *screen_id != evicted_id);
                            state.loaded_screens.remove(&evicted_id);
                        }
                    }
                    update_card_image(&ui, &state_for_ui, id);
                });
            }
        }
        if let Ok(mut state) = state.lock() {
            state.loading_screens.remove(&id);
            state.loaded_screens.insert(id);
        }
        let state_for_ui = state.clone();
        let _ = weak.upgrade_in_event_loop(move |ui| update_card_image(&ui, &state_for_ui, id));
    });
}

fn cover_target_size() -> (u32, u32) {
    let (ui_scale, card_scale) = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| {
            (
                settings.ui_scale_percent as f32 / 100.0,
                settings.card_scale_percent as f32 / 100.0,
            )
        })
        .unwrap_or((1.0, 1.0));

    // Keep a little resolution reserve for fractional DPI, but never retain the
    // original multi-megapixel cover: the software renderer otherwise rescales
    // it for every frame while the list is moving.
    let card_factor = ui_scale * card_scale;
    let width = ((320.0 * card_factor - 18.0 * card_factor) * 1.25)
        .round()
        .clamp(160.0, 720.0) as u32;
    let height = ((width as f32 * 9.0 / 16.0).round() as u32).clamp(90, 405);
    (width, height)
}

fn decode_image_path(path: PathBuf, target_size: (u32, u32)) -> Option<ImagePixels> {
    let image = image::open(path)
        .ok()?
        .resize_to_fill(
            target_size.0,
            target_size.1,
            image::imageops::FilterType::Triangle,
        )
        .to_rgba8();
    Some(ImagePixels {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

fn resize_cover_pixels(
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    target_size: (u32, u32),
) -> Option<ImagePixels> {
    let source = image::RgbaImage::from_raw(width, height, rgba)?;
    let resized = image::DynamicImage::ImageRgba8(source)
        .resize_to_fill(
            target_size.0,
            target_size.1,
            image::imageops::FilterType::Triangle,
        )
        .to_rgba8();
    Some(ImagePixels {
        width: resized.width(),
        height: resized.height(),
        rgba: resized.into_raw(),
    })
}

fn rebuild_cards(ui: &MainWindow, state: &SharedState) {
    let app_settings = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| settings.clone())
        .unwrap_or_default();
    let Ok(state) = state.lock() else { return };
    let columns = state.columns.max(1);
    let rows = state
        .cards
        .chunks(columns)
        .map(|cards| {
            let cards = cards
                .iter()
                .map(|card| {
                    let (warning_count, bookmark_badges, unplayed) =
                        card_badges(card, &app_settings);
                    GameCardData {
                        id: card.id.to_string().into(),
                        title: card.title.clone().into(),
                        creator: card.creator.clone().into(),
                        version: card.version.clone().into(),
                        prefix: card.prefix.clone().into(),
                        date: card.date.clone().into(),
                        likes: card.likes.clone().into(),
                        views: card.views.clone().into(),
                        rating: card.rating.clone().into(),
                        cover: cached_cover_image(card.id),
                        screens: screen_model(&state, card),
                        screen_loaded: screen_loaded_model(&state, card),
                        screen_count: card.screens.len() as i32,
                        screens_ready: available_screen_count(&state, card) > 0,
                        tag_rows: tag_rows(&card.tags),
                        installed: card.installed,
                        warning_count,
                        bookmark_badges,
                        unplayed,
                    }
                })
                .collect::<Vec<_>>();
            GameRowData {
                cards: ModelRc::from(Rc::new(VecModel::from(cards))),
            }
        })
        .collect::<Vec<_>>();
    ui.set_game_rows(ModelRc::from(Rc::new(VecModel::from(rows))));
    ui.set_page_label(
        if state.library_mode {
            format!("Установлено игр: {}", state.cards.len())
        } else {
            format!("Страница {} из {}", state.page, state.total_pages)
        }
        .into(),
    );
}

fn card_badges(
    card: &CardRecord,
    settings: &crate::app::settings::AppSettings,
) -> (i32, ModelRc<CardBadgeData>, bool) {
    let warning_count = card
        .tags
        .iter()
        .filter(|id| settings.warn_tags.contains(id))
        .count()
        + card
            .prefixes
            .iter()
            .filter(|id| settings.warn_prefixes.contains(id))
            .count();
    let downloaded = settings
        .downloaded_games
        .iter()
        .find(|game| game.thread_id == card.id);
    let unplayed = settings.show_unplayed_badge
        && downloaded
            .map(|game| !game.has_been_launched)
            .unwrap_or(false);
    let bookmark_ids = downloaded
        .map(|game| game.bookmark_ids.as_slice())
        .unwrap_or_default();
    let limit = settings.bookmarks_visible_on_cover as usize;
    let matching = settings
        .bookmarks
        .iter()
        .filter(|bookmark| bookmark_ids.contains(&bookmark.id))
        .collect::<Vec<_>>();
    let mut badges = matching
        .iter()
        .take(limit)
        .map(|bookmark| {
            let [red, green, blue] = bookmark.color.unwrap_or(settings.default_bookmark_color);
            CardBadgeData {
                label: bookmark.emoji.clone().into(),
                red: red.into(),
                green: green.into(),
                blue: blue.into(),
            }
        })
        .collect::<Vec<_>>();
    if matching.len() > limit {
        badges.push(CardBadgeData {
            label: "…".into(),
            red: 60,
            green: 60,
            blue: 60,
        });
    }
    (
        warning_count as i32,
        ModelRc::from(Rc::new(VecModel::from(badges))),
        unplayed,
    )
}

fn update_card_image(ui: &MainWindow, state: &SharedState, id: u64) {
    let Ok(state) = state.lock() else { return };
    let columns = state.columns.max(1);
    let Some(index) = state.cards.iter().position(|card| card.id == id) else {
        return;
    };
    let rows = ui.get_game_rows();
    let Some(row) = rows.row_data(index / columns) else {
        return;
    };
    let Some(mut card) = row.cards.row_data(index % columns) else {
        return;
    };
    card.cover = cached_cover_image(id);
    if let Some(record) = state.cards.get(index) {
        card.screens = screen_model(&state, record);
        card.screen_loaded = screen_loaded_model(&state, record);
        card.screen_count = record.screens.len() as i32;
        card.screens_ready = available_screen_count(&state, record) > 0;
    }
    row.cards.set_row_data(index % columns, card);
}

fn screen_model(_state: &UiState, card: &CardRecord) -> ModelRc<Image> {
    let images = (0..card.screens.len())
        .map(|index| cached_screen_image(card.id, index))
        .collect::<Vec<_>>();
    ModelRc::from(Rc::new(VecModel::from(images)))
}

fn screen_loaded_model(state: &UiState, card: &CardRecord) -> ModelRc<bool> {
    let loaded = (0..card.screens.len())
        .map(|index| state.loaded_screen_images.contains(&(card.id, index)))
        .collect::<Vec<_>>();
    ModelRc::from(Rc::new(VecModel::from(loaded)))
}

fn available_screen_count(state: &UiState, card: &CardRecord) -> usize {
    (0..card.screens.len())
        .filter(|index| state.loaded_screen_images.contains(&(card.id, *index)))
        .count()
}

fn tag_rows(tags: &[u32]) -> ModelRc<TagRowData> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut used = 0usize;
    for tag in tags {
        let name = crate::tags::get_tag_name_by_id(*tag);
        if name.is_empty() {
            continue;
        }
        let width = name.chars().count() + 3;
        if !row.is_empty() && used + width > 38 {
            rows.push(TagRowData {
                tags: ModelRc::from(Rc::new(VecModel::from(row))),
            });
            row = Vec::new();
            used = 0;
        }
        used += width;
        row.push(name.into());
    }
    if !row.is_empty() {
        rows.push(TagRowData {
            tags: ModelRc::from(Rc::new(VecModel::from(row))),
        });
    }
    ModelRc::from(Rc::new(VecModel::from(rows)))
}

fn image_from_pixels(pixels: &ImagePixels) -> Image {
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        &pixels.rgba,
        pixels.width,
        pixels.height,
    );
    Image::from_rgba8(buffer)
}

fn prune_cover_cache(state: &SharedState) {
    let mut keep = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| {
            settings
                .downloaded_games
                .iter()
                .map(|game| game.thread_id)
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();
    if let Ok(state) = state.lock() {
        keep.extend(state.cards.iter().map(|card| card.id));
    }
    UI_IMAGE_CACHE.with(|cache| {
        cache.borrow_mut().covers.retain(|id, _| keep.contains(id));
    });
    if let Ok(mut state) = state.lock() {
        state.loaded_images.retain(|id| keep.contains(id));
    }
}

fn finish_cover_decode(
    ui: &MainWindow,
    state: &SharedState,
    id: u64,
    decoded: Option<ImagePixels>,
) {
    if let Some(pixels) = decoded.filter(|_| should_keep_cover(state, id)) {
        let image = image_from_pixels(&pixels);
        UI_IMAGE_CACHE.with(|cache| {
            cache.borrow_mut().covers.insert(id, image);
        });
        if let Ok(mut state) = state.lock() {
            state.loaded_images.insert(id);
            state.loading_images.remove(&id);
        }
    } else if let Ok(mut state) = state.lock() {
        state.loading_images.remove(&id);
    }
    update_card_image(ui, state, id);
}

fn should_keep_cover(state: &SharedState, id: u64) -> bool {
    let installed = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| {
            settings
                .downloaded_games
                .iter()
                .any(|game| game.thread_id == id)
        })
        .unwrap_or(false);
    installed
        || state
            .lock()
            .map(|state| state.cards.iter().any(|card| card.id == id))
            .unwrap_or(false)
}

fn cached_cover_image(id: u64) -> Image {
    UI_IMAGE_CACHE.with(|cache| cache.borrow().covers.get(&id).cloned().unwrap_or_default())
}

fn cached_screen_image(id: u64, index: usize) -> Image {
    UI_IMAGE_CACHE.with(|cache| {
        cache
            .borrow()
            .screens
            .get(&(id, index))
            .cloned()
            .unwrap_or_default()
    })
}

fn cache_screen_image(id: u64, index: usize, pixels: &ImagePixels) -> Vec<u64> {
    const SCREEN_GAME_CACHE_LIMIT: usize = 12;
    UI_IMAGE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.screens.insert((id, index), image_from_pixels(pixels));
        cache.screen_games.retain(|game_id| *game_id != id);
        cache.screen_games.push_back(id);
        let mut evicted = Vec::new();
        while cache.screen_games.len() > SCREEN_GAME_CACHE_LIMIT {
            if let Some(game_id) = cache.screen_games.pop_front() {
                cache
                    .screens
                    .retain(|(screen_id, _), _| *screen_id != game_id);
                evicted.push(game_id);
            }
        }
        evicted
    })
}

fn update_bookmarks(ui: &MainWindow, state: &SharedState) {
    let selected = state
        .lock()
        .map(|state| state.selected_bookmarks.clone())
        .unwrap_or_default();
    let bookmarks = crate::app::settings::get_bookmarks()
        .into_iter()
        .map(|bookmark| BookmarkData {
            active: selected.contains(&bookmark.id),
            id: bookmark.id.into(),
            label: format!("{} {}", bookmark.emoji, bookmark.label).into(),
        })
        .collect::<Vec<_>>();
    ui.set_bookmarks(ModelRc::from(Rc::new(VecModel::from(bookmarks))));
}

fn game_bookmark_model(thread_id: u64) -> ModelRc<BookmarkData> {
    let bookmarks = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| {
            let active = settings
                .downloaded_games
                .iter()
                .find(|game| game.thread_id == thread_id)
                .map(|game| game.bookmark_ids.clone())
                .unwrap_or_default();
            settings
                .bookmarks
                .iter()
                .map(|bookmark| BookmarkData {
                    id: bookmark.id.clone().into(),
                    label: format!("{} {}", bookmark.emoji, bookmark.label).into(),
                    active: active.contains(&bookmark.id),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    ModelRc::from(Rc::new(VecModel::from(bookmarks)))
}

fn empty_rows() -> ModelRc<GameRowData> {
    ModelRc::from(Rc::new(VecModel::<GameRowData>::default()))
}
