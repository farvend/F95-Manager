use reqwest::Url;
use std::str::FromStr;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::app::settings::APP_SETTINGS;
use crate::{
    game_download::{GameDownloadStatus, Progress},
    parser::game_info::HostingSubset,
};

use self::info::DirectRequest;

mod archive;
pub mod direct;
mod download;
pub mod f95dl;
pub mod gofile;
pub mod info;

// Futures-IO writer adapter for MEGA -> tokio::fs::File
use futures::io as futures_io;
use futures_io::AsyncWrite as FuturesAsyncWrite;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
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
        Self {
            file,
            sd,
            total,
            written: 0,
        }
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
                    let _ = this
                        .sd
                        .send(GameDownloadStatus::Downloading(Progress::Pending(progress)));
                }
                Poll::Ready(Ok(n))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(futures_io::Error::other(e))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), futures_io::Error>> {
        let this = unsafe { self.get_unchecked_mut() };
        let mut pinned = Pin::new(&mut this.file);
        match TokioAsyncWrite::poll_flush(pinned.as_mut(), cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(futures_io::Error::other(e))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_close(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), futures_io::Error>> {
        let this = unsafe { self.get_unchecked_mut() };
        let mut pinned = Pin::new(&mut this.file);
        match TokioAsyncWrite::poll_shutdown(pinned.as_mut(), cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(futures_io::Error::other(e))),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub use self::direct::DirectDownloadLink;

#[derive(Debug, Clone)]
pub enum DownloadLink {
    Direct(DirectDownloadLink),
    Masked(Url),
    /// F95 download page (`/dl/<post>/?...`) that resolves to a hosting URL.
    F95Page(F95PageLink),
}

/// `/dl/` link together with the hosting name F95 shows on it.
///
/// The URL alone says nothing about the hosting (it always points to
/// f95zone.to), so the anchor text ("GOFILE", "MEGA", ...) is kept to tell the
/// user which mirror failed, and to skip mirrors the app cannot download
/// before asking the user to pass F95's check for them.
#[derive(Debug, Clone)]
pub struct F95PageLink {
    pub url: Url,
    pub host: String,
    pub hosting: Option<HostingSubset>,
}

impl DownloadLink {
    /// `label` is the anchor text from the post; it names the hosting for
    /// `/dl/` links, whose URL does not.
    pub fn new(value: Url, label: &str) -> Option<DownloadLink> {
        if value.host_str() == Some("f95zone.to") && is_download_page(&value) {
            return Some(Self::F95Page(F95PageLink {
                url: value,
                host: label.trim().to_string(),
                hosting: HostingSubset::from_label(label),
            }));
        }
        if let Some(mut segs) = value.path_segments() {
            if segs.next() == Some("masked") {
                // Validate masked target hosting is supported (e.g. skip workupload, mediafire, etc. if not in subset)
                if let Some(host) = segs.next() {
                    let host_url_str = format!("https://{host}");
                    if let Ok(host_url) = Url::from_str(&host_url_str) {
                        if HostingSubset::try_from(host_url).is_ok() {
                            return Some(Self::Masked(value));
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                }
                return None;
            } else {
                return Some(Self::Direct(DirectDownloadLink::new(value)?));
            }
        }
        Some(Self::Direct(DirectDownloadLink::new(value)?))
    }

    /// Hosting the link downloads from, for messages and the link picker.
    pub fn host_label(&self) -> String {
        match self {
            DownloadLink::Direct(link) => link.hosting.to_string(),
            DownloadLink::Masked(url) => match url.path_segments().and_then(|mut s| s.nth(1)) {
                Some(host) => host.to_string(),
                None => url.as_str().to_string(),
            },
            DownloadLink::F95Page(link) if link.host.is_empty() => link.url.path().to_string(),
            DownloadLink::F95Page(link) => link.host.clone(),
        }
    }
}

/// `/dl/<post>/` pages serve the hosting link behind F95's own check.
fn is_download_page(value: &Url) -> bool {
    match value.path_segments().and_then(|mut segments| segments.next()) {
        Some("dl") => true,
        Some(_) | None => false,
    }
}

#[derive(Debug)]
pub enum PixeldrainLinkError {
    MissingFileId,
    InvalidUrl(url::ParseError),
}

#[derive(Debug)]
pub enum CatboxLinkError {
    MissingFilePath,
    InvalidUrl(url::ParseError),
}

#[derive(Debug)]
pub enum MegaLinkError {
    MissingNodeId,
    MissingNodeKey,
    MalformedLegacyFragment,
    InvalidUrl(url::ParseError),
}

#[derive(Debug)]
pub enum DirectLinkError {
    Pixeldrain(PixeldrainLinkError),
    Gofile(gofile::GofileLinkError),
    Catbox(CatboxLinkError),
    Mega(MegaLinkError),
}

#[derive(Debug)]
pub enum DownloadError {
    Config(crate::net::NetError),
    Network(reqwest::Error),
    NoRedirect,
    DirectLinkFailed(DirectLinkError),
    UnsupportedHosting,
    Captcha,
    ClientBuild(reqwest::Error),
    Request(reqwest::Error),
    MissingHeader(&'static str),
    Io(std::io::Error),
    InvalidUrl,
    InvalidRedirectUrl,
    JsonParse,
    MegaClientBuild,
    MegaFetchNodes,
    MegaNoFileNode,
    MegaDownload,
    FilenameParse,
    F95Page(self::f95dl::F95DlError),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::Config(error) => write!(f, "{error}"),
            DownloadError::Network(error) | DownloadError::Request(error) => {
                write!(f, "request error: {error}")
            }
            DownloadError::NoRedirect => write!(f, "hosting did not return a redirect"),
            DownloadError::DirectLinkFailed(DirectLinkError::Gofile(error)) => write!(f, "{error}"),
            DownloadError::DirectLinkFailed(error) => {
                write!(f, "failed to resolve the direct link: {error:?}")
            }
            DownloadError::UnsupportedHosting => write!(f, "download host is not supported"),
            DownloadError::Captcha => write!(f, "hosting requires a captcha"),
            DownloadError::ClientBuild(error) => write!(f, "HTTP client build failed: {error}"),
            DownloadError::MissingHeader(name) => {
                write!(f, "download response is missing the {name} header")
            }
            DownloadError::Io(error) => write!(f, "I/O error: {error}"),
            DownloadError::InvalidUrl => write!(f, "invalid download URL"),
            DownloadError::InvalidRedirectUrl => write!(f, "invalid redirect URL"),
            DownloadError::JsonParse => write!(f, "could not parse the hosting response"),
            DownloadError::MegaClientBuild => write!(f, "MEGA client build failed"),
            DownloadError::MegaFetchNodes => write!(f, "MEGA node lookup failed"),
            DownloadError::MegaNoFileNode => write!(f, "MEGA link has no file node"),
            DownloadError::MegaDownload => write!(f, "MEGA download failed"),
            DownloadError::FilenameParse => write!(f, "could not determine the archive filename"),
            DownloadError::F95Page(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for DownloadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DownloadError::Config(error) => Some(error),
            DownloadError::Network(error)
            | DownloadError::Request(error)
            | DownloadError::ClientBuild(error) => Some(error),
            DownloadError::Io(error) => Some(error),
            DownloadError::F95Page(error) => Some(error),
            DownloadError::NoRedirect
            | DownloadError::DirectLinkFailed(_)
            | DownloadError::UnsupportedHosting
            | DownloadError::Captcha
            | DownloadError::MissingHeader(_)
            | DownloadError::InvalidUrl
            | DownloadError::InvalidRedirectUrl
            | DownloadError::JsonParse
            | DownloadError::MegaClientBuild
            | DownloadError::MegaFetchNodes
            | DownloadError::MegaNoFileNode
            | DownloadError::MegaDownload
            | DownloadError::FilenameParse => None,
        }
    }
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
            DownloadLink::F95Page(link) => {
                // Mirrors the app cannot download are rejected up front, so the
                // user is never asked to pass F95's check for them.
                if link.hosting.is_none() {
                    return Err(DownloadError::UnsupportedHosting);
                }
                let host_url = self::f95dl::resolve_host_url(&link.url)
                    .await
                    .map_err(DownloadError::F95Page)?;
                log::info!("resolved {} ({}) to {}", link.url.as_str(), link.host, host_url.as_str());
                DirectDownloadLink::new(host_url).ok_or(DownloadError::UnsupportedHosting)
            }
            DownloadLink::Masked(link) => {
                //check is hosting valid
                {
                    let url_str = link.path_segments().unwrap().nth(1).unwrap();
                    let url_str: String = "https://".to_string() + url_str;
                    let url: Url = url_str
                        .as_str()
                        .try_into()
                        .map_err(|_| DownloadError::InvalidUrl)?;
                    let _: HostingSubset = url
                        .try_into()
                        .map_err(|_| DownloadError::UnsupportedHosting)?;
                }

                let client = crate::net::client().map_err(DownloadError::Config)?;
                let ans = client
                    .post(link.clone())
                    .header(
                        "Content-Type",
                        "application/x-www-form-urlencoded; charset=UTF-8",
                    )
                    .body("xhr=1&download=1")
                    .send()
                    .await
                    .map_err(DownloadError::Network)?;

                let text = ans.text().await.map_err(DownloadError::Network)?;
                let resp: MaskedRedirection =
                    serde_json::from_str(&text).map_err(|_| DownloadError::JsonParse)?;

                if resp.status == "captcha" {
                    log::warn!("Pass the captcha on {}", link);
                    return Err(DownloadError::Captcha);
                }
                let url =
                    Url::from_str(&resp.msg).map_err(|_| DownloadError::InvalidRedirectUrl)?;
                DirectDownloadLink::new(url).ok_or(DownloadError::UnsupportedHosting)
            }
        }
    }

    pub async fn download(&self) -> Result<UnboundedReceiver<GameDownloadStatus>, DownloadError> {
        let (sd, rc) = unbounded_channel();

        // Resolve direct request (either direct HTTP or MEGA public URL)
        let direct_req = {
            let direct = self.clone().get_direct().await?;
            direct
                .clone()
                .get()
                .await
                .map_err(DownloadError::DirectLinkFailed)?
        };

        // Fire request / or branch for MEGA
        let client = crate::net::client().map_err(DownloadError::Config)?;
        let resp = match direct_req {
            DirectRequest::Http(request) => client
                .execute(request)
                .await
                .map_err(DownloadError::Request)?,
            DirectRequest::MegaPublicUrl(url) => {
                // MEGA public link handling: fetch nodes and download via mega::Client to disk.
                log::info!("downloading from {}", url.as_str());

                // 1) Init MEGA client over reqwest
                let http_client = crate::net::client()
                    .map_err(DownloadError::Config)?
                    .clone();
                let mega_client = match mega::ClientBuilder::new().https(true).build(http_client) {
                    Ok(c) => c,
                    Err(e) => {
                        log::error!("mega client build error: {:?}", e);
                        return Err(DownloadError::MegaClientBuild);
                    }
                };
                // 2) Resolve public nodes
                let nodes = match mega_client.fetch_public_nodes(url.as_str()).await {
                    Ok(n) => n,
                    Err(e) => {
                        log::error!(
                            "mega fetch_public_nodes error: {:?}, tried to fetch: {url}",
                            e
                        );
                        return Err(DownloadError::MegaFetchNodes);
                    }
                };

                // 3) Pick first file node
                let Some(handle) = nodes
                    .iter()
                    .find(|n| n.kind().is_file())
                    .map(|n| n.handle().to_string())
                else {
                    log::warn!("no file node found in MEGA link");
                    return Err(DownloadError::MegaNoFileNode);
                };
                let file_node = nodes
                    .get_node_by_handle(&handle)
                    .ok_or(DownloadError::MegaNoFileNode)?;

                // 4) Prepare output file path using node name
                let download_dir = APP_SETTINGS.read().unwrap().temp_dir.clone();
                tokio::fs::create_dir_all(&download_dir)
                    .await
                    .map_err(DownloadError::Io)?;
                let filepath = download_dir.join(file_node.name());
                let file = tokio::fs::File::create(&filepath)
                    .await
                    .map_err(DownloadError::Io)?;

                // 5) Download and extract in the background, so the UI gets
                // the receiver (and the progress) right away.
                let size = file_node.size();
                let _ = sd.send(GameDownloadStatus::Downloading(Progress::Pending(0.0)));
                tokio::spawn(async move {
                    let Some(file_node) = nodes.get_node_by_handle(&handle) else {
                        return;
                    };
                    let writer = MegaFileWriter::new(file, sd.clone(), size);
                    if let Err(e) = mega_client.download_node(file_node, writer).await {
                        log::error!("mega download_node error: {:?}", e);
                        let _ = sd.send(GameDownloadStatus::Downloading(Progress::Error(
                            DownloadError::MegaDownload.to_string(),
                        )));
                        return;
                    }
                    download::extract_and_finish(filepath, &sd).await;
                });
                return Ok(rc);
            }
        };

        let filename_fallback = resp.url().path().split('/').next_back();

        // Extract filename
        let header = resp
            .headers()
            .get("content-disposition")
            .map(|e| e.to_str().unwrap())
            .or(filename_fallback)
            .ok_or(DownloadError::MissingHeader("content-disposition"))?;
        let filename = header
            .replace(['"', '\\', '/', ':', '*', '?', '<', '>', '|'], "")
            .split('=')
            .nth(1)
            .or(filename_fallback)
            .ok_or(DownloadError::FilenameParse)?
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
        download::start_download_task(resp, sd, file, filepath);

        Ok(rc)
    }
}
