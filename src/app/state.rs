// App state modules extracted from app.rs to reduce size and improve structure.

use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::time::Instant;

use crate::types::{DateLimit, SearchMode, Sorting, TagLogic};
use super::fetch::CoverMsg;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    AuthLogin,
    Main,
}

pub struct FiltersState {
    pub sort: Sorting,
    pub date_limit: DateLimit,
    pub include_logic: TagLogic,
    pub include_tags: Vec<u32>,
    pub exclude_tags: Vec<u32>,
    pub include_prefixes: Vec<u32>,
    pub exclude_prefixes: Vec<u32>,
    // Not used by logic, but part of the filters panel signature
    pub exclude_mode: Vec<u32>,
    pub search_mode: SearchMode,
    pub query: String,
    pub library_only: bool,
    pub last_library_only: bool,
    pub search_due_at: Option<Instant>,
}

impl Default for FiltersState {
    fn default() -> Self {
        let (mut inc, mut exc, mut pref, mut nopref) = super::settings::with_settings(|st| {
            (
                st.startup_tags.clone(),
                st.startup_exclude_tags.clone(),
                st.startup_prefixes.clone(),
                st.startup_exclude_prefixes.clone(),
            )
        });
        let max = crate::ui_constants::MAX_FILTER_ITEMS;
        if inc.len() > max { inc.truncate(max); }
        if exc.len() > max { exc.truncate(max); }
        if pref.len() > max { pref.truncate(max); }
        if nopref.len() > max { nopref.truncate(max); }

        Self {
            sort: Sorting::default(),
            date_limit: DateLimit::default(),
            include_logic: TagLogic::default(),
            include_tags: inc,
            exclude_tags: exc,
            include_prefixes: pref,
            exclude_prefixes: nopref,
            exclude_mode: Vec::new(),
            search_mode: SearchMode::default(),
            query: String::new(),
            library_only: false,
            last_library_only: false,
            search_due_at: None,
        }
    }
}

pub struct NetState {
    pub counter: u64,
    pub loading: bool,
    pub tx: mpsc::Sender<(u64, Result<crate::parser::F95Msg, crate::parser::F95Error>)>,
    pub rx: mpsc::Receiver<(u64, Result<crate::parser::F95Msg, crate::parser::F95Error>)>,
    pub last_result: Option<crate::parser::F95Msg>,
    pub last_error: Option<String>,
    pub library_req_ids: HashSet<u64>,
    pub lib_started: bool,
    pub lib_result: Option<crate::parser::F95Msg>,
    pub lib_error: Option<String>,
    pub lib_tx: mpsc::Sender<Result<crate::parser::F95Msg, crate::parser::F95Error>>,
    pub lib_rx: mpsc::Receiver<Result<crate::parser::F95Msg, crate::parser::F95Error>>,
}

impl NetState {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let (lib_tx, lib_rx) = mpsc::channel();
        Self {
            counter: 0,
            loading: false,
            tx,
            rx,
            last_result: None,
            last_error: None,
            library_req_ids: HashSet::new(),
            lib_started: false,
            lib_result: None,
            lib_error: None,
            lib_tx,
            lib_rx,
        }
    }
}

pub struct ImagesState {
    pub covers: HashMap<u64, egui::TextureHandle>,
    pub covers_loading: HashSet<u64>,
    pub screens: HashMap<u64, Vec<Option<egui::TextureHandle>>>,
    pub screens_loading: HashSet<(u64, usize)>,
    pub cover_tx: mpsc::Sender<CoverMsg>,
    pub cover_rx: mpsc::Receiver<CoverMsg>,
}

impl ImagesState {
    pub fn new() -> Self {
        let (cover_tx, cover_rx) = mpsc::channel();
        Self {
            covers: HashMap::new(),
            covers_loading: HashSet::new(),
            screens: HashMap::new(),
            screens_loading: HashSet::new(),
            cover_tx,
            cover_rx,
        }
    }
}

pub struct AuthState {
    pub screen: Screen,
    pub login_username: String,
    pub login_password: String,
    pub login_cookies_input: String,
    pub login_error: Option<String>,
    pub login_in_progress: bool,
    pub auth_tx: mpsc::Sender<Result<(), String>>,
    pub auth_rx: mpsc::Receiver<Result<(), String>>,
}

impl AuthState {
    pub fn new(screen: Screen) -> Self {
        let (auth_tx, auth_rx) = mpsc::channel();
        Self {
            screen,
            login_username: String::new(),
            login_password: String::new(),
            login_cookies_input: String::new(),
            login_error: None,
            login_in_progress: false,
            auth_tx,
            auth_rx,
        }
    }
}
