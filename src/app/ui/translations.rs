use super::*;

pub(super) fn tr(key: &str) -> slint::SharedString {
    crate::localization::translate(key).into()
}

pub(super) fn tr_with(key: &str, arguments: &[(&str, String)]) -> slint::SharedString {
    crate::localization::translate_with(key, arguments).into()
}

fn strings(values: &[&str]) -> ModelRc<slint::SharedString> {
    ModelRc::from(Rc::new(VecModel::from(
        values.iter().map(|key| tr(key)).collect::<Vec<_>>(),
    )))
}

pub(super) fn apply_translations(translations: &AppTranslations, version: &str) {
    translations.set_app_title(tr("app-window-title"));
    translations.set_auth_title(tr("auth-login-title"));
    translations.set_auth_username(tr("auth-username"));
    translations.set_auth_password(tr("auth-password"));
    translations.set_auth_login(tr("auth-login-button"));
    translations.set_auth_authorizing(tr("auth-authorizing"));
    translations.set_auth_cookies(tr("auth-or-paste-cookies"));
    translations.set_auth_use_cookies(tr("auth-use-cookies"));
    translations.set_auth_info(tr("auth-info-needed"));
    translations.set_loading(tr("loading"));
    translations.set_catalog_empty(tr("catalog-empty"));
    translations.set_library_empty(tr("library-empty"));
    translations.set_filters_title(tr("filters-title"));
    translations.set_filters_sorting(tr("filters-sorting"));
    translations.set_filters_date(tr("filters-date-limit"));
    translations.set_filters_search(tr("filters-search"));
    translations.set_filters_tags(tr_with(
        "filters-include-tags-header",
        &[("max", "10".into())],
    ));
    translations.set_filters_exclude_tags(tr_with(
        "filters-exclude-tags-header",
        &[("max", "10".into())],
    ));
    translations.set_filters_prefixes(tr_with(
        "filters-include-prefixes-header",
        &[("max", "10".into())],
    ));
    translations.set_filters_search_placeholder(tr("filters-search-placeholder"));
    translations.set_filters_tag_placeholder(tr("filters-select-tag-include"));
    translations.set_filters_exclude_tag_placeholder(tr("filters-select-tag-exclude"));
    translations.set_filters_prefix_placeholder(tr("filters-select-prefix-include"));
    translations.set_filters_bookmarks(tr("filters-bookmarks-header"));
    translations.set_filters_downloaded(tr("filters-library-button"));
    translations.set_logic_or(tr("tag-logic-or"));
    translations.set_logic_and(tr("tag-logic-and"));
    translations.set_filters_unplayed(tr("filters-unplayed"));
    translations.set_view_catalog(tr("view-mode-catalog"));
    translations.set_view_downloaded(tr("view-mode-downloaded"));
    translations.set_search_title(tr("search-mode-title"));
    translations.set_search_creator(tr("search-mode-creator"));
    translations.set_common_settings(tr("common-settings"));
    translations.set_common_about(tr("common-about"));
    translations.set_common_logs(tr("common-logs"));
    translations.set_common_refresh(tr("common-refresh"));
    translations.set_common_cancel(tr("settings-cancel"));
    translations.set_common_delete(tr("common-delete"));
    translations.set_common_hide(tr("common-hide"));
    translations.set_common_open_f95(tr("common-open-f95"));
    translations.set_common_bookmarks(tr("common-bookmarks"));
    translations.set_common_open_folder(tr("common-open-folder"));
    translations.set_context_remove_library(tr("context-remove-library"));
    translations.set_context_refresh(tr("common-refresh"));
    translations.set_no_bookmarks(tr("bookmarks-mgmt-no-bookmarks"));
    translations.set_delete_title(tr("delete-game-title"));
    translations.set_delete_warning(tr("delete-game-warning"));

    translations.set_settings_title(tr("settings-window-title"));
    translations.set_settings_folders(tr("settings-folders"));
    translations.set_settings_temp_folder(tr("settings-choose-temp"));
    translations.set_settings_games_folder(tr("settings-choose-games"));
    translations.set_settings_cache_folder(tr("settings-choose-cache"));
    translations.set_settings_cover_metadata(tr("settings-cover-metadata"));
    translations.set_settings_choose(tr("common-choose"));
    translations.set_settings_bookmarks(tr("settings-bookmarks-header"));
    translations.set_settings_bookmark_cover_count(tr("settings-bookmark-cover-count"));
    translations.set_settings_bookmark_color(tr("settings-bookmark-color"));
    translations.set_settings_color_red(tr("settings-color-red"));
    translations.set_settings_color_green(tr("settings-color-green"));
    translations.set_settings_color_blue(tr("settings-color-blue"));
    translations.set_settings_interface(tr("settings-interface"));
    translations.set_settings_language(tr("settings-language"));
    translations.set_settings_language_options(strings(&[
        "settings-language-auto",
        "settings-language-en",
        "settings-language-ru",
    ]));
    translations.set_settings_loading_animation(tr("settings-loading-anim"));
    translations.set_settings_loading_options(strings(&[
        "settings-loading-bottom",
        "settings-loading-circle",
    ]));
    translations.set_settings_ui_scale(tr("settings-ui-scale"));
    translations.set_settings_card_scale(tr("settings-card-scale"));
    translations.set_settings_image_cache_games(tr("settings-image-cache-games"));
    translations.set_settings_log_file(tr("settings-log-to-file"));
    translations.set_settings_show_unplayed(tr("settings-show-unplayed-badge"));
    translations.set_settings_classic_library(tr("settings-classic-library-toggle"));
    translations.set_settings_custom_launch(tr("settings-custom-launch"));
    translations.set_settings_startup_filters(tr("settings-startup-filters"));
    translations.set_settings_add_tags(tr("settings-add-tags"));
    translations.set_settings_exclude_tags(tr("settings-exclude-tags"));
    translations.set_settings_add_prefixes(tr("settings-add-prefixes"));
    translations.set_settings_exclude_prefixes(tr("settings-exclude-prefixes"));
    translations.set_settings_max_ten(tr("settings-max-10"));
    translations.set_settings_startup_tag_placeholder(tr("settings-startup-tags-placeholder"));
    translations.set_settings_startup_exclude_tag_placeholder(tr(
        "settings-startup-exclude-tags-placeholder",
    ));
    translations
        .set_settings_startup_prefix_placeholder(tr("settings-startup-prefixes-placeholder"));
    translations.set_settings_startup_exclude_prefix_placeholder(tr(
        "settings-startup-exclude-prefixes-placeholder",
    ));
    translations.set_settings_warnings(tr("settings-custom-warnings"));
    translations.set_settings_warning_tags(tr("settings-warn-tags"));
    translations.set_settings_warning_prefixes(tr("settings-warn-prefixes"));
    translations.set_settings_warning_tag_placeholder(tr("settings-warning-tag-placeholder"));
    translations.set_settings_warning_prefix_placeholder(tr("settings-warning-prefix-placeholder"));
    translations.set_settings_save(tr("settings-save"));

    translations.set_logs_title(tr("logs-window-title"));
    translations.set_logs_clear(tr("logs-clear"));
    translations.set_logs_copy(tr("logs-copy"));
    translations.set_errors_title(tr("errors-title"));
    translations.set_errors_clear(tr("errors-clear"));
    translations.set_download_select_link(tr("download-select-link"));
    translations.set_migration_confirm(tr("settings-move-confirm-text"));
    translations.set_migration_move(tr("settings-move-confirm-move"));
    translations.set_migration_progress(tr("settings-move-progress-text"));
    translations.set_bookmarks_new(tr("bookmarks-new"));
    translations.set_bookmarks_edit(tr("bookmarks-edit"));
    translations.set_bookmarks_create(tr("bookmarks-create"));
    translations.set_bookmarks_save(tr("bookmarks-mgmt-save-btn"));
    translations.set_bookmarks_emoji(tr("bookmarks-emoji"));
    translations.set_bookmarks_name(tr("bookmarks-name"));
    translations.set_bookmarks_color(tr("bookmarks-color"));
    translations.set_bookmarks_delete_confirm(tr("bookmarks-mgmt-delete-confirm"));
    translations.set_about_title(tr("about-window-title"));
    translations.set_about_version(tr_with(
        "about-version",
        &[("version", version.to_string())],
    ));
    translations.set_about_description(tr("about-description"));
    translations.set_about_footer(tr("about-footer"));
}

pub(super) fn date_limit_label(index: i32) -> slint::SharedString {
    let key = match index {
        1 => "date-limit-today",
        2 => "date-limit-days3",
        3 => "date-limit-days7",
        4 => "date-limit-days14",
        5 => "date-limit-days30",
        6 => "date-limit-days90",
        7 => "date-limit-days180",
        8 => "date-limit-days365",
        _ => "date-limit-anytime",
    };
    tr(key)
}

pub(super) fn update_all_translations(
    main: &MainWindow,
    settings: &SettingsWindow,
    logs: &LogsWindow,
    about: &AboutWindow,
    errors: &ErrorsWindow,
    bookmarks: &BookmarksWindow,
) {
    for translations in [
        main.global::<AppTranslations>(),
        settings.global::<AppTranslations>(),
        logs.global::<AppTranslations>(),
        about.global::<AppTranslations>(),
        errors.global::<AppTranslations>(),
        bookmarks.global::<AppTranslations>(),
    ] {
        apply_translations(&translations, env!("CARGO_PKG_VERSION"));
    }
    main.set_date_label(date_limit_label(main.get_date_index()));
}
