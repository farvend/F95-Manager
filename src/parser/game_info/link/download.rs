use std::{fs, path::PathBuf};

use reqwest::Response;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::UnboundedSender;

use crate::game_download::{GameDownloadStatus, Progress};

use super::archive::extract_archive;
use super::DownloadError;

async fn extract_and_report(
    archive_path: PathBuf,
    sd: UnboundedSender<GameDownloadStatus>,
) {
    // Notify that extraction started
    let _ = sd.send(GameDownloadStatus::Unzipping(Progress::Pending(0.0)));

    let sd_unzip = sd.clone();
    let dest_base = crate::app::settings::with_settings(|s| s.extract_dir.clone());

    // Run potentially heavy extraction on a blocking thread
    let archive_path_for_task = archive_path.clone();
    let unzip_res = tokio::task::spawn_blocking(move || {
        extract_archive(&archive_path_for_task, &dest_base, &sd_unzip)
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
            let _ = sd.send(GameDownloadStatus::Unzipping(Progress::Error(format!(
                "Unzip task join error: {e}"
            ))));
        }
    }
}

pub async fn start_download_task(
    mut resp: Response,
    sd: UnboundedSender<GameDownloadStatus>,
    mut file: tokio::fs::File,
    filepath: PathBuf,
) -> Result<(), DownloadError> {
    let total_size = match resp.content_length() {
        Some(sz) => sz,
        None => {
            log::warn!("server didn't send content length");
            let _ = sd.send(GameDownloadStatus::Downloading(Progress::Error(
                "Server didn't send content length".to_string(),
            )));
            return Err(DownloadError::MissingContentLength);
        }
    };

    tokio::spawn(async move {
        let mut downloaded = 0u64;
        // Throttle progress updates to reduce load on UI thread / channel allocations.
        // Report at most ~30 times per second or on every >=256 KiB increment.
        let mut last_reported = 0u64;
        let mut last_report_at = std::time::Instant::now();
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

                    let should_report = downloaded.saturating_sub(last_reported) >= 256 * 1024
                        || last_report_at.elapsed() >= std::time::Duration::from_millis(33)
                        || downloaded >= total_size;
                    if should_report {
                        last_reported = downloaded;
                        last_report_at = std::time::Instant::now();
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

                    extract_and_report(filepath.clone(), sd.clone()).await;
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

    Ok(())
}

/// Helper for non-HTTP download flows (e.g. MEGA) where the file is already on disk.
pub async fn extract_downloaded_archive(
    archive_path: PathBuf,
    sd: UnboundedSender<GameDownloadStatus>,
) {
    extract_and_report(archive_path, sd).await;
}
