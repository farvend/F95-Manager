use super::*;

#[derive(Clone)]
pub(super) struct CardRecord {
    pub(super) id: u64,
    pub(super) title: String,
    pub(super) creator: String,
    pub(super) version: String,
    pub(super) prefix: String,
    pub(super) date: String,
    pub(super) ts: u64,
    pub(super) likes: u64,
    pub(super) views: u64,
    pub(super) rating: f32,
    pub(super) cover_url: Option<String>,
    pub(super) screens: Vec<String>,
    pub(super) tags: Vec<u32>,
    pub(super) prefixes: Vec<u32>,
    pub(super) cached_cover: Option<PathBuf>,
    pub(super) installed: bool,
    pub(super) folder: Option<PathBuf>,
}

pub(super) struct ImagePixels {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) rgba: Vec<u8>,
}

pub(super) struct UiState {
    pub(super) cards: Vec<CardRecord>,
    pub(super) columns: usize,
    pub(super) page: u32,
    pub(super) total_pages: u32,
    pub(super) query: String,
    pub(super) library_mode: bool,
    pub(super) session_mode: bool,
    pub(super) launched_games: Vec<u64>,
    pub(super) selected_bookmarks: HashSet<String>,
    pub(super) include_tags: Vec<u32>,
    pub(super) include_logic: TagLogic,
    pub(super) exclude_tags: Vec<u32>,
    pub(super) prefixes: Vec<u32>,
    pub(super) exclude_prefixes: Vec<u32>,
    pub(super) search_mode: SearchMode,
    pub(super) unplayed_only: bool,
    pub(super) sorting: Sorting,
    pub(super) date_limit: DateLimit,
    pub(super) loaded_images: HashSet<u64>,
    pub(super) loading_images: HashSet<u64>,
    pub(super) loaded_screen_images: HashSet<(u64, usize)>,
    pub(super) loading_screens: HashSet<u64>,
    pub(super) loaded_screens: HashSet<u64>,
    pub(super) downloads: HashMap<u64, DownloadJob>,
    pub(super) request_generation: u64,
    pub(super) library_refresh_scheduled: bool,
}

pub(super) struct DownloadJob {
    pub(super) progress: Progress,
    pub(super) link_choices: Vec<DownloadLink>,
}

impl UiState {
    pub(super) fn track_launch(&mut self, id: u64) -> bool {
        if self.launched_games.contains(&id) {
            false
        } else {
            self.launched_games.push(id);
            true
        }
    }

    pub(super) fn rollback_launch(&mut self, id: u64) {
        self.launched_games.retain(|launched_id| *launched_id != id);
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            cards: Vec::new(),
            columns: 3,
            page: 1,
            total_pages: 1,
            query: String::new(),
            library_mode: false,
            session_mode: false,
            launched_games: Vec::new(),
            selected_bookmarks: HashSet::new(),
            include_tags: Vec::new(),
            include_logic: TagLogic::Or,
            exclude_tags: Vec::new(),
            prefixes: Vec::new(),
            exclude_prefixes: Vec::new(),
            search_mode: SearchMode::Title,
            unplayed_only: false,
            sorting: Sorting::Date,
            date_limit: DateLimit::Anytime,
            loaded_images: HashSet::new(),
            loading_images: HashSet::new(),
            loaded_screen_images: HashSet::new(),
            loading_screens: HashSet::new(),
            loaded_screens: HashSet::new(),
            downloads: HashMap::new(),
            request_generation: 0,
            library_refresh_scheduled: false,
        }
    }
}

pub(super) type SharedState = Arc<Mutex<UiState>>;

#[derive(Default)]
pub(super) struct UiImageCache {
    pub(super) covers: HashMap<u64, Image>,
    pub(super) screens: HashMap<(u64, usize), Image>,
    pub(super) screen_games: VecDeque<u64>,
}

thread_local! {
    pub(super) static UI_IMAGE_CACHE: RefCell<UiImageCache> = RefCell::new(UiImageCache::default());
}

#[derive(Default)]
pub(super) struct SettingsFilterState {
    pub(super) values: [Vec<u32>; 6],
}

impl SettingsFilterState {
    pub(super) fn from_settings(settings: &crate::app::settings::AppSettings) -> Self {
        Self {
            values: [
                settings.startup_tags.clone(),
                settings.startup_exclude_tags.clone(),
                settings.startup_prefixes.clone(),
                settings.startup_exclude_prefixes.clone(),
                settings.warn_tags.clone(),
                settings.warn_prefixes.clone(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UiState;

    #[test]
    fn session_accumulates_unique_launched_games() {
        let mut state = UiState::default();

        assert!(state.track_launch(101));
        assert!(state.track_launch(202));
        assert!(!state.track_launch(101));

        assert_eq!(state.launched_games, vec![101, 202]);
    }

    #[test]
    fn failed_launch_removes_only_its_game() {
        let mut state = UiState::default();
        state.track_launch(101);
        state.track_launch(202);

        state.rollback_launch(202);

        assert_eq!(state.launched_games, vec![101]);
    }
}
