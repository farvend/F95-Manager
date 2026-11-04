use std::{fs, path::PathBuf};

use reqwest::Response;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::UnboundedSender;

use crate::app::settings::APP_SETTINGS;
use crate::game_download::{GameDownloadStatus, Progress};

use super::archive::extract_archive;

/// Stream `resp` into `file` in the background, then extract it.
///
/// Without `Content-Length` the download still runs, reported as
/// `Progress::Unknown` instead of a percentage.
pub fn start_download_task(
    mut resp: Response,
    sd: UnboundedSender<GameDownloadStatus>,
    mut file: tokio::fs::File,
    filepath: PathBuf,
) {
    let total_size = resp.content_length().filter(|size| *size > 0);
    let _ = sd.send(GameDownloadStatus::Downloading(match total_size {
        Some(_) => Progress::Pending(0.0),
        None => Progress::Unknown,
    }));

    tokio::spawn(async move {
        let mut downloaded = 0u64;
        loop {
            match resp.chunk().await {
                Ok(Some(bytes)) => {
                    if let Err(e) = file.write_all(&bytes).await {
                        log::info!("write error: {:?}", e);
                        let _ = sd.send(GameDownloadStatus::Downloading(Progress::Error(
                            "Couldn't write data to disk".to_string(),
                        )));
                        return;
                    }
                    downloaded += bytes.len() as u64;
                    if let Some(total_size) = total_size {
                        let progress = (downloaded as f32) / (total_size as f32);
                        let _ =
                            sd.send(GameDownloadStatus::Downloading(Progress::Pending(progress)));
                    }
                }
                Ok(None) => {
                    log::info!("download completed");
                    if let Err(e) = file.sync_all().await {
                        log::warn!("sync_all failed: {:?}", e);
                    }
                    // Close the file handle before extraction
                    drop(file);
                    extract_and_finish(filepath, &sd).await;
                    return;
                }
                Err(e) => {
                    log::error!("read chunk error: {:?}", e);
                    let _ = sd.send(GameDownloadStatus::Downloading(Progress::Error(format!(
                        "Error reading chunk: {e}"
                    ))));
                    return;
                }
            }
        }
    });
}

/// Extract a fully downloaded archive into the games folder, delete it and
/// report `Completed` (or the extraction error).
pub async fn extract_and_finish(archive_path: PathBuf, sd: &UnboundedSender<GameDownloadStatus>) {
    let dest_base = APP_SETTINGS.read().unwrap().extract_dir.clone();
    let _ = sd.send(GameDownloadStatus::Unzipping(Progress::Pending(0.0)));

    // Extraction is heavy and synchronous: keep it off the async workers.
    let path = archive_path.clone();
    let sd_unzip = sd.clone();
    let unzip_res =
        tokio::task::spawn_blocking(move || extract_archive(&path, &dest_base, &sd_unzip)).await;

    match unzip_res {
        Ok(Ok((dest_dir, exe_path))) => {
            if let Err(e) = fs::remove_file(&archive_path) {
                log::warn!("Failed to delete archive {}: {}", archive_path.display(), e);
            }
            log::info!("successfully extracted");
            let _ = sd.send(GameDownloadStatus::Completed { dest_dir, exe_path });
        }
        Ok(Err(msg)) => {
            let _ = sd.send(GameDownloadStatus::Unzipping(Progress::Error(msg)));
        }
        Err(e) => {
            let _ = sd.send(GameDownloadStatus::Unzipping(Progress::Error(format!(
                "Unzip task join error: {e}"
            ))));
        }
    }
}
