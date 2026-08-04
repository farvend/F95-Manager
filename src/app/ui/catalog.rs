use super::*;

pub(super) fn load_catalog(
    page: u32,
    query: String,
    state: SharedState,
    weak: slint::Weak<MainWindow>,
) {
    let generation = {
        let Ok(mut state) = state.lock() else { return };
        state.request_generation = state.request_generation.wrapping_add(1);
        state.library_mode = false;
        state.request_generation
    };
    let _ = weak.upgrade_in_event_loop(|ui| {
        ui.set_library_mode(false);
        ui.set_loading(true);
        ui.set_status_text(tr("catalog-loading"));
    });
    crate::app::rt().spawn(async move {
        let (
            include_tags,
            include_logic,
            exclude_tags,
            prefixes,
            exclude_prefixes,
            sorting,
            date_limit,
            search_mode,
        ) = state
            .lock()
            .map(|state| {
                (
                    state.include_tags.clone(),
                    state.include_logic,
                    state.exclude_tags.clone(),
                    state.prefixes.clone(),
                    state.exclude_prefixes.clone(),
                    state.sorting.clone(),
                    state.date_limit,
                    state.search_mode,
                )
            })
            .unwrap_or_default();
        let filters = F95Filters::default()
            .with_category("games")
            .with_search_query(query.clone())
            .with_include_tags(include_tags.clone())
            .with_exclude_tags(exclude_tags)
            .with_prefixes(prefixes)
            .with_noprefixes(exclude_prefixes)
            .with_sort(sorting)
            .with_date_limit(date_limit);
        let result = crate::parser::fetch_list_page(page, &filters).await;
        match result {
            Ok(mut message) => {
                let normalized_query = query.trim().to_lowercase();
                message.data.retain(|thread| {
                    let query_matches = normalized_query.is_empty()
                        || match search_mode {
                            SearchMode::Creator => {
                                thread.creator.to_lowercase().contains(&normalized_query)
                            }
                            SearchMode::Title => {
                                thread.title.to_lowercase().contains(&normalized_query)
                            }
                        };
                    let tags_match = include_tags.is_empty()
                        || match include_logic {
                            TagLogic::And => include_tags.iter().all(|id| thread.tags.contains(id)),
                            TagLogic::Or => include_tags.iter().any(|id| thread.tags.contains(id)),
                        };
                    query_matches && tags_match
                });
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
                    if state.request_generation != generation || state.library_mode {
                        return;
                    }
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
                    ui.set_status_text(tr("catalog-empty"));
                    prune_cover_cache(&state_for_ui);
                    rebuild_cards(&ui, &state_for_ui);
                });
            }
            Err(error) => {
                let current = state
                    .lock()
                    .map(|state| state.request_generation == generation && !state.library_mode)
                    .unwrap_or(false);
                if !current {
                    return;
                }
                append_error(format!("Catalog load error: {error}"), &weak);
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    ui.set_loading(false);
                    ui.set_status_text(tr_with(
                        "catalog-load-error",
                        &[("err", error.to_string())],
                    ));
                    ui.set_game_rows(empty_rows());
                });
            }
        }
    });
}

