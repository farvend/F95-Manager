use std::path::PathBuf;
use std::sync::mpsc;

use crate::parser::game_info::link::DownloadLink;
use crate::parser::game_info::{F95PageUrl, Platform};

#[derive(Debug, Clone)]
pub enum Progress {
    Pending(f32),
    Paused,
    Error(String),
    Unknown,
}
pub enum GameDownloadStatus {
    Downloading(Progress),
    // Signal UI to select a link (no platform parsed)
    SelectLinks(Vec<DownloadLink>),
    Unzipping(Progress),
    Completed {
        dest_dir: PathBuf,
        exe_path: Option<PathBuf>,
    },
}

pub fn create_download_task(page: F95PageUrl) -> mpsc::Receiver<GameDownloadStatus> {
    let rt = crate::app::RUNTIME.get().unwrap();

    // Создаем канал для передачи статусов загрузки
    let (tx, rx) = mpsc::channel();

    rt.spawn(async move {
        let downloads = match page.get_page().await {
            Ok(b) => match b.get_download_links() {
                Ok(links) => links,
                Err(err) => {
                    log::error!("err getting links: {err}");
                    match b.save_failed_parse_html(&page, &err).await {
                        Ok(path) => {
                            log::warn!("Saved failed parser HTML to {}", path.to_string_lossy());
                        }
                        Err(save_err) => {
                            log::error!("Failed to save parser HTML: {save_err}");
                        }
                    }
                    let _ = tx.send(GameDownloadStatus::Downloading(Progress::Error(
                        err.to_string(),
                    )));
                    return;
                }
            },
            Err(err) => {
                log::error!("err getting links: {err}");
                let _ = tx.send(GameDownloadStatus::Downloading(Progress::Error(
                    err.to_string(),
                )));
                return;
            }
        };
        // Pick links for the current OS automatically. Manual choice is only a
        // fallback when the parser could not classify a suitable platform.
        let preferred_platform = if cfg!(target_os = "windows") {
            Platform::WINDOWS
        } else if cfg!(target_os = "linux") {
            Platform::LINUX
        } else if cfg!(target_os = "macos") {
            Platform::MAC
        } else if cfg!(target_os = "android") {
            Platform::ANDROID
        } else {
            Platform::WINDOWS
        };

        let selected = downloads
            .iter()
            .find(|e| e.platform().contains(preferred_platform));

        let links = match selected {
            Some(pd) if !pd.links().is_empty() => pd.links(),
            _ => {
                let fallback_links = downloads
                    .iter()
                    .flat_map(|downloads| downloads.links().iter().cloned())
                    .collect::<Vec<_>>();
                if fallback_links.is_empty() {
                    let message = format!(
                        "No suitable platform downloads found. Available: {:?}",
                        downloads.iter().map(|e| e.platform()).collect::<Vec<_>>()
                    );
                    let _ = tx.send(GameDownloadStatus::Downloading(Progress::Error(message)));
                } else {
                    let _ = tx.send(GameDownloadStatus::SelectLinks(fallback_links));
                }
                return;
            }
        };

        let mut errors = Vec::new();
        for link in links {
            match link.download().await {
                Ok(mut download_recv) => {
                    while let Some(status) = download_recv.recv().await {
                        if tx.send(status).is_err() {
                            return;
                        }
                    }
                    return;
                }
                Err(error) => {
                    log::error!("Error downloading: {error:?}");
                    errors.push(format!("{error:?}"));
                }
            }
        }

        let error = if errors.is_empty() {
            "For some reason no download links was found".to_string()
        } else {
            format!("Errors trying download from hostings: {errors:?}")
        };
        let _ = tx.send(GameDownloadStatus::Downloading(Progress::Error(error)));
    });

    rx
}

pub fn create_download_from_link(link: DownloadLink) -> mpsc::Receiver<GameDownloadStatus> {
    let rt = crate::app::RUNTIME.get().unwrap();
    let (tx, rx) = mpsc::channel();

    rt.spawn(async move {
        match link.download().await {
            Ok(mut download_recv) => {
                while let Some(status) = download_recv.recv().await {
                    if tx.send(status).is_err() {
                        return; // receiver dropped
                    }
                }
            }
            Err(err) => {
                let _ = tx.send(GameDownloadStatus::Downloading(Progress::Error(format!(
                    "{err:?}"
                ))));
            }
        }
    });

    rx
}
