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

pub mod types;
pub mod hosting;
pub mod link;
pub mod page;
pub mod thread_meta;

// Re-exports to keep external API unchanged
pub use types::{ThreadId, Platform, PlatformDownloads};
pub use hosting::{Hosting, HostingSubset};
pub use page::{F95Page, GetLinksError};
pub use link::{DownloadLink, DirectDownloadLink, DownloadLinkInfo};