pub(super) fn load_library(state: SharedState, weak: slint::Weak<MainWindow>) {
    let generation = {
        let Ok(mut state) = state.lock() else { return };
        state.request_generation = state.request_generation.wrapping_add(1);
        state.library_mode = true;
        state.request_generation
    };
    let _ = weak.upgrade_in_event_loop(|ui| {
        ui.set_library_mode(true);
        ui.set_loading(true);
        ui.set_status_text(tr("library-loading"));
    });
    let selected = state
        .lock()
        .map(|state| state.selected_bookmarks.clone())
        .unwrap_or_default();
    let filter = state
        .lock()
        .map(|state| LibraryFilter::from(&*state))
        .unwrap_or_default();
    crate::app::rt().spawn(async move {
        let selected_for_fast = selected.clone();
        let fast_filter = filter.clone();
        let cards = tokio::task::spawn_blocking(move || {
            let mut cards = library_cards_fast(&selected_for_fast);
            filter_library_cards(&mut cards, &fast_filter);
            cards
        })
        .await
        .unwrap_or_default();
        if let Ok(mut state) = state.lock() {
            if !state.library_mode
                || state.request_generation != generation
                || state.selected_bookmarks != selected
            {
                return;
            }
            state.cards = cards;
            state.library_mode = true;
        }
        let state_for_ui = state.clone();
        request_all_covers(state.clone(), weak.clone());
        let _ = weak.upgrade_in_event_loop(move |ui| {
            ui.set_loading(false);
            ui.set_status_text(tr("library-empty"));
            prune_cover_cache(&state_for_ui);
            rebuild_cards(&ui, &state_for_ui);
            update_bookmarks(&ui, &state_for_ui);
        });

        let selected_for_enrich = selected.clone();
        let enriched = tokio::task::spawn_blocking(move || {
            let mut cards = library_cards(&selected_for_enrich);
            filter_library_cards(&mut cards, &filter);
            cards
        })
        .await
        .unwrap_or_default();
        if let Ok(mut state) = state.lock() {
            if !state.library_mode
                || state.request_generation != generation
                || state.selected_bookmarks != selected
            {
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

#[derive(Clone)]
struct LibraryFilter {
    query: String,
    search_mode: SearchMode,
    include_tags: Vec<u32>,
    include_logic: TagLogic,
    exclude_tags: Vec<u32>,
    prefixes: Vec<u32>,
    exclude_prefixes: Vec<u32>,
    unplayed_only: bool,
    sorting: Sorting,
    date_limit: DateLimit,
}

impl Default for LibraryFilter {
    fn default() -> Self {
        Self {
            query: String::new(),
            search_mode: SearchMode::Title,
            include_tags: Vec::new(),
            include_logic: TagLogic::Or,
            exclude_tags: Vec::new(),
            prefixes: Vec::new(),
            exclude_prefixes: Vec::new(),
            unplayed_only: false,
            sorting: Sorting::Date,
            date_limit: DateLimit::Anytime,
        }
    }
}

impl From<&UiState> for LibraryFilter {
    fn from(state: &UiState) -> Self {
        Self {
            query: state.query.trim().to_lowercase(),
            search_mode: state.search_mode.clone(),
            include_tags: state.include_tags.clone(),
            include_logic: state.include_logic.clone(),
            exclude_tags: state.exclude_tags.clone(),
            prefixes: state.prefixes.clone(),
            exclude_prefixes: state.exclude_prefixes.clone(),
            unplayed_only: state.unplayed_only,
            sorting: state.sorting.clone(),
            date_limit: state.date_limit,
        }
    }
}

fn filter_library_cards(cards: &mut Vec<CardRecord>, filter: &LibraryFilter) {
    let settings = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| settings.clone())
        .unwrap_or_default();
    let minimum_timestamp = match filter.date_limit {
        DateLimit::Anytime => None,
        DateLimit::Today => Some(1),
        DateLimit::Days3 => Some(3),
        DateLimit::Days7 => Some(7),
        DateLimit::Days14 => Some(14),
        DateLimit::Days30 => Some(30),
        DateLimit::Days90 => Some(90),
        DateLimit::Days180 => Some(180),
        DateLimit::Days365 => Some(365),
    }
    .map(|days| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        now.saturating_sub(days * 24 * 60 * 60)
    });
    cards.retain(|card| {
        let text_matches = filter.query.is_empty()
            || match filter.search_mode {
                SearchMode::Creator => card.creator.to_lowercase().contains(&filter.query),
                SearchMode::Title => card.title.to_lowercase().contains(&filter.query),
            };
        let include_matches = filter.include_tags.is_empty()
            || match filter.include_logic {
                TagLogic::And => filter.include_tags.iter().all(|id| card.tags.contains(id)),
                TagLogic::Or => filter.include_tags.iter().any(|id| card.tags.contains(id)),
            };
        let excludes_match = !filter.exclude_tags.iter().any(|id| card.tags.contains(id));
        let prefixes_match = filter.prefixes.is_empty()
            || filter.prefixes.iter().any(|id| card.prefixes.contains(id));
        let excluded_prefixes_match = !filter
            .exclude_prefixes
            .iter()
            .any(|id| card.prefixes.contains(id));
        let unplayed_matches = !filter.unplayed_only
            || settings
                .downloaded_games
                .iter()
                .find(|game| game.thread_id == card.id)
                .is_some_and(|game| !game.has_been_launched);
        let date_matches =
            minimum_timestamp.is_none_or(|minimum| card.ts == 0 || card.ts >= minimum);
        text_matches
            && include_matches
            && excludes_match
            && prefixes_match
            && excluded_prefixes_match
            && unplayed_matches
            && date_matches
    });
    match filter.sorting {
        Sorting::Date => cards.sort_by(|left, right| right.ts.cmp(&left.ts)),
        Sorting::Likes => cards.sort_by(|left, right| right.likes.cmp(&left.likes)),
        Sorting::Views => cards.sort_by(|left, right| right.views.cmp(&left.views)),
        Sorting::Title => {
            cards.sort_by(|left, right| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
        }
        Sorting::Rating => cards.sort_by(|left, right| right.rating.total_cmp(&left.rating)),
    }
}

pub(super) fn library_cards_fast(selected: &HashSet<String>) -> Vec<CardRecord> {
    let settings = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| settings.clone())
        .unwrap_or_default();
    let mut seen = HashSet::new();
    let mut cards = settings
        .downloaded_games
        .iter()
        .cloned()
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
                ts: 0,
                likes: 0,
                views: 0,
                rating: 0.0,
                cover_url: None,
                screens: Vec::new(),
                tags: Vec::new(),
                prefixes: Vec::new(),
                cached_cover: cached_cover.exists().then_some(cached_cover),
                installed: true,
                folder: Some(game.folder),
            }
        })
        .collect();
    append_pending_cards(&mut cards, &settings, &mut seen);
    cards
}

