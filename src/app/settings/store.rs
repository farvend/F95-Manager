// Settings store: data types, global state, load/save, and records of downloaded games.

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize, Deserializer, Serializer};
use std::path::PathBuf;
use std::sync::RwLock;

fn default_cache_dir() -> PathBuf {
    PathBuf::from("cache")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadedGame {
    pub thread_id: u64,
    pub folder: PathBuf,
    pub exe_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum LoadingAnim {
    #[serde(rename = "bottom_bar")]
    #[default]
    BottomBar,
    #[serde(rename = "circle_bottom_right")]
    CircleBottomRight,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub temp_dir: PathBuf,
    pub extract_dir: PathBuf,
    #[serde(default = "default_cache_dir")]
    pub cache_dir: PathBuf,
    #[serde(default)]
    pub downloaded_games: Vec<DownloadedGame>,
    #[serde(default)]
    pub pending_downloads: Vec<u64>,
    #[serde(default)]
    pub hidden_threads: Vec<u64>,
    // Tags to auto-include in filters at startup
    #[serde(default)]
    pub startup_tags: Vec<u32>,
    // Tags to auto-exclude at startup
    #[serde(default)]
    pub startup_exclude_tags: Vec<u32>,
    // Prefixes to include at startup
    #[serde(default)]
    pub startup_prefixes: Vec<u32>,
    // Prefixes to exclude at startup
    #[serde(default)]
    pub startup_exclude_prefixes: Vec<u32>,
    // IDs of tags/prefixes that should trigger a warning badge on cards
    #[serde(default)]
    pub warn_tags: Vec<u32>,
    #[serde(default)]
    pub warn_prefixes: Vec<u32>,
    // Custom launch command template; use {{path}} placeholder for the game's exe path
    #[serde(default)]
    pub custom_launch: String,
    // Cache metadata/images on download click (default: false)
    #[serde(default)]
    pub cache_on_download: bool,
    // Loading indicator animation type
    #[serde(default)]
    pub loading_anim: LoadingAnim,
    // UI language (None = auto/system). Stored as "en"/"ru" or null; legacy "auto" maps to null.
    #[serde(
        default,
        serialize_with = "serialize_language_opt",
        deserialize_with = "deserialize_language_opt"
    )]
    pub language: Option<crate::localization::SupportedLang>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            temp_dir: PathBuf::from("downloads"),
            extract_dir: PathBuf::from("games"),
            cache_dir: PathBuf::from("cache"),
            downloaded_games: Vec::new(),
            pending_downloads: Vec::new(),
            hidden_threads: Vec::new(),
            startup_tags: Vec::new(),
            startup_exclude_tags: Vec::new(),
            startup_prefixes: Vec::new(),
            startup_exclude_prefixes: Vec::new(),
            warn_tags: Vec::new(),
            warn_prefixes: Vec::new(),
            custom_launch: String::new(),
            cache_on_download: false,
            loading_anim: LoadingAnim::BottomBar,
            language: None,
        }
    }
}

//// Serde helpers for language field to keep backward compatibility with older JSONs.
fn deserialize_language_opt<'de, D>(deserializer: D) -> Result<Option<crate::localization::SupportedLang>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    Ok(match opt {
        Some(s) => {
            let s = s.to_ascii_lowercase();
            match s.as_str() {
                "en" => Some(crate::localization::SupportedLang::English),
                "ru" => Some(crate::localization::SupportedLang::Russian),
                // treat "auto" or any unknown as None
                _ => None,
            }
        }
        None => None,
    })
}

fn serialize_language_opt<S>(
    value: &Option<crate::localization::SupportedLang>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match value {
        Some(crate::localization::SupportedLang::English) => serializer.serialize_some("en"),
        Some(crate::localization::SupportedLang::Russian) => serializer.serialize_some("ru"),
        None => serializer.serialize_none(),
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
    // Try delete from disk, but only if the path is inside the configured extract_dir.
    if let Some(folder) = downloaded_game_folder(thread_id) {
        let extract_dir = { APP_SETTINGS.read().unwrap().extract_dir.clone() };

        // Resolve canonical extract_dir first
        match std::fs::canonicalize(&extract_dir) {
            Ok(extract_root) => {
                // Resolve the target folder to a canonical path if it exists.
                // Fallback: if canonicalizing the stored path fails, try resolving it relative to extract_root.
                let target_canon = std::fs::canonicalize(&folder).or_else(|_| {
                    let candidate = if folder.is_absolute() {
                        folder.clone()
                    } else {
                        extract_root.join(&folder)
                    };
                    std::fs::canonicalize(&candidate)
                });

                if let Ok(target) = target_canon {
                    // Prevent deleting the extract_dir itself and ensure target is strictly within extract_dir.
                    if target != extract_root && target.strip_prefix(&extract_root).is_ok() {
                        match std::fs::remove_dir_all(&target) {
                            Ok(_) => log::info!("Deleted game folder: {}", target.to_string_lossy()),
                            Err(e) => log::error!(
                                "Failed to delete game folder {}: {}",
                                target.to_string_lossy(),
                                e
                            ),
                        }
                    } else {
                        log::warn!(
                            "Refusing to delete outside extract_dir. folder={}, extract_dir={}",
                            folder.to_string_lossy(),
                            extract_root.to_string_lossy()
                        );
                    }
                } else {
                    log::warn!(
                        "Game folder not found or cannot resolve for deletion: {}",
                        folder.to_string_lossy()
                    );
                }
            }
            Err(e) => {
                log::warn!(
                    "Cannot resolve extract_dir ({}). Skipping deletion: {}",
                    extract_dir.to_string_lossy(),
                    e
                );
            }
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
