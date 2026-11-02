// Split game_info into submodules

pub mod hosting;
pub mod link;
pub mod page;
pub mod thread_meta;
pub mod types;

// Re-exports to keep external API unchanged
pub use hosting::HostingSubset;
pub use link::DownloadLink;
pub use page::F95PageUrl;
pub use types::{Platform, PlatformDownloads, ThreadId};
