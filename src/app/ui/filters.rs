use super::*;

pub(super) fn sorting_from_index(index: i32) -> Sorting {
    match index {
        1 => Sorting::Likes,
        2 => Sorting::Views,
        3 => Sorting::Title,
        4 => Sorting::Rating,
        _ => Sorting::Date,
    }
}

pub(super) fn date_limit_from_index(index: i32) -> DateLimit {
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

pub(super) fn update_selected_filters(ui: &MainWindow, state: &SharedState) {
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
    ui.set_selected_exclude_prefixes(to_model(&state.exclude_prefixes, true));
}

pub(super) fn empty_suggestions() -> ModelRc<SuggestionData> {
    ModelRc::from(Rc::new(VecModel::<SuggestionData>::default()))
}

pub(super) fn settings_filter_model(ids: &[u32], prefix: bool) -> ModelRc<SuggestionData> {
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

pub(super) fn update_settings_filter_models(
    window: &SettingsWindow,
    filters: &SettingsFilterState,
) {
    window.set_startup_tags(settings_filter_model(&filters.values[0], false));
    window.set_startup_exclude_tags(settings_filter_model(&filters.values[1], false));
    window.set_startup_prefixes(settings_filter_model(&filters.values[2], true));
    window.set_startup_exclude_prefixes(settings_filter_model(&filters.values[3], true));
    window.set_warning_tags(settings_filter_model(&filters.values[4], false));
    window.set_warning_prefixes(settings_filter_model(&filters.values[5], true));
}

pub(super) fn refresh_logs(window: &LogsWindow) {
    let text = crate::logger::get_all().join("\n");
    window.set_contents(text.into());
    let mut lines = Vec::new();
    crate::logger::for_each_range(0, crate::logger::len(), |entry| {
        let (red, green, blue) = match entry.level {
            log::Level::Error => (220, 80, 80),
            log::Level::Warn => (235, 200, 80),
            log::Level::Info => (200, 200, 200),
            log::Level::Debug => (120, 180, 255),
            log::Level::Trace => (160, 160, 160),
        };
        let location = match (&entry.file, entry.line) {
            (Some(file), Some(line)) => format!(" @ {file}:{line}"),
            (Some(file), None) => format!(" @ {file}"),
            _ => String::new(),
        };
        lines.push(LogLineData {
            text: format!(
                "[{:>5}] {}{}: {}",
                entry.level, entry.target, location, entry.msg
            )
            .into(),
            red,
            green,
            blue,
        });
    });
    window.set_lines(ModelRc::from(Rc::new(VecModel::from(lines))));
}

pub(super) fn matching_suggestions(query: &str, kind: i32) -> ModelRc<SuggestionData> {
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
