use reqwest::Url;
use std::str::FromStr;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::{game_download::{GameDownloadStatus, Progress}, parser::{game_info::HostingSubset, CLIENT}};
use crate::app::settings::APP_SETTINGS;

use super::cookies;
use self::info::DirectRequest;

pub mod info;
pub mod gofile;
pub mod direct;
mod archive;
mod download;

// Futures-IO writer adapter for MEGA -> tokio::fs::File
use std::{pin::Pin, task::{Context, Poll}};
use futures::io as futures_io;
use futures_io::AsyncWrite as FuturesAsyncWrite;
use tokio::io::AsyncWrite as TokioAsyncWrite;
use tokio::sync::mpsc::UnboundedSender;

struct MegaFileWriter {
    file: tokio::fs::File,
    sd: UnboundedSender<GameDownloadStatus>,
    total: u64,
    written: u64,
}

impl MegaFileWriter {
    fn new(file: tokio::fs::File, sd: UnboundedSender<GameDownloadStatus>, total: u64) -> Self {
        Self { file, sd, total, written: 0 }
    }
}

impl FuturesAsyncWrite for MegaFileWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, futures_io::Error>> {
        // Safety: MegaFileWriter is pinned solely to protect inner `file` pin projection here.
        let this = unsafe { self.get_unchecked_mut() };
        let mut pinned = Pin::new(&mut this.file);
        match TokioAsyncWrite::poll_write(pinned.as_mut(), cx, buf) {
            Poll::Ready(Ok(n)) => {
                this.written += n as u64;
                if this.total > 0 {
                    let progress = (this.written as f32) / (this.total as f32);
                    let _ = this.sd.send(GameDownloadStatus::Downloading(Progress::Pending(progress)));
                }
                Poll::Ready(Ok(n))
            }
            Poll::Ready(Err(e)) => {
                Poll::Ready(Err(futures_io::Error::new(futures_io::ErrorKind::Other, e)))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), futures_io::Error>> {
        let this = unsafe { self.get_unchecked_mut() };
        let mut pinned = Pin::new(&mut this.file);
        match TokioAsyncWrite::poll_flush(pinned.as_mut(), cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(futures_io::Error::new(futures_io::ErrorKind::Other, e))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), futures_io::Error>> {
        let this = unsafe { self.get_unchecked_mut() };
        let mut pinned = Pin::new(&mut this.file);
        match TokioAsyncWrite::poll_shutdown(pinned.as_mut(), cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(futures_io::Error::new(futures_io::ErrorKind::Other, e))),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub use self::info::DownloadLinkInfo;
pub use self::direct::DirectDownloadLink;

#[derive(Debug, Clone)]
pub enum DownloadLink {
    Direct(DirectDownloadLink),
    Masked(Url),
}
impl DownloadLink {
    pub fn new(value: Url) -> Option<DownloadLink> {
        if let Some(mut path) = value.path_segments() {
            if path.next() == Some("masked") {
                Some(Self::Masked(value))
            } else {
                Some(Self::Direct(DirectDownloadLink::new(value)?))
            }
        } else {
            Some(Self::Direct(DirectDownloadLink::new(value)?))
        }
    }
}

#[derive(Debug)]
pub enum DownloadError {
    Network(reqwest::Error),
    NoRedirect,
    UnsupportedHosting,
    UnexpectedResponse,
    Captcha,
    ClientBuild(reqwest::Error),
    Request(reqwest::Error),
    MissingHeader(&'static str),
    Io(std::io::Error),
    StartTask,
}

#[derive(serde::Deserialize, Debug)]
struct MaskedRedirection {
    status: String,
    msg: String,
}

impl DownloadLink {
    async fn get_direct(self) -> Result<DirectDownloadLink, DownloadError> {
        match self {
            DownloadLink::Direct(link) => Ok(link),
            DownloadLink::Masked(link) => {
                //check is hosting valid
                {
                    let url_str = link
                        .path_segments()
                        .unwrap()
                        .nth(1)
                        .unwrap();
                    let url_str: String = "https://".to_string() + url_str;
                    let url: Url = url_str.as_str()
                        .try_into()
                        .map_err(|_| DownloadError::UnexpectedResponse)?;
                    let _: HostingSubset = url.try_into().map_err(|_| DownloadError::UnsupportedHosting)?;
                }
                


                let ans = CLIENT
                    .post(link.clone())
                    .header(
                        "Content-Type",
                        "application/x-www-form-urlencoded; charset=UTF-8",
                    )
                    .header("Cookie", cookies())
                    .body("xhr=1&download=1")
                    .send()
                    .await
                    .map_err(DownloadError::Network)?;

                let text = ans.text().await.map_err(DownloadError::Network)?;
                let resp: MaskedRedirection =
                    serde_json::from_str(&text).map_err(|_| DownloadError::UnexpectedResponse)?;

                if resp.status == "captcha" {
                    log::warn!("Pass the captcha on {}", link);
                    return Err(DownloadError::Captcha);
                }
                let url = Url::from_str(&resp.msg).map_err(|_| DownloadError::UnexpectedResponse)?;
                DirectDownloadLink::new(url).ok_or(DownloadError::UnsupportedHosting)
            }
        }
    }

    pub async fn download(&self) -> Result<UnboundedReceiver<GameDownloadStatus>, DownloadError> {
        let (sd, rc) = unbounded_channel();

        // Resolve direct request (either direct HTTP or MEGA public URL)
        let direct_req = {
            let direct = self.clone().get_direct().await?;
            direct.clone().get().await.ok_or(DownloadError::UnexpectedResponse)?
        };

        // Fire request / or branch for MEGA
        let client = reqwest::Client::builder()
            .build()
            .map_err(DownloadError::ClientBuild)?;
        let resp = match direct_req {
            DirectRequest::Http(request) => {
                client.execute(request).await.map_err(DownloadError::Request)?
            }
            DirectRequest::MegaPublicUrl(url) => {
                // MEGA public link handling: fetch nodes and download via mega::Client to disk.
                log::info!("downloading from {}", url.as_str());

                // 1) Init MEGA client over reqwest
                let http_client = reqwest::Client::new();
                let mut mega_client = match mega::ClientBuilder::new().https(true).build(http_client) {
                    Ok(c) => c,
                    Err(e) => {
                        log::error!("mega client build error: {:?}", e);
                        return Err(DownloadError::UnexpectedResponse);
                    }
                };

                // 2) Resolve public nodes
                let nodes = match mega_client.fetch_public_nodes(url.as_str()).await {
                    Ok(n) => n,
                    Err(e) => {
                        log::error!("mega fetch_public_nodes error: {:?}, tried to fetch: {url}", e);
                        return Err(DownloadError::UnexpectedResponse);
                    }
                };

                // 3) Pick first file node
                let file_node = match nodes.iter().find(|n| n.kind().is_file()) {
                    Some(n) => n,
                    None => {
                        log::warn!("no file node found in MEGA link");
                        return Err(DownloadError::UnexpectedResponse);
                    }
                };

                // 4) Prepare output file path using node name
                let filename = file_node.name().to_string();
                let download_dir = {
                    let s = APP_SETTINGS.read().unwrap();
                    s.temp_dir.clone()
                };
                tokio::fs::create_dir_all(&download_dir)
                    .await
                    .map_err(DownloadError::Io)?;
                let filepath = download_dir.join(filename);
                let file = tokio::fs::File::create(&filepath).await.map_err(DownloadError::Io)?;

                // 5) Start MEGA download into writer that updates progress
                let writer = MegaFileWriter::new(file, sd.clone(), file_node.size());
                if let Err(e) = mega_client.download_node(file_node, writer).await {
                    log::error!("mega download_node error: {:?}", e);
                    return Err(DownloadError::UnexpectedResponse);
                }

                // 6) Run extraction pipeline (reuse logic like in start_download_task)
                // Notify that extraction started
                let _ = sd.send(GameDownloadStatus::Unzipping(Progress::Pending(0.0)));

                let archive_path = filepath.clone();
                let sd_unzip = sd.clone();
                let dest_base = {
                    let s = APP_SETTINGS.read().unwrap();
                    s.extract_dir.clone()
                };

                // Run potentially heavy extraction on a blocking thread
                let path = archive_path.clone();
                let unzip_res =
                    tokio::task::spawn_blocking(move || self::archive::extract_archive(&path, &dest_base, &sd_unzip)).await;

                match unzip_res {
                    Ok(Ok((dest_dir, exe_path))) => {
                        // Delete the original archive after successful extraction
                        if let Err(e) = std::fs::remove_file(&archive_path) {
                            log::warn!("Failed to delete archive {}: {}", archive_path.display(), e);
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

                // MEGA path completes here.
                return Ok(rc);
            }
        };

        let filename_fallback = resp.url().path().split('/').last();

        // Extract filename
        let header= resp
            .headers()
            .get("content-disposition")
            .map(|e| e.to_str().unwrap())
            .or(filename_fallback)
            .ok_or(DownloadError::MissingHeader("content-disposition"))?;
        let filename = header
            .replace('"', "")
            .replace('\\', "")
            .replace('/', "")
            .replace(':', "")
            .replace('*', "")
            .replace('?', "")
            .replace('<', "")
            .replace('>', "",)
            .replace('|', "")
            .split('=')
            .nth(1)
            .or(filename_fallback)
            .ok_or(DownloadError::UnexpectedResponse)?
            .to_owned();

        // Prepare file (use user-configured temp dir)
        let download_dir = {
            let s = APP_SETTINGS.read().unwrap();
            s.temp_dir.clone()
        };
        tokio::fs::create_dir_all(&download_dir)
            .await
            .map_err(DownloadError::Io)?;
        let filepath = download_dir.join(filename);
        let file = tokio::fs::File::create(&filepath)
            .await
            .map_err(DownloadError::Io)?;

        // Start streaming to disk
        log::info!("downloading from {}", resp.url().as_str());
        if !download::start_download_task(resp, sd, file, filepath.clone()).await {
            return Err(DownloadError::StartTask);
        }

        Ok(rc)
    }
}