use std::{fs, path::PathBuf};

use reqwest::Response;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::UnboundedSender;

use crate::app::settings::APP_SETTINGS;
use crate::game_download::{GameDownloadStatus, Progress};

use super::archive::extract_archive;

pub async fn start_download_task(
    mut resp: Response,
    sd: UnboundedSender<GameDownloadStatus>,
    mut file: tokio::fs::File,
    filepath: PathBuf,
) -> bool {
    let total_size = match resp.content_length() {
        Some(sz) => sz,
        None => {
            log::warn!("server didn't send content length");
            let _ = sd.send(GameDownloadStatus::Downloading(Progress::Error(
                "Server didn't send content length".to_string(),
            )));
            return false;
        }
    };

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
                        break;
                    }
                    downloaded += bytes.len() as u64;
                    let progress = (downloaded as f32) / (total_size as f32);
                    let _ = sd.send(GameDownloadStatus::Downloading(Progress::Pending(progress)));
                }
                Ok(None) => {
                    log::info!("download completed");
                    if let Err(e) = file.sync_all().await {
                        log::warn!("sync_all failed: {:?}", e);
                    }
                    // Close the file handle before extraction
                    drop(file);

                    let archive_path = filepath.clone();
                    let sd_unzip = sd.clone();
                    let dest_base = {
                        let s = APP_SETTINGS.read().unwrap();
                        s.extract_dir.clone()
                    };

                    // Notify that extraction started
                    let _ = sd.send(GameDownloadStatus::Unzipping(Progress::Pending(0.0)));

                    // Run potentially heavy extraction on a blocking thread
                    let path = archive_path.clone();
                    let unzip_res = tokio::task::spawn_blocking(move || {
                        extract_archive(&path, &dest_base, &sd_unzip)
                    })
                    .await;

                    match unzip_res {
                        Ok(Ok((dest_dir, exe_path))) => {
                            // Delete the original archive after successful extraction
                            if let Err(e) = fs::remove_file(&archive_path) {
                                log::warn!(
                                    "Failed to delete archive {}: {}",
                                    archive_path.display(),
                                    e
                                );
                            }
                            let _ = sd.send(GameDownloadStatus::Completed { dest_dir, exe_path });
                        }
                        Ok(Err(msg)) => {
                            let _ = sd.send(GameDownloadStatus::Unzipping(Progress::Error(msg)));
                        }
                        Err(e) => {
                            let _ = sd.send(GameDownloadStatus::Unzipping(Progress::Error(
                                format!("Unzip task join error: {e}"),
                            )));
                        }
                    }
                    log::info!("successfully extracted");
                    break;
                }
                Err(e) => {
                    log::error!("read chunk error: {:?}", e);
                    let _ = sd.send(GameDownloadStatus::Downloading(Progress::Error(format!(
                        "Error reading chunk: {e}"
                    ))));
                    break;
                }
            }
        }
    });

    tokio::task::yield_now().await;

    true
}