pub(super) fn library_cards(selected: &HashSet<String>) -> Vec<CardRecord> {
    let settings = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| settings.clone())
        .unwrap_or_default();
    let mut seen = HashSet::new();
    let mut cards = settings
        .downloaded_games
        .iter()
        .cloned()
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
                    ts: 0,
                    likes: 0,
                    views: 0,
                    rating: 0.0,
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
        .collect();
    append_pending_cards(&mut cards, &settings, &mut seen);
    cards
}

fn append_pending_cards(
    cards: &mut Vec<CardRecord>,
    settings: &crate::app::settings::AppSettings,
    seen: &mut HashSet<u64>,
) {
    for &id in &settings.pending_downloads {
        if !seen.insert(id) {
            continue;
        }
        let cached_cover = settings.cache_dir.join(id.to_string()).join("cover.png");
        if let Some(thread) = crate::app::fetch_helpers::load_from_cache(&settings.cache_dir, id) {
            cards.push(card_from_thread(
                thread,
                false,
                None,
                cached_cover.exists().then_some(cached_cover),
            ));
        } else {
            cards.push(CardRecord {
                id,
                title: format!("Thread #{id}"),
                creator: String::new(),
                version: String::new(),
                prefix: String::new(),
                date: String::new(),
                ts: 0,
                likes: 0,
                views: 0,
                rating: 0.0,
                cover_url: None,
                screens: Vec::new(),
                tags: Vec::new(),
                prefixes: Vec::new(),
                cached_cover: cached_cover.exists().then_some(cached_cover),
                installed: false,
                folder: None,
            });
        }
    }
}

pub(super) fn card_from_thread(
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
        ts: thread.ts,
        likes: thread.likes,
        views: thread.views,
        rating: thread.rating,
        cover_url,
        screens,
        tags: thread.tags,
        prefixes,
        cached_cover,
        installed,
        folder,
    }
}

pub(super) fn cache_card_metadata(card: &CardRecord) {
    let cache_dir = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| settings.cache_dir.clone())
        .unwrap_or_else(|_| PathBuf::from("cache"));
    let previous = crate::app::fetch_helpers::load_from_cache(&cache_dir, card.id);
    let previous_ref = previous.as_ref();
    let thread = F95Thread {
        thread_id: crate::parser::game_info::ThreadId(card.id),
        title: card.title.clone(),
        creator: card.creator.clone(),
        version: card.version.clone(),
        views: card.views,
        likes: card.likes,
        prefixes: if card.prefixes.is_empty() {
            previous_ref
                .map(|thread| thread.prefixes.clone())
                .unwrap_or_default()
        } else {
            card.prefixes.clone()
        },
        tags: if card.tags.is_empty() {
            previous_ref
                .map(|thread| thread.tags.clone())
                .unwrap_or_default()
        } else {
            card.tags.clone()
        },
        rating: card.rating,
        cover: card
            .cover_url
            .clone()
            .or_else(|| previous_ref.map(|thread| thread.cover.clone()))
            .unwrap_or_default(),
        screens: if card.screens.is_empty() {
            previous_ref
                .map(|thread| thread.screens.clone())
                .unwrap_or_default()
        } else {
            card.screens.clone()
        },
        date: card.date.clone(),
        watched: false,
        ignored: false,
        is_new: false,
        ts: card.ts,
    };
    if let Err(error) = crate::app::fetch_helpers::save_to_cache(&cache_dir, card.id, &thread) {
        log::warn!("Failed to cache catalog metadata for {}: {error}", card.id);
    }
}
