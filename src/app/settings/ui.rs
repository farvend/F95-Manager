// Settings UI: egui viewport window and separate eframe App, plus UI state.

use eframe::{egui, App};
use lazy_static::lazy_static;
use std::path::PathBuf;
use std::sync::{mpsc, RwLock};

use super::helpers::move_directory;
use super::store::{save_settings_to_disk, APP_SETTINGS};
use crate::views::filters::items::{tags_menu::tags_picker, prefixes_menu::prefixes_picker};
use super::migrate;


lazy_static! {
    pub static ref SETTINGS_OPEN: RwLock<bool> = RwLock::new(false);
    static ref TEMP_DIR_INPUT: RwLock<String> = RwLock::new(String::new());
    static ref EXTRACT_DIR_INPUT: RwLock<String> = RwLock::new(String::new());
    static ref CACHE_DIR_INPUT: RwLock<String> = RwLock::new(String::new());
    static ref CUSTOM_LAUNCH_INPUT: RwLock<String> = RwLock::new(String::new());
    // Toggle: cache metadata/images on download click
    static ref CACHE_ON_DOWNLOAD_INPUT: RwLock<bool> = RwLock::new(false);
    // State for extract-dir change confirmation and migration
    static ref MOVE_CONFIRM_OPEN: RwLock<bool> = RwLock::new(false);
    static ref PENDING_TEMP_DIR: RwLock<String> = RwLock::new(String::new());
    static ref PENDING_EXTRACT_DIR: RwLock<String> = RwLock::new(String::new());
    static ref PENDING_OLD_EXTRACT_DIR: RwLock<PathBuf> = RwLock::new(PathBuf::new());
    // Warnings configuration (staged values for Save/Cancel)
    static ref WARN_TAGS_INPUT: RwLock<Vec<u32>> = RwLock::new(Vec::new());
    static ref WARN_PREFIXES_INPUT: RwLock<Vec<u32>> = RwLock::new(Vec::new());
    // Migration background task state
    static ref MOVE_RUNNING: RwLock<bool> = RwLock::new(false);
    static ref MOVE_RESULT: RwLock<Option<Vec<(u64, PathBuf, Option<PathBuf>)>>> = RwLock::new(None);
    static ref MOVE_ERROR: RwLock<Option<String>> = RwLock::new(None);
}

#[derive(Debug, Clone)]
pub enum SettingsMsg {
    Update { temp_dir: String, extract_dir: String },
}

pub fn open_settings() {
    let s = APP_SETTINGS.read().unwrap();
    {
        let mut tmp = TEMP_DIR_INPUT.write().unwrap();
        *tmp = s.temp_dir.to_string_lossy().to_string();
    }
    {
        let mut ext = EXTRACT_DIR_INPUT.write().unwrap();
        *ext = s.extract_dir.to_string_lossy().to_string();
    }
    {
        let mut cd = CACHE_DIR_INPUT.write().unwrap();
        *cd = s.cache_dir.to_string_lossy().to_string();
    }
    {
        let mut cl = CUSTOM_LAUNCH_INPUT.write().unwrap();
        *cl = s.custom_launch.clone();
    }
    {
        let mut b = CACHE_ON_DOWNLOAD_INPUT.write().unwrap();
        *b = s.cache_on_download;
    }
    {
        let mut v = WARN_TAGS_INPUT.write().unwrap();
        *v = s.warn_tags.clone();
    }
    {
        let mut v = WARN_PREFIXES_INPUT.write().unwrap();
        *v = s.warn_prefixes.clone();
    }
    *SETTINGS_OPEN.write().unwrap() = true;
}

