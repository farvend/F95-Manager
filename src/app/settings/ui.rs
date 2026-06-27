// Settings UI: egui viewport window and separate eframe App, plus UI state.

use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc;

use super::migrate;
use super::store::{save_settings_to_disk, AppSettings, APP_SETTINGS, LoadingAnim, UpdateCheckFrequency};
use crate::views::filters::items::{prefixes_menu::prefixes_picker, tags_menu::tags_picker};

type MigratedGame = (u64, PathBuf, Option<PathBuf>);

enum MigrationMsg {
    Completed(Vec<MigratedGame>),
}

#[derive(Debug, Clone)]
struct PendingMoveState {
    temp_dir: String,
    extract_dir: String,
    old_extract_dir: PathBuf,
}

pub struct SettingsUiState {
    pub is_open: bool,
    temp_dir_input: String,
    extract_dir_input: String,
    cache_dir_input: String,
    custom_launch_input: String,
    cache_on_download_input: bool,
    language_input: Option<crate::localization::SupportedLang>,
    loading_anim_input: LoadingAnim,
    log_to_file_input: bool,
    update_freq_input: UpdateCheckFrequency,
    show_unplayed_badge_input: bool,
    classic_library_toggle_input: bool,
    default_bookmark_color_input: [u8; 3],
    bookmarks_visible_on_cover_input: u8,
    move_confirm_open: bool,
    pending_move: Option<PendingMoveState>,
    warn_tags_input: Vec<u32>,
    warn_prefixes_input: Vec<u32>,
    startup_tags_input: Vec<u32>,
    startup_exclude_tags_input: Vec<u32>,
    startup_prefixes_input: Vec<u32>,
    startup_exclude_prefixes_input: Vec<u32>,
    move_running: bool,
    move_result: Option<Vec<MigratedGame>>,
    move_error: Option<String>,
    migration_tx: mpsc::Sender<MigrationMsg>,
    migration_rx: mpsc::Receiver<MigrationMsg>,
}

impl Default for SettingsUiState {
    fn default() -> Self {
        let (migration_tx, migration_rx) = mpsc::channel();
        Self {
            is_open: false,
            temp_dir_input: String::new(),
            extract_dir_input: String::new(),
            cache_dir_input: String::new(),
            custom_launch_input: String::new(),
            cache_on_download_input: false,
            language_input: None,
            loading_anim_input: LoadingAnim::BottomBar,
            log_to_file_input: true,
            update_freq_input: UpdateCheckFrequency::Manual,
            show_unplayed_badge_input: false,
            classic_library_toggle_input: false,
            default_bookmark_color_input: [60, 120, 200],
            bookmarks_visible_on_cover_input: 3,
            move_confirm_open: false,
            pending_move: None,
            warn_tags_input: Vec::new(),
            warn_prefixes_input: Vec::new(),
            startup_tags_input: Vec::new(),
            startup_exclude_tags_input: Vec::new(),
            startup_prefixes_input: Vec::new(),
            startup_exclude_prefixes_input: Vec::new(),
            move_running: false,
            move_result: None,
            move_error: None,
            migration_tx,
            migration_rx,
        }
    }
}

impl SettingsUiState {
    fn load_from_settings(&mut self, settings: &AppSettings) {
        self.temp_dir_input = settings.temp_dir.to_string_lossy().to_string();
        self.extract_dir_input = settings.extract_dir.to_string_lossy().to_string();
        self.cache_dir_input = settings.cache_dir.to_string_lossy().to_string();
        self.custom_launch_input = settings.custom_launch.clone();
        self.cache_on_download_input = settings.cache_on_download;
        self.language_input = settings.language;
        self.loading_anim_input = settings.loading_anim;
        self.log_to_file_input = settings.log_to_file;
        self.update_freq_input = settings.update_check_frequency.clone();
        self.show_unplayed_badge_input = settings.show_unplayed_badge;
        self.classic_library_toggle_input = settings.classic_library_toggle;
        self.default_bookmark_color_input = settings.default_bookmark_color;
        self.bookmarks_visible_on_cover_input = settings.bookmarks_visible_on_cover;
        self.warn_tags_input = settings.warn_tags.clone();
        self.warn_prefixes_input = settings.warn_prefixes.clone();
        self.startup_tags_input = settings.startup_tags.clone();
        self.startup_exclude_tags_input = settings.startup_exclude_tags.clone();
        self.startup_prefixes_input = settings.startup_prefixes.clone();
        self.startup_exclude_prefixes_input = settings.startup_exclude_prefixes.clone();
        self.move_confirm_open = false;
        self.pending_move = None;
        self.move_running = false;
        self.move_result = None;
        self.move_error = None;
        while self.migration_rx.try_recv().is_ok() {}
    }

