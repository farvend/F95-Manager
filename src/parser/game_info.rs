// Split game_info into submodules

pub fn cookies() -> String {
    let cfg = crate::app::config::APP_CONFIG.read().unwrap();
    if let Some(c) = cfg.cookies.as_ref() {
        let trimmed = c.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    String::new()
}

pub mod hosting;
pub mod link;
pub mod page;
pub mod thread_meta;
pub mod types;

// Re-exports to keep external API unchanged
pub use hosting::{Hosting, HostingSubset};
pub use link::{DirectDownloadLink, DownloadLink, DownloadLinkInfo};
pub use page::{F95PageUrl, GetLinksError};
pub use types::{Platform, PlatformDownloads, ThreadId};
