use std::sync::mpsc;
use std::path::PathBuf;

use crate::parser::{game_info::{F95Page, Platform, PlatformDownloads, ThreadId}, F95Thread};

#[derive(Debug, Clone)]
pub enum Progress {
    Pending(f32),
    Paused,
    Error(String),
    Unknown
}
pub enum GameDownloadStatus {
    Downloading(Progress),
    Unzipping(Progress),
    Completed { dest_dir: PathBuf, exe_path: Option<PathBuf> },
}

pub fn create_download_task(page: F95Page) -> mpsc::Receiver<GameDownloadStatus> {
    let rt = crate::app::RUNTIME.get().unwrap();
    
    // Создаем канал для передачи статусов загрузки
    let (tx, rx) = mpsc::channel();
    
    rt.spawn(async move {
        let downloads = match page.get_download_links().await {
            Ok(b) => b,
            Err(err) => {
                let msg = match err {
                    crate::parser::game_info::page::GetLinksError::BuildClient => "Failed to build HTTP client".to_string(),
                    crate::parser::game_info::page::GetLinksError::Request(e) => format!("Request error: {e}"),
                    crate::parser::game_info::page::GetLinksError::ReadText(e) => format!("Response read error: {e}"),
                    crate::parser::game_info::page::GetLinksError::NoDownloadsBlock => "Downloads block not found on page".to_string(),
                    crate::parser::game_info::page::GetLinksError::PlatformLineFormat => "Platform line parse error".to_string(),
                    crate::parser::game_info::page::GetLinksError::PlatformNameMissing => "Platform name missing".to_string(),
                    crate::parser::game_info::page::GetLinksError::NoPlatformLinks => "No platform links found".to_string(),
                };
                log::error!("err getting links: {msg}");
                let _ = tx.send(GameDownloadStatus::Downloading(Progress::Error(msg)));
                return;
            }
        };
        
        // Auto-select platform based on host OS; fall back to any available platform with links
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

        // Try preferred platform first, then common priority, then any with most links
        let selected = downloads
            .iter()
            .find(|e| e.platform().contains(preferred_platform));

        let links = match selected {
            Some(pd) if !pd.links().is_empty() => pd.links(),
            _ => {
                let message = format!("No suitable platform downloads found. Available: {:?}", downloads.iter().map(|e| e.platform()).collect::<Vec<_>>());
                let _ = tx.send(GameDownloadStatus::Downloading(Progress::Error(message)));
                return;
            }
        };
        
        let mut errors = vec![];
        for link in links {
            match link.download().await {
                Ok(mut download_recv) => {
                    while let Some(status) = download_recv.recv().await {
                        if tx.send(status).is_err() {
                            return; // Получатель отключился
                        }
                    }
                    return 
                },
                Err(err) => {
                    log::error!("Error downloading: {err:?}");
                    let err = format!("{err:?}");
                    errors.push(err);
                    //let _ = tx.send(GameDownloadStatus::Downloading(Progress::Error(err)));

                    // im lazyy
                    // struct Result {
                    //     should_return: bool,
                    //     error: String
                    // }
                    // use crate::parser::game_info::link::DownloadError;
                    // let result = match err {
                    //     DownloadError::Captcha => {
                    //         let error = ""
                    //     },
                    // }
                    // let _ = tx.send(GameDownloadStatus::Downloading(Progress::Error(err)));
                },
            }
        }
        
        // Если ни одна ссылка не сработала
        let error_text = if errors.len() == 0 {
            "For some reason no download links was found".to_string()
        } else {
            format!("Errors trying download from hostings: {errors:?}")
        };
        let _ = tx.send(GameDownloadStatus::Downloading(Progress::Error(error_text)));
    });
    
    rx
}