    fn apply_to_settings(&self, settings: &mut AppSettings, temp_dir: PathBuf, extract_dir: PathBuf) {
        settings.temp_dir = temp_dir;
        settings.extract_dir = extract_dir;
        settings.warn_tags = self.warn_tags_input.clone();
        settings.warn_prefixes = self.warn_prefixes_input.clone();
        settings.startup_tags = self.startup_tags_input.clone();
        settings.startup_exclude_tags = self.startup_exclude_tags_input.clone();
        settings.startup_prefixes = self.startup_prefixes_input.clone();
        settings.startup_exclude_prefixes = self.startup_exclude_prefixes_input.clone();
        settings.custom_launch = self.custom_launch_input.clone();
        settings.cache_on_download = self.cache_on_download_input;
        settings.cache_dir = PathBuf::from(&self.cache_dir_input);
        settings.loading_anim = self.loading_anim_input;
        settings.language = self.language_input;
        settings.log_to_file = self.log_to_file_input;
        settings.update_check_frequency = self.update_freq_input.clone();
        settings.show_unplayed_badge = self.show_unplayed_badge_input;
        settings.classic_library_toggle = self.classic_library_toggle_input;
        settings.default_bookmark_color = self.default_bookmark_color_input;
        settings.bookmarks_visible_on_cover = self.bookmarks_visible_on_cover_input;
    }

    fn poll_migration_updates(&mut self) {
        while let Ok(msg) = self.migration_rx.try_recv() {
            match msg {
                MigrationMsg::Completed(moved) => {
                    self.move_running = false;
                    self.move_result = Some(moved);
                }
            }
        }
    }

    fn close(&mut self, ctx: &egui::Context) {
        self.is_open = false;
        self.move_confirm_open = false;
        self.pending_move = None;
        self.move_result = None;
        self.move_error = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

pub fn open_settings(state: &mut SettingsUiState) {
    super::with_settings(|settings| state.load_from_settings(settings));
    state.is_open = true;
}

fn calc_settings_window_size(state: &SettingsUiState) -> [f32; 2] {
    const ROW_HEIGHT: f32 = 26.0;
    const FIXED_UI_ROWS: f32 = 28.0;
    const TAG_PICKER_SECTIONS: f32 = 6.0;
    const PICKER_HEIGHT: f32 = 28.0;
    const PADDING: f32 = 40.0;
    const CHIPS_PER_ROW: f32 = 4.0;

    let total_chips = state.startup_tags_input.len()
        + state.startup_exclude_tags_input.len()
        + state.startup_prefixes_input.len()
        + state.startup_exclude_prefixes_input.len()
        + state.warn_tags_input.len()
        + state.warn_prefixes_input.len();
    let chip_rows = (total_chips as f32 / CHIPS_PER_ROW).ceil();

    let height = FIXED_UI_ROWS * ROW_HEIGHT
        + TAG_PICKER_SECTIONS * PICKER_HEIGHT
        + chip_rows * ROW_HEIGHT
        + PADDING;

    [620.0, height.max(500.0).min(900.0)]
}

fn prefix_label(id: u32) -> String {
    for group in &crate::tags::TAGS.prefixes.games {
        if let Some(prefix) = group.prefixes.iter().find(|prefix| prefix.id as u32 == id) {
            return prefix.name.clone();
        }
    }
    id.to_string()
}

fn render_tag_chip_list(ui: &mut egui::Ui, list: &mut Vec<u32>) {
    let mut to_remove = None;
    for (index, id) in list.clone().iter().enumerate() {
        let name = crate::tags::TAGS
            .tags
            .get(&id.to_string())
            .cloned()
            .unwrap_or_else(|| id.to_string());
        if ui.button(format!("{} ×", name)).clicked() {
            to_remove = Some(index);
        }
    }
    if let Some(index) = to_remove {
        list.remove(index);
    }
}

fn render_prefix_chip_list(ui: &mut egui::Ui, list: &mut Vec<u32>) {
    let mut to_remove = None;
    for (index, id) in list.clone().iter().enumerate() {
        if ui.button(format!("{} ×", prefix_label(*id))).clicked() {
            to_remove = Some(index);
        }
    }
    if let Some(index) = to_remove {
        list.remove(index);
    }
}

fn render_folder_picker_row(ui: &mut egui::Ui, label: String, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        let current_value = value.clone();
        let resp = ui.add(egui::Label::new(current_value.clone()).sense(egui::Sense::click()));
        if resp.clicked() {
            let init = if !current_value.is_empty() {
                PathBuf::from(&current_value)
            } else {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            };
            if let Some(dir) = rfd::FileDialog::new().set_directory(init).pick_folder() {
                *value = dir.to_string_lossy().to_string();
            }
        }
    });
}

