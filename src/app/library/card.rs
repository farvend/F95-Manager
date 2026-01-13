use url::Url;

#[derive(Debug, Clone)]
pub struct LibraryCard {
    pub thread_id: u64,
    pub title: String,
    pub creator: String,
    pub version: String,
    pub cover_url: Option<Url>,
    pub screen_urls: Vec<Url>,
    pub tags: Vec<u32>,
    pub prefixes: Vec<u32>,
}

impl LibraryCard {
    pub fn from_f95_thread(thread: &crate::parser::F95Thread) -> Option<Self> {
        let cover_url = if thread.cover.is_empty() {
            None
        } else {
            Url::parse(&crate::parser::normalize_url(&thread.cover)).ok()
        };

        let screen_urls: Vec<Url> = thread
            .screens
            .iter()
            .filter(|s| !s.is_empty())
            .filter_map(|s| Url::parse(&crate::parser::normalize_url(s)).ok())
            .collect();

        Some(Self {
            thread_id: thread.thread_id.get(),
            title: thread.title.clone(),
            creator: thread.creator.clone(),
            version: thread.version.clone(),
            cover_url,
            screen_urls,
            tags: thread.tags.clone(),
            prefixes: thread.prefixes.clone(),
        })
    }
}