pub fn draw_settings_viewport(ctx: &egui::Context) {
    if !*SETTINGS_OPEN.read().unwrap() {
        return;
    }
    let viewport_id = egui::ViewportId::from_hash_of("settings_window");
    ctx.show_viewport_immediate(
        viewport_id,
        egui::ViewportBuilder::default()
            .with_title("Settings")
            .with_inner_size([640.0, 420.0])
            .with_resizable(true),
        move |ctx, _class| {
            egui::CentralPanel::default().show(ctx, |ui| {
                // Temp folder
                ui.horizontal(|ui| {
                    ui.label("Temp folder:");
                    let temp_val = TEMP_DIR_INPUT.read().unwrap().clone();
                    let resp = ui.add(egui::Label::new(temp_val.clone()).sense(egui::Sense::click()));
                    if resp.clicked() {
                        let init = if !temp_val.is_empty() {
                            std::path::PathBuf::from(temp_val.clone())
                        } else {
                            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
                        };
                        if let Some(dir) = rfd::FileDialog::new().set_directory(init).pick_folder() {
                            *TEMP_DIR_INPUT.write().unwrap() = dir.to_string_lossy().to_string();
                        }
                    }
                });
                // Extract-to folder
                ui.horizontal(|ui| {
                    ui.label("Extract-to folder:");
                    let extract_val = EXTRACT_DIR_INPUT.read().unwrap().clone();
                    let resp =
                        ui.add(egui::Label::new(extract_val.clone()).sense(egui::Sense::click()));
                    if resp.clicked() {
                        let init = if !extract_val.is_empty() {
                            std::path::PathBuf::from(extract_val.clone())
                        } else {
                            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
                        };
                        if let Some(dir) = rfd::FileDialog::new().set_directory(init).pick_folder() {
                            *EXTRACT_DIR_INPUT.write().unwrap() = dir.to_string_lossy().to_string();
                        }
                    }
                });
                // Cache folder
                ui.horizontal(|ui| {
                    ui.label("Cache folder:");
                    let cache_val = CACHE_DIR_INPUT.read().unwrap().clone();
                    let resp = ui.add(egui::Label::new(cache_val.clone()).sense(egui::Sense::click()));
                    if resp.clicked() {
                        let init = if !cache_val.is_empty() {
                            std::path::PathBuf::from(cache_val.clone())
                        } else {
                            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
                        };
                        if let Some(dir) = rfd::FileDialog::new().set_directory(init).pick_folder() {
                            *CACHE_DIR_INPUT.write().unwrap() = dir.to_string_lossy().to_string();
                        }
                    }
                });
                //ui.add_space(8.0);
                ui.separator();

                ui.label("Custom launch command (use {{path}} placeholder):");
                {
                    let mut custom_val = CUSTOM_LAUNCH_INPUT.read().unwrap().clone();
                    if ui.add(egui::TextEdit::singleline(&mut custom_val).hint_text("\"C:\\\\Start.exe\" /box:TestBox {{path}}")).changed() {
                        *CUSTOM_LAUNCH_INPUT.write().unwrap() = custom_val;
                    }
                }

                //ui.add_space(8.0);
                ui.separator();
                // ui.separator();
                // // Toggle: cache metadata/images on download click
                // ui.horizontal(|ui| {
                //     let mut cache_val = *CACHE_ON_DOWNLOAD_INPUT.read().unwrap();
                //     if ui.checkbox(&mut cache_val, "Cache metadata/images on download").on_hover_text("Saves thread meta to cache/<id>/meta.json and images (cover + screenshots) to cache/<id> when you click download.").changed() {
                //         *CACHE_ON_DOWNLOAD_INPUT.write().unwrap() = cache_val;
                //     }
                // });
                ui.label("Warn on tags/prefixes:");

                ui.label("Warn tags:");
                if let Some(id) = tags_picker(ui, "settings_warn_tags", "Select a tag to warn...") {
                    let mut list = WARN_TAGS_INPUT.write().unwrap();
                    if !list.contains(&id) {
                        list.push(id);
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    let mut to_remove: Option<usize> = None;
                    let list_clone = { WARN_TAGS_INPUT.read().unwrap().clone() };
                    for (i, id) in list_clone.iter().enumerate() {
                        let name = crate::tags::TAGS
                            .tags
                            .get(&id.to_string())
                            .cloned()
                            .unwrap_or_else(|| id.to_string());
                        if ui.button(format!("{} ×", name)).clicked() {
                            to_remove = Some(i);
                        }
                    }
                    if let Some(i) = to_remove {
                        let mut list = WARN_TAGS_INPUT.write().unwrap();
                        if i < list.len() {
                            list.remove(i);
                        }
                    }
                });

                ui.add_space(6.0);
                ui.label("Warn prefixes:");
                if let Some(id) = prefixes_picker(ui, "settings_warn_prefixes", "Select a prefix to warn...") {
                    let mut list = WARN_PREFIXES_INPUT.write().unwrap();
                    if !list.contains(&id) {
                        list.push(id);
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    let mut to_remove: Option<usize> = None;
                    let list_clone = { WARN_PREFIXES_INPUT.read().unwrap().clone() };
                    for (i, id) in list_clone.iter().enumerate() {
                        // Find prefix name by id
                        let mut name: Option<String> = None;
                        for group in &crate::tags::TAGS.prefixes.games {
                            if let Some(p) = group.prefixes.iter().find(|p| p.id as u32 == *id) {
                                name = Some(p.name.clone());
                                break;
                            }
                        }
                        let label = name.unwrap_or_else(|| id.to_string());
                        if ui.button(format!("{} ×", label)).clicked() {
                            to_remove = Some(i);
                        }
                    }
                    if let Some(i) = to_remove {
                        let mut list = WARN_PREFIXES_INPUT.write().unwrap();
                        if i < list.len() {
                            list.remove(i);
                        }
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Save").clicked() {
                        let temp_val = TEMP_DIR_INPUT.read().unwrap().clone();
                        let extract_val = EXTRACT_DIR_INPUT.read().unwrap().clone();
                        // Check if extract-dir changed and if there are installed games
                        let (old_extract, has_installed) = {
                            let st = APP_SETTINGS.read().unwrap();
                            (st.extract_dir.clone(), !st.downloaded_games.is_empty())
                        };
                        let new_extract_pb = std::path::PathBuf::from(extract_val.clone());
                        if has_installed && new_extract_pb != old_extract {
                            // Ask for confirmation and stash pending values
                            *PENDING_TEMP_DIR.write().unwrap() = temp_val.clone();
                            *PENDING_EXTRACT_DIR.write().unwrap() = extract_val.clone();
                            *PENDING_OLD_EXTRACT_DIR.write().unwrap() = old_extract.clone();
                            *MOVE_CONFIRM_OPEN.write().unwrap() = true;
                        } else {
                            // No installed games or path unchanged: apply immediately
                            {
                                let warn_tags = WARN_TAGS_INPUT.read().unwrap().clone();
                                let warn_prefixes = WARN_PREFIXES_INPUT.read().unwrap().clone();
                                let custom_launch = CUSTOM_LAUNCH_INPUT.read().unwrap().clone();
                                let cache_on_download = *CACHE_ON_DOWNLOAD_INPUT.read().unwrap();
                                let cache_dir_str = CACHE_DIR_INPUT.read().unwrap().clone();
                                let mut st = APP_SETTINGS.write().unwrap();
                                st.temp_dir = std::path::PathBuf::from(temp_val);
                                st.extract_dir = new_extract_pb;
                                st.warn_tags = warn_tags;
                                st.warn_prefixes = warn_prefixes;
                                st.custom_launch = custom_launch;
                                st.cache_on_download = cache_on_download;
                                st.cache_dir = std::path::PathBuf::from(cache_dir_str);
                            } // drop write lock before saving to avoid deadlock
                            save_settings_to_disk();
                            *SETTINGS_OPEN.write().unwrap() = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    if ctx.input(|i| i.viewport().close_requested()) {
                        *SETTINGS_OPEN.write().unwrap() = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    ui.add_space(8.0);

                    // Confirmation modal for moving installed games when extract-dir changes
                    if *MOVE_CONFIRM_OPEN.read().unwrap() {
                        egui::Window::new("Подтверждение изменения папки")
                            .collapsible(false)
                            .resizable(false)
                            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                            .show(ctx, |ui| {
                                ui.label("Обнаружены установленные игры. Все игры будут перенесены в новую папку. Продолжить?");
                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    if ui.button("Перенести").clicked() {
                                        let new_extract_str = PENDING_EXTRACT_DIR.read().unwrap().clone();
                                        let old_extract = PENDING_OLD_EXTRACT_DIR.read().unwrap().clone();
                                        let new_extract = std::path::PathBuf::from(&new_extract_str);

                                        // Clone entries to move without holding the lock during IO
                                        let entries: Vec<(u64, std::path::PathBuf, Option<std::path::PathBuf>)> = {
                                            let st = APP_SETTINGS.read().unwrap();
                                            st.downloaded_games
                                                .iter()
                                                .map(|e| (e.thread_id, e.folder.clone(), e.exe_path.clone()))
                                                .collect()
                                        };

                                        // Start background migration thread
                                        {
                                            let mut res = MOVE_RESULT.write().unwrap();
                                            *res = None;
                                        }
                                        {
                                            let mut err = MOVE_ERROR.write().unwrap();
                                            *err = None;
                                        }
                                        *MOVE_RUNNING.write().unwrap() = true;

                                        std::thread::spawn(move || {
                                            let moved = migrate::migrate_installed_games(&old_extract, &new_extract, entries);
                                            {
                                                let mut res = MOVE_RESULT.write().unwrap();
                                                *res = Some(moved);
                                            }
                                            *MOVE_RUNNING.write().unwrap() = false;
                                        });

                                        // Close confirmation modal, keep settings window open to show progress
                                        *MOVE_CONFIRM_OPEN.write().unwrap() = false;
                                    }
                                    if ui.button("Отмена").clicked() {
                                        *MOVE_CONFIRM_OPEN.write().unwrap() = false;
                                    }
                                });
                            });
                    }
                });
            });

            // Show progress / completion overlay while moving games
            if *MOVE_RUNNING.read().unwrap() {
                egui::Window::new("Перенос установленных игр")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label("Переносим игры… Не закрывайте приложение.");
                    });
                ctx.request_repaint(); // keep UI responsive during background work
            } else {
                // Apply settings once migration completed
                let mut maybe_res = MOVE_RESULT.write().unwrap();
                if let Some(moved) = maybe_res.take() {
                    let new_temp = PENDING_TEMP_DIR.read().unwrap().clone();
                    let new_extract_str = PENDING_EXTRACT_DIR.read().unwrap().clone();
                    let new_extract = std::path::PathBuf::from(&new_extract_str);
                    let warn_tags = WARN_TAGS_INPUT.read().unwrap().clone();
                    let warn_prefixes = WARN_PREFIXES_INPUT.read().unwrap().clone();
                    let custom_launch = CUSTOM_LAUNCH_INPUT.read().unwrap().clone();
                    let cache_on_download = *CACHE_ON_DOWNLOAD_INPUT.read().unwrap();
                    let cache_dir_str = CACHE_DIR_INPUT.read().unwrap().clone();
                    let cache_dir = std::path::PathBuf::from(&cache_dir_str);
                    {
                        let mut st = APP_SETTINGS.write().unwrap();
                        st.temp_dir = std::path::PathBuf::from(new_temp);
                        st.extract_dir = new_extract.clone();
                        st.warn_tags = warn_tags;
                        st.warn_prefixes = warn_prefixes;
                        st.custom_launch = custom_launch;
                        st.cache_on_download = cache_on_download;
                        st.cache_dir = cache_dir;
                        for (tid, nf, ne) in moved {
                            if let Some(entry) = st.downloaded_games.iter_mut().find(|e| e.thread_id == tid) {
                                entry.folder = nf;
                                if let Some(nep) = ne {
                                    entry.exe_path = Some(nep);
                                }
                            }
                        }
                    }
                    save_settings_to_disk();
                    *SETTINGS_OPEN.write().unwrap() = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        },
    );
}

pub struct SettingsApp {
    temp_dir: String,
    extract_dir: String,
    tx: mpsc::Sender<SettingsMsg>,
}

impl SettingsApp {
    pub fn new(tx: mpsc::Sender<SettingsMsg>, temp_dir: String, extract_dir: String) -> Self {
        Self { tx, temp_dir, extract_dir }
    }
}

impl App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Settings");
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Temp folder:");
                ui.text_edit_singleline(&mut self.temp_dir);
            });

            ui.horizontal(|ui| {
                ui.label("Extract-to folder:");
                ui.text_edit_singleline(&mut self.extract_dir);
            });

            ui.add_space(8.0);
            ui.separator();

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Save").clicked() {
                    let _ = self.tx.send(SettingsMsg::Update {
                        temp_dir: self.temp_dir.clone(),
                        extract_dir: self.extract_dir.clone(),
                    });
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }

                if ui.button("Cancel").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }

                ui.add_space(8.0);
            });
        });
    }
}
