// Settings store: data types, global state, load/save, and records of downloaded games.

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadedGame {
    pub thread_id: u64,
    pub folder: PathBuf,
    pub exe_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub temp_dir: PathBuf,
    pub extract_dir: PathBuf,
    #[serde(default)]
    pub downloaded_games: Vec<DownloadedGame>,
    #[serde(default)]
    pub pending_downloads: Vec<u64>,
    #[serde(default)]
    pub hidden_threads: Vec<u64>,
    // IDs of tags/prefixes that should trigger a warning badge on cards
    #[serde(default)]
    pub warn_tags: Vec<u32>,
    #[serde(default)]
    pub warn_prefixes: Vec<u32>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            temp_dir: PathBuf::from("downloads"),
            extract_dir: PathBuf::from("games"),
            downloaded_games: Vec::new(),
            pending_downloads: Vec::new(),
            hidden_threads: Vec::new(),
            warn_tags: Vec::new(),
            warn_prefixes: Vec::new(),
        }
    }
}

lazy_static! {
    pub static ref APP_SETTINGS: RwLock<AppSettings> = RwLock::new(AppSettings::default());
}

fn settings_file_path() -> PathBuf {
    // Store settings in current working directory to avoid extra deps
    PathBuf::from("app_settings.json")
}

impl AppSettings {
    pub fn load_from_file(path: &std::path::Path) -> std::io::Result<Self> {
        let data = std::fs::read_to_string(path)?;
        let s: AppSettings = serde_json::from_str(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(s)
    }

    pub fn save_to_file(&self, path: &std::path::Path) -> std::io::Result<()> {
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, data)
    }
}

pub fn load_settings_from_disk() {
    let path = settings_file_path();
    match AppSettings::load_from_file(&path) {
        Ok(s) => {
            *APP_SETTINGS.write().unwrap() = s;
            log::info!("Loaded settings from {}", path.to_string_lossy());
        }
        Err(e) => {
            // Keep defaults if missing/unreadable
            log::info!(
                "Using default settings; cannot load {}: {}",
                path.to_string_lossy(),
                e
            );
        }
    }
}

pub fn save_settings_to_disk() {
    let path = settings_file_path();
    let st = APP_SETTINGS.read().unwrap().clone();
    if let Err(e) = st.save_to_file(&path) {
        log::error!(
            "Failed to save settings to {}: {}",
            path.to_string_lossy(),
            e
        );
    } else {
        log::info!("Saved settings to {}", path.to_string_lossy());
    }
}

// New helpers: persist IDs of pending/incomplete downloads
pub fn record_pending_download(thread_id: u64) {
    {
        let mut st = APP_SETTINGS.write().unwrap();
        if !st.pending_downloads.contains(&thread_id) {
            st.pending_downloads.push(thread_id);
        }
    }
    save_settings_to_disk();
}

pub fn remove_pending_download(thread_id: u64) {
    {
        let mut st = APP_SETTINGS.write().unwrap();
        let before = st.pending_downloads.len();
        st.pending_downloads.retain(|id| *id != thread_id);
        if st.pending_downloads.len() != before {
            log::info!("Removed pending download entry for thread {}", thread_id);
        }
    }
    save_settings_to_disk();
}

pub fn record_downloaded_game(thread_id: u64, folder: PathBuf, exe_path: Option<PathBuf>) {
    {
        let mut st = APP_SETTINGS.write().unwrap();
        if let Some(entry) = st.downloaded_games.iter_mut().find(|e| e.thread_id == thread_id) {
            entry.folder = folder.clone();
            entry.exe_path = exe_path.clone();
        } else {
            st.downloaded_games.push(DownloadedGame {
                thread_id,
                folder: folder.clone(),
                exe_path: exe_path.clone(),
            });
        }
        // Also clear any pending entry for this thread
        st.pending_downloads.retain(|id| *id != thread_id);
    }
    save_settings_to_disk();
}

// Mark a thread as hidden (adds its thread_id to settings and saves to disk)
pub fn hide_thread(thread_id: u64) {
    {
        let mut st = APP_SETTINGS.write().unwrap();
        if !st.hidden_threads.contains(&thread_id) {
            st.hidden_threads.push(thread_id);
        }
    }
    save_settings_to_disk();
}

// Check if a thread is hidden
pub fn is_thread_hidden(thread_id: u64) -> bool {
    let st = APP_SETTINGS.read().unwrap();
    st.hidden_threads.contains(&thread_id)
}

pub fn is_pending_download(thread_id: u64) -> bool {
    let st = APP_SETTINGS.read().unwrap();
    st.pending_downloads.contains(&thread_id)
}

// Return the folder of a downloaded game by thread_id, if present
pub fn downloaded_game_folder(thread_id: u64) -> Option<PathBuf> {
    let st = APP_SETTINGS.read().unwrap();
    st.downloaded_games
        .iter()
        .find(|e| e.thread_id == thread_id)
        .map(|e| e.folder.clone())
}

pub fn downloaded_game_exe(thread_id: u64) -> Option<PathBuf> {
    let st = APP_SETTINGS.read().unwrap();
    st.downloaded_games
        .iter()
        .find(|e| e.thread_id == thread_id)
        .and_then(|e| e.exe_path.clone())
}

// Remove downloaded game files and its record from settings
pub fn delete_downloaded_game(thread_id: u64) {
    // Try delete from disk
    if let Some(folder) = downloaded_game_folder(thread_id) {
        match std::fs::remove_dir_all(&folder) {
            Ok(_) => log::info!("Deleted game folder: {}", folder.to_string_lossy()),
            Err(e) => log::error!("Failed to delete game folder {}: {}", folder.to_string_lossy(), e),
        }
    }
    // Remove entry from settings
    {
        let mut st = APP_SETTINGS.write().unwrap();
        let before = st.downloaded_games.len();
        st.downloaded_games.retain(|e| e.thread_id != thread_id);
        if st.downloaded_games.len() != before {
            log::info!("Removed downloaded game entry for thread {}", thread_id);
        }
    }
    save_settings_to_disk();
}
