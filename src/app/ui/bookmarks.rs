use super::*;

pub(super) fn update_bookmarks(ui: &MainWindow, state: &SharedState) {
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

pub(super) fn game_bookmark_model(thread_id: u64) -> ModelRc<BookmarkData> {
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

pub(super) fn update_bookmark_editor(window: &BookmarksWindow) {
    let settings = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| settings.clone())
        .unwrap_or_default();
    let items = settings
        .bookmarks
        .iter()
        .map(|bookmark| {
            let [red, green, blue] = bookmark.color.unwrap_or(settings.default_bookmark_color);
            BookmarkEditorData {
                id: bookmark.id.clone().into(),
                emoji: bookmark.emoji.clone().into(),
                label: bookmark.label.clone().into(),
                red: red.into(),
                green: green.into(),
                blue: blue.into(),
            }
        })
        .collect::<Vec<_>>();
    window.set_items(ModelRc::from(Rc::new(VecModel::from(items))));
}

pub(super) fn empty_rows() -> ModelRc<GameRowData> {
    ModelRc::from(Rc::new(VecModel::<GameRowData>::default()))
}