fn apply_runtime_settings(state: &SettingsUiState) {
    if let Some(lang) = state.language_input {
        let _ = crate::localization::set_current_language(lang);
    } else {
        let _ = crate::localization::set_language_auto();
    }
    crate::logger::set_file_logging_enabled(state.log_to_file_input);
}

fn apply_settings_and_close(
    ctx: &egui::Context,
    state: &mut SettingsUiState,
    temp_dir: PathBuf,
    extract_dir: PathBuf,
    moved_games: Option<Vec<MigratedGame>>,
) {
    {
        let mut settings = APP_SETTINGS.write().unwrap();
        state.apply_to_settings(&mut settings, temp_dir, extract_dir);
        if let Some(moved_games) = moved_games {
            for (thread_id, new_folder, new_exe_path) in moved_games {
                if let Some(entry) = settings
                    .downloaded_games
                    .iter_mut()
                    .find(|entry| entry.thread_id == thread_id)
                {
                    entry.folder = new_folder;
                    if let Some(new_exe_path) = new_exe_path {
                        entry.exe_path = Some(new_exe_path);
                    }
                }
            }
        }
    }

    apply_runtime_settings(state);
    save_settings_to_disk();
    state.close(ctx);
}

fn queue_extract_dir_migration(state: &mut SettingsUiState, old_extract_dir: PathBuf) {
    state.pending_move = Some(PendingMoveState {
        temp_dir: state.temp_dir_input.clone(),
        extract_dir: state.extract_dir_input.clone(),
        old_extract_dir,
    });
    state.move_confirm_open = true;
}

fn start_extract_dir_migration(state: &mut SettingsUiState) {
    let Some(pending_move) = state.pending_move.clone() else {
        return;
    };

    let new_extract_dir = PathBuf::from(&pending_move.extract_dir);
    let old_extract_dir = pending_move.old_extract_dir.clone();
    let entries: Vec<MigratedGame> = super::with_settings(|settings| {
        settings
            .downloaded_games
            .iter()
            .map(|entry| (entry.thread_id, entry.folder.clone(), entry.exe_path.clone()))
            .collect()
    });

    state.move_result = None;
    state.move_error = None;
    state.move_running = true;
    state.move_confirm_open = false;

    let tx = state.migration_tx.clone();
    std::thread::spawn(move || {
        let moved = migrate::migrate_installed_games(&old_extract_dir, &new_extract_dir, entries);
        let _ = tx.send(MigrationMsg::Completed(moved));
    });
}

fn handle_save_clicked(ctx: &egui::Context, state: &mut SettingsUiState) {
    let temp_dir = PathBuf::from(&state.temp_dir_input);
    let extract_dir = PathBuf::from(&state.extract_dir_input);
    let (old_extract_dir, has_installed_games) =
        super::with_settings(|settings| (settings.extract_dir.clone(), !settings.downloaded_games.is_empty()));

    if has_installed_games && extract_dir != old_extract_dir {
        queue_extract_dir_migration(state, old_extract_dir);
    } else {
        apply_settings_and_close(ctx, state, temp_dir, extract_dir, None);
    }
}

