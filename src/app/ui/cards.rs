use super::*;

pub(super) fn rebuild_cards(ui: &MainWindow, state: &SharedState) {
    let scroll_y = ui.get_catalog_scroll_y();
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
                    let (warning_count, warning_details, bookmark_badges, unplayed) =
                        card_badges_for_mode(card, &app_settings, state.library_mode);
                    let (download_state, download_progress, download_error) =
                        card_download_visual(&state, card.id);
                    GameCardData {
                        id: card.id.to_string().into(),
                        title: card.title.clone().into(),
                        creator: card.creator.clone().into(),
                        version: card.version.clone().into(),
                        prefix: card.prefix.clone().into(),
                        date: card.date.clone().into(),
                        likes: card.likes.to_string().into(),
                        views: card.views.to_string().into(),
                        rating: format!("{:.1}", card.rating).into(),
                        cover: cached_cover_image(card.id),
                        screens: screen_model(&state, card),
                        screen_loaded: screen_loaded_model(&state, card),
                        screen_count: card.screens.len() as i32,
                        screens_ready: available_screen_count(&state, card) > 0,
                        tag_rows: tag_rows(&card.tags),
                        installed: card.installed,
                        pending_removable: !card.installed
                            && app_settings.pending_downloads.contains(&card.id)
                            && !state.downloads.contains_key(&card.id),
                        warning_count,
                        warning_details: warning_details.into(),
                        bookmark_badges,
                        unplayed,
                        download_state,
                        download_progress,
                        download_display_mode: match app_settings.loading_anim {
                            crate::app::settings::store::LoadingAnim::BottomBar => {
                                DownloadDisplayMode::BottomBar
                            }
                            crate::app::settings::store::LoadingAnim::CircleBottomRight => {
                                DownloadDisplayMode::CircleBottomRight
                            }
                        },
                        download_error: download_error.into(),
                    }
                })
                .collect::<Vec<_>>();
            GameRowData {
                cards: ModelRc::from(Rc::new(VecModel::from(cards))),
            }
        })
        .collect::<Vec<_>>();
    ui.set_game_rows(ModelRc::from(Rc::new(VecModel::from(rows))));
    // Model replacement can make ListView write its default viewport back to
    // the two-way binding. Restore it once now and once after the new model's
    // layout has been calculated on the next event-loop turn.
    ui.set_catalog_scroll_y(scroll_y);
    let weak = ui.as_weak();
    slint::Timer::single_shot(std::time::Duration::ZERO, move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_catalog_scroll_y(scroll_y);
        }
    });
    ui.set_page_label(if state.library_mode {
        tr_with(
            "library-installed-count",
            &[("count", state.cards.len().to_string())],
        )
    } else {
        tr_with(
            "pagination-page",
            &[
                ("cur", state.page.to_string()),
                ("total", state.total_pages.to_string()),
            ],
        )
    });
}

fn card_badges_for_mode(
    card: &CardRecord,
    settings: &crate::app::settings::AppSettings,
    library_mode: bool,
) -> (i32, String, ModelRc<CardBadgeData>, bool) {
    let warning_tags = card
        .tags
        .iter()
        .filter(|id| settings.warn_tags.contains(id))
        .map(|id| crate::tags::get_tag_name_by_id(*id))
        .collect::<Vec<_>>();
    let warning_prefixes = card
        .prefixes
        .iter()
        .filter(|id| settings.warn_prefixes.contains(id))
        .map(|id| crate::tags::get_prefix_name_by_id(*id))
        .collect::<Vec<_>>();
    let warning_count = warning_tags.len() + warning_prefixes.len();
    let mut warning_lines = Vec::new();
    if !warning_tags.is_empty() {
        warning_lines.push("Tags:".to_string());
        warning_lines.extend(warning_tags.iter().map(|name| format!(" • {name}")));
    }
    if !warning_prefixes.is_empty() {
        if !warning_lines.is_empty() {
            warning_lines.push(String::new());
        }
        warning_lines.push("Prefixes:".to_string());
        warning_lines.extend(warning_prefixes.iter().map(|name| format!(" • {name}")));
    }
    let warning_details = warning_lines.join("\n");
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
    let matching = if library_mode {
        settings
            .bookmarks
            .iter()
            .filter(|bookmark| bookmark_ids.contains(&bookmark.id))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut badges = matching
        .iter()
        .take(limit)
        .map(|bookmark| {
            let [red, green, blue] = bookmark.color.unwrap_or(settings.default_bookmark_color);
            CardBadgeData {
                label: bookmark.emoji.clone().into(),
                tooltip: bookmark.label.clone().into(),
                red: red.into(),
                green: green.into(),
                blue: blue.into(),
            }
        })
        .collect::<Vec<_>>();
    if matching.len() > limit {
        badges.push(CardBadgeData {
            label: "…".into(),
            tooltip: matching
                .iter()
                .map(|bookmark| format!("{}  {}", bookmark.emoji, bookmark.label))
                .collect::<Vec<_>>()
                .join("\n")
                .into(),
            red: 60,
            green: 60,
            blue: 60,
        });
    }
    (
        warning_count as i32,
        warning_details,
        ModelRc::from(Rc::new(VecModel::from(badges))),
        unplayed,
    )
}

pub(super) fn update_card_image(ui: &MainWindow, state: &SharedState, id: u64) {
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

pub(super) fn screen_model(_state: &UiState, card: &CardRecord) -> ModelRc<Image> {
    let images = (0..card.screens.len())
        .map(|index| cached_screen_image(card.id, index))
        .collect::<Vec<_>>();
    ModelRc::from(Rc::new(VecModel::from(images)))
}

pub(super) fn screen_loaded_model(state: &UiState, card: &CardRecord) -> ModelRc<bool> {
    let loaded = (0..card.screens.len())
        .map(|index| state.loaded_screen_images.contains(&(card.id, index)))
        .collect::<Vec<_>>();
    ModelRc::from(Rc::new(VecModel::from(loaded)))
}

pub(super) fn available_screen_count(state: &UiState, card: &CardRecord) -> usize {
    (0..card.screens.len())
        .filter(|index| state.loaded_screen_images.contains(&(card.id, *index)))
        .count()
}

pub(super) fn tag_rows(tags: &[u32]) -> ModelRc<TagRowData> {
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