pub fn draw_settings_viewport(ctx: &egui::Context, state: &mut SettingsUiState) {
    if !state.is_open {
        return;
    }

    state.poll_migration_updates();

    let size = calc_settings_window_size(state);
    let viewport_id = egui::ViewportId::from_hash_of("settings_window");
    ctx.show_viewport_immediate(
        viewport_id,
        egui::ViewportBuilder::default()
            .with_title(crate::localization::translate("settings-window-title"))
            .with_inner_size(size)
            .with_resizable(true),
        |ctx, _class| {
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    render_folder_picker_row(
                        ui,
                        crate::localization::translate("settings-temp-folder"),
                        &mut state.temp_dir_input,
                    );
                    render_folder_picker_row(
                        ui,
                        crate::localization::translate("settings-extract-folder"),
                        &mut state.extract_dir_input,
                    );
                    render_folder_picker_row(
                        ui,
                        crate::localization::translate("settings-cache-folder"),
                        &mut state.cache_dir_input,
                    );

                    ui.separator();

                    ui.heading(crate::localization::translate("settings-bookmarks-header"));
                    if ui
                        .button(crate::localization::translate("settings-bookmarks-mgmt-btn"))
                        .clicked()
                    {
                        crate::views::bookmarks_management::open_bookmarks_management();
                    }

                    ui.horizontal(|ui| {
                        ui.label(crate::localization::translate("settings-bookmarks-visible-limit"));
                        ui.add(egui::Slider::new(
                            &mut state.bookmarks_visible_on_cover_input,
                            1..=5,
                        ));
                    });

                    ui.horizontal(|ui| {
                        ui.label(crate::localization::translate("settings-bookmarks-default-color"));
                        let mut color_f32 = [
                            state.default_bookmark_color_input[0] as f32 / 255.0,
                            state.default_bookmark_color_input[1] as f32 / 255.0,
                            state.default_bookmark_color_input[2] as f32 / 255.0,
                        ];
                        if ui.color_edit_button_rgb(&mut color_f32).changed() {
                            state.default_bookmark_color_input = [
                                (color_f32[0] * 255.0) as u8,
                                (color_f32[1] * 255.0) as u8,
                                (color_f32[2] * 255.0) as u8,
                            ];
                        }
                    });

                    ui.separator();

                    let selected_language = match state.language_input {
                        Some(crate::localization::SupportedLang::English) => {
                            crate::localization::translate("settings-language-en")
                        }
                        Some(crate::localization::SupportedLang::Russian) => {
                            crate::localization::translate("settings-language-ru")
                        }
                        None => crate::localization::translate("settings-language-auto"),
                    };
                    ui.horizontal(|ui| {
                        ui.label(crate::localization::translate("settings-language"));
                        egui::ComboBox::from_id_source("settings_language_combo")
                            .selected_text(selected_language)
                            .show_ui(ui, |ui| {
                                let lbl_auto = crate::localization::translate("settings-language-auto");
                                let lbl_en = crate::localization::translate("settings-language-en");
                                let lbl_ru = crate::localization::translate("settings-language-ru");
                                ui.selectable_value(&mut state.language_input, None, lbl_auto);
                                ui.selectable_value(
                                    &mut state.language_input,
                                    Some(crate::localization::SupportedLang::English),
                                    lbl_en,
                                );
                                ui.selectable_value(
                                    &mut state.language_input,
                                    Some(crate::localization::SupportedLang::Russian),
                                    lbl_ru,
                                );
                            });
                    });

                    let selected_anim = match state.loading_anim_input {
                        LoadingAnim::BottomBar => {
                            crate::localization::translate("settings-loading-anim-bottom-bar")
                        }
                        LoadingAnim::CircleBottomRight => crate::localization::translate(
                            "settings-loading-anim-circle-bottom-right",
                        ),
                    };
                    ui.horizontal(|ui| {
                        ui.label(crate::localization::translate("settings-loading-anim"));
                        egui::ComboBox::from_id_source("settings_loading_anim_combo")
                            .selected_text(selected_anim)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut state.loading_anim_input,
                                    LoadingAnim::BottomBar,
                                    crate::localization::translate(
                                        "settings-loading-anim-bottom-bar",
                                    ),
                                );
                                ui.selectable_value(
                                    &mut state.loading_anim_input,
                                    LoadingAnim::CircleBottomRight,
                                    crate::localization::translate(
                                        "settings-loading-anim-circle-bottom-right",
                                    ),
                                );
                            });
                    });

                    ui.separator();

                    let selected_frequency = match &state.update_freq_input {
                        UpdateCheckFrequency::Manual => {
                            crate::localization::translate("settings-update-manual")
                        }
                        UpdateCheckFrequency::OnStartup => {
                            crate::localization::translate("settings-update-on-startup")
                        }
                        UpdateCheckFrequency::EveryNDays(days) => crate::localization::translate_with(
                            "settings-update-every-n-days",
                            &[("days", days.to_string())],
                        ),
                    };
                    ui.horizontal(|ui| {
                        ui.label(crate::localization::translate("settings-update-frequency"));
                        egui::ComboBox::from_id_source("settings_update_freq_combo")
                            .selected_text(selected_frequency)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut state.update_freq_input,
                                    UpdateCheckFrequency::Manual,
                                    crate::localization::translate("settings-update-manual"),
                                );
                                ui.selectable_value(
                                    &mut state.update_freq_input,
                                    UpdateCheckFrequency::OnStartup,
                                    crate::localization::translate("settings-update-on-startup"),
                                );
                                ui.selectable_value(
                                    &mut state.update_freq_input,
                                    UpdateCheckFrequency::EveryNDays(7),
                                    crate::localization::translate_with(
                                        "settings-update-every-n-days",
                                        &[("days", "7".to_string())],
                                    ),
                                );
                            });
                    });

                    ui.horizontal(|ui| {
                        if ui
                            .button(crate::localization::translate("settings-check-updates"))
                            .clicked()
                        {
                            crate::app::game_updates::ui::trigger_update_check(ctx);
                        }

                        let updates_available = crate::app::game_updates::ui::GAMES_WITH_UPDATES
                            .read()
                            .map(|games| !games.is_empty())
                            .unwrap_or(false);

                        if updates_available
                            && ui
                                .button(crate::localization::translate("settings-update-all"))
                                .clicked()
                        {
                            crate::app::game_updates::ui::trigger_update_all();
                            ctx.request_repaint();
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.checkbox(
                            &mut state.log_to_file_input,
                            crate::localization::translate("settings-log-to-file"),
                        );
                    });

                    ui.horizontal(|ui| {
                        ui.checkbox(
                            &mut state.show_unplayed_badge_input,
                            crate::localization::translate("settings-show-unplayed-badge"),
                        );
                    });

                    ui.horizontal(|ui| {
                        ui.checkbox(
                            &mut state.classic_library_toggle_input,
                            crate::localization::translate("settings-classic-library-toggle"),
                        );
                    });

                    ui.label(crate::localization::translate("settings-custom-launch"));
                    ui.add(
                        egui::TextEdit::singleline(&mut state.custom_launch_input)
                            .hint_text("\"C:\\\\Start.exe\" /box:TestBox {{path}}"),
                    );

                    ui.separator();

                    ui.label(crate::localization::translate("settings-startup-tags"));
                    if let Some(id) = tags_picker(
                        ui,
                        "settings_startup_tags",
                        crate::localization::translate("settings-startup-tags-placeholder").as_str(),
                    ) {
                        if state.startup_tags_input.len() < 10
                            && !state.startup_tags_input.contains(&id)
                        {
                            state.startup_tags_input.push(id);
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        render_tag_chip_list(ui, &mut state.startup_tags_input);
                    });

                    ui.add_space(crate::ui_constants::card::STATS_MARGIN_V);
                    ui.label(crate::localization::translate("settings-startup-exclude-tags"));
                    if let Some(id) = tags_picker(
                        ui,
                        "settings_startup_exclude_tags",
                        crate::localization::translate("settings-startup-exclude-tags-placeholder")
                            .as_str(),
                    ) {
                        if state.startup_exclude_tags_input.len() < 10
                            && !state.startup_exclude_tags_input.contains(&id)
                        {
                            state.startup_exclude_tags_input.push(id);
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        render_tag_chip_list(ui, &mut state.startup_exclude_tags_input);
                    });

                    ui.add_space(crate::ui_constants::card::STATS_MARGIN_V);
                    ui.label(crate::localization::translate("settings-startup-prefixes"));
                    if let Some(id) = prefixes_picker(
                        ui,
                        "settings_startup_prefixes",
                        crate::localization::translate("settings-startup-prefixes-placeholder")
                            .as_str(),
                    ) {
                        if state.startup_prefixes_input.len() < 10
                            && !state.startup_prefixes_input.contains(&id)
                        {
                            state.startup_prefixes_input.push(id);
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        render_prefix_chip_list(ui, &mut state.startup_prefixes_input);
                    });

                    ui.add_space(crate::ui_constants::card::STATS_MARGIN_V);
                    ui.label(crate::localization::translate("settings-startup-exclude-prefixes"));
                    if let Some(id) = prefixes_picker(
                        ui,
                        "settings_startup_exclude_prefixes",
                        crate::localization::translate(
                            "settings-startup-exclude-prefixes-placeholder",
                        )
                        .as_str(),
                    ) {
                        if state.startup_exclude_prefixes_input.len() < 10
                            && !state.startup_exclude_prefixes_input.contains(&id)
                        {
                            state.startup_exclude_prefixes_input.push(id);
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        render_prefix_chip_list(ui, &mut state.startup_exclude_prefixes_input);
                    });

                    ui.add_space(crate::ui_constants::card::STATS_MARGIN_V);
                    ui.label(crate::localization::translate("settings-warn-heading"));

                    ui.label(crate::localization::translate("settings-warn-tags"));
                    if let Some(id) = tags_picker(
                        ui,
                        "settings_warn_tags",
                        crate::localization::translate("settings-warn-tags-placeholder").as_str(),
                    ) {
                        if !state.warn_tags_input.contains(&id) {
                            state.warn_tags_input.push(id);
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        render_tag_chip_list(ui, &mut state.warn_tags_input);
                    });

                    ui.add_space(crate::ui_constants::card::STATS_MARGIN_V);
                    ui.label(crate::localization::translate("settings-warn-prefixes"));
                    if let Some(id) = prefixes_picker(
                        ui,
                        "settings_warn_prefixes",
                        crate::localization::translate("settings-warn-prefixes-placeholder").as_str(),
                    ) {
                        if !state.warn_prefixes_input.contains(&id) {
                            state.warn_prefixes_input.push(id);
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        render_prefix_chip_list(ui, &mut state.warn_prefixes_input);
                    });

                    ui.add_space(crate::ui_constants::spacing::MEDIUM);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        if ui.button(crate::localization::translate("settings-save")).clicked() {
                            handle_save_clicked(ctx, state);
                        }
                        if ctx.input(|input| input.viewport().close_requested()) && !state.move_running {
                            state.close(ctx);
                        }
                        ui.add_space(crate::ui_constants::spacing::MEDIUM);

                        if state.move_confirm_open {
                            egui::Window::new(crate::localization::translate(
                                "settings-move-confirm-title",
                            ))
                            .collapsible(false)
                            .resizable(false)
                            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                            .show(ctx, |ui| {
                                ui.label(crate::localization::translate(
                                    "settings-move-confirm-text",
                                ));
                                ui.add_space(crate::ui_constants::spacing::MEDIUM);
                                ui.horizontal(|ui| {
                                    if ui
                                        .button(crate::localization::translate(
                                            "settings-move-confirm-move",
                                        ))
                                        .clicked()
                                    {
                                        start_extract_dir_migration(state);
                                    }
                                    if ui.button(crate::localization::translate("settings-cancel")).clicked() {
                                        state.move_confirm_open = false;
                                    }
                                });
                            });
                        }
                    });
                });
            });

            if state.move_running {
                egui::Window::new(crate::localization::translate("settings-move-progress-title"))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(crate::localization::translate("settings-move-progress-text"));
                    });
                ctx.request_repaint();
            } else if let Some(moved_games) = state.move_result.take() {
                if let Some(pending_move) = state.pending_move.take() {
                    apply_settings_and_close(
                        ctx,
                        state,
                        PathBuf::from(pending_move.temp_dir),
                        PathBuf::from(pending_move.extract_dir),
                        Some(moved_games),
                    );
                }
            }

            if let Some(error) = state.move_error.clone() {
                egui::Window::new(crate::localization::translate("errors-window-title"))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(error);
                        if ui.button("OK").clicked() {
                            state.move_error = None;
                        }
                    });
            }
        },
    );
}
