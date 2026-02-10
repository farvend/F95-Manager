use reqwest::Url;
use std::str::FromStr;
use thiserror::Error;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::app::settings;
use crate::{
    game_download::{GameDownloadStatus, Progress},
    parser::{CLIENT, game_info::HostingSubset},
};

use self::info::DirectRequest;
use super::cookies;

mod archive;
pub mod direct;
mod download;
pub mod gofile;
pub mod info;

#[cfg(test)]
mod tests;

// Futures-IO writer adapter for MEGA -> tokio::fs::File
use futures::io as futures_io;
use futures_io::AsyncWrite as FuturesAsyncWrite;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::AsyncWrite as TokioAsyncWrite;
use tokio::sync::mpsc::UnboundedSender;

// (kept for future header-decoding improvements)

// base64::Engine trait is required for `.decode()` calls.
use base64::Engine as _;

struct MegaFileWriter {
    file: tokio::fs::File,
    sd: UnboundedSender<GameDownloadStatus>,
    total: u64,
    written: u64,
    last_reported: u64,
}

// `tokio::fs::File` is `Unpin`, so this wrapper is also safe to treat as `Unpin`.
impl Unpin for MegaFileWriter {}

impl MegaFileWriter {
    fn new(file: tokio::fs::File, sd: UnboundedSender<GameDownloadStatus>, total: u64) -> Self {
        Self {
            file,
            sd,
            total,
            written: 0,
            last_reported: 0,
        }
    }
}

impl FuturesAsyncWrite for MegaFileWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, futures_io::Error>> {
        let this = self.get_mut();
        let mut pinned = Pin::new(&mut this.file);
        match TokioAsyncWrite::poll_write(pinned.as_mut(), cx, buf) {
            Poll::Ready(Ok(n)) => {
                this.written += n as u64;

                // Throttle progress updates: avoid spamming UI/channel on fast writes.
                // Send only when at least 256 KiB more written or at completion.
                const REPORT_STEP: u64 = 256 * 1024;
                if this.total > 0 {
                    let should_report = this.written == this.total
                        || this.written.saturating_sub(this.last_reported) >= REPORT_STEP;
                    if should_report {
                        this.last_reported = this.written;
                        let progress = (this.written as f32) / (this.total as f32);
                        let _ = this.sd.send(GameDownloadStatus::Downloading(
                            Progress::Pending(progress),
                        ));
                    }
                }
                Poll::Ready(Ok(n))
            }
            Poll::Ready(Err(e)) => {
                Poll::Ready(Err(futures_io::Error::new(futures_io::ErrorKind::Other, e)))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), futures_io::Error>> {
        let this = self.get_mut();
        let mut pinned = Pin::new(&mut this.file);
        match TokioAsyncWrite::poll_flush(pinned.as_mut(), cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => {
                Poll::Ready(Err(futures_io::Error::new(futures_io::ErrorKind::Other, e)))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_close(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), futures_io::Error>> {
        let this = self.get_mut();
        let mut pinned = Pin::new(&mut this.file);
        match TokioAsyncWrite::poll_shutdown(pinned.as_mut(), cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => {
                Poll::Ready(Err(futures_io::Error::new(futures_io::ErrorKind::Other, e)))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

pub use self::direct::DirectDownloadLink;
pub use self::DownloadError;
pub use self::info::DownloadLinkInfo;

fn truncate_utf8_to_boundary(s: &mut String, max_len: usize) {
    if s.len() <= max_len {
        return;
    }

    // `String::truncate` requires a UTF-8 char boundary.
    // Find the nearest boundary <= max_len.
    let new_len = s
        .char_indices()
        .take_while(|(i, _)| *i <= max_len)
        .map(|(i, _)| i)
        .last()
        .unwrap_or(0);
    s.truncate(new_len);
}

pub(crate) fn sanitize_filename(input: &str) -> String {
    // Aggressive Windows-safe sanitization.
    // Windows-incompatible characters: < > : " / \ | ? *
    // Also drop ASCII control chars and normalize whitespace.
    let mut out: String = input
        .chars()
        .filter(|&c| {
            !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
                && !c.is_ascii_control()
        })
        .collect();

    // Avoid path traversal / strange names.
    // Collapse any ".." sequences to single dot and remove directory separators already filtered.
    while out.contains("..") {
        out = out.replace("..", ".");
    }

    // Normalize common ugly whitespace: tabs/newlines -> space, and collapse multiple spaces.
    out = out
        .replace(['\t', '\r', '\n'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    // Trim leading/trailing spaces/dots which are invalid on Windows.
    out = out.trim_matches(|c| c == ' ' || c == '.').to_string();

    // Windows reserved device names are invalid even with an extension.
    // See: CON, PRN, AUX, NUL, COM1..COM9, LPT1..LPT9
    fn is_reserved_device(name: &str) -> bool {
        let base = name.split('.').next().unwrap_or("");
        let upper = base.to_ascii_uppercase();
        matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (upper.starts_with("COM") && upper[3..].parse::<u8>().ok().is_some_and(|n| (1..=9).contains(&n)))
            || (upper.starts_with("LPT") && upper[3..].parse::<u8>().ok().is_some_and(|n| (1..=9).contains(&n)))
    }

    if out.is_empty() {
        out = "download".to_string();
    } else if is_reserved_device(&out) {
        out = format!("_{out}");
    }

    // Keep filename reasonably sized (leave room for path / temp dir).
    const MAX_LEN: usize = 240;
    if out.len() > MAX_LEN {
        truncate_utf8_to_boundary(&mut out, MAX_LEN);
    }

    out
}

fn percent_decode_to_string(input: &str) -> String {
    // Minimal percent-decoder for RFC 5987 / URL-style encodings.
    // Keeps invalid sequences as-is.
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());

    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = bytes[i + 1];
            let lo = bytes[i + 2];
            let hex = |b: u8| -> Option<u8> {
                match b {
                    b'0'..=b'9' => Some(b - b'0'),
                    b'a'..=b'f' => Some(b - b'a' + 10),
                    b'A'..=b'F' => Some(b - b'A' + 10),
                    _ => None,
                }
            };

            if let (Some(hi), Some(lo)) = (hex(hi), hex(lo)) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }

        out.push(bytes[i]);
        i += 1;
    }

    String::from_utf8_lossy(&out).into_owned()
}

fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') {
        &s[1..s.len().saturating_sub(1)]
    } else {
        s
    }
}

/// Decode RFC 2047 encoded-word if present, e.g.
/// =?UTF-8?B?....?= or =?UTF-8?Q?....?=
///
/// This is a best-effort decoder for filenames in headers.
fn decode_rfc2047_word(s: &str) -> Option<String> {
    let s = s.trim();
    if !s.starts_with("=?") || !s.ends_with("?=") {
        return None;
    }
    let inner = &s[2..s.len() - 2];
    let mut it = inner.splitn(3, '?');
    let _charset = it.next()?;
    let encoding = it.next()?;
    let data = it.next()?;

    match encoding.to_ascii_uppercase().as_str() {
        "B" => base64::engine::general_purpose::STANDARD
            .decode(data)
            .ok()
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned()),
        "Q" => {
            // RFC 2047 Q-encoding: underscore means space, and =HH hex escapes.
            let data = data.replace('_', " ");
            let mut out: Vec<u8> = Vec::with_capacity(data.len());
            let bytes = data.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'=' && i + 2 < bytes.len() {
                    let hi = bytes[i + 1];
                    let lo = bytes[i + 2];
                    let hex = |b: u8| -> Option<u8> {
                        match b {
                            b'0'..=b'9' => Some(b - b'0'),
                            b'a'..=b'f' => Some(b - b'a' + 10),
                            b'A'..=b'F' => Some(b - b'A' + 10),
                            _ => None,
                        }
                    };
                    if let (Some(hi), Some(lo)) = (hex(hi), hex(lo)) {
                        out.push((hi << 4) | lo);
                        i += 3;
                        continue;
                    }
                }
                out.push(bytes[i]);
                i += 1;
            }
            Some(String::from_utf8_lossy(&out).into_owned())
        }
        _ => None,
    }
}

pub(crate) fn parse_content_disposition_filename(header_value: &str) -> Option<String> {
    // Content-Disposition parser (best-effort).
    // Prefer RFC 5987 `filename*=` over `filename=` when both exist.
    // Also support a very common broken pattern with RFC 2047 encoded-words.
    let mut filename_star: Option<String> = None;
    let mut filename_plain: Option<String> = None;

    for part in header_value.split(';') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("filename*=") {
            let v = strip_quotes(v);
            // RFC 5987: charset'lang'value
            let mut it = v.splitn(3, '\'');
            let _charset = it.next();
            let _lang = it.next();
            let value = it.next();

            if let Some(value) = value {
                filename_star = Some(percent_decode_to_string(value));
            } else if let Some(rest) = v.strip_prefix("UTF-8''") {
                filename_star = Some(percent_decode_to_string(rest));
            } else {
                filename_star = Some(percent_decode_to_string(v));
            }
            continue;
        }

        if let Some(v) = part.strip_prefix("filename=") {
            let v = strip_quotes(v);
            // Sometimes servers put RFC 2047 encoded-word here.
            filename_plain = decode_rfc2047_word(v).or_else(|| Some(v.to_string()));
            continue;
        }
    }

    filename_star.or(filename_plain)
}

#[derive(Debug, Clone)]
pub enum DownloadLink {
    Direct(DirectDownloadLink),
    Masked(Url),
}
impl DownloadLink {
    pub fn new(value: Url) -> Option<DownloadLink> {
        let mut segs = value.path_segments()?;

        if segs.next() == Some("masked") {
            // Validate masked target hosting is supported (e.g. skip workupload, mediafire, etc.)
            let host = segs.next()?;
            let host_url = Url::from_str(&format!("https://{host}")).ok()?;
            HostingSubset::try_from(host_url).ok()?;
            return Some(Self::Masked(value));
        }

        Some(Self::Direct(DirectDownloadLink::new(value)?))
    }
}

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error(transparent)]
    Network(#[from] reqwest::Error),

    #[error("No redirect")]
    NoRedirect,

    #[error("Failed to resolve direct download link")]
    DirectLinkFailed,

    #[error("Unsupported hosting")]
    UnsupportedHosting,

    #[error("Captcha required")]
    Captcha,

    #[error("Missing required header: {0}")]
    MissingHeader(&'static str),

    #[error("Server didn't provide Content-Length")]
    MissingContentLength,

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("Download task couldn't be started")]
    StartTask,

    #[error("Invalid URL")]
    InvalidUrl,

    #[error("Invalid redirect URL")]
    InvalidRedirectUrl,

    #[error("Failed to parse JSON")]
    JsonParse,

    #[error("Failed to build MEGA client")]
    MegaClientBuild,

    #[error("Failed to fetch MEGA public nodes")]
    MegaFetchNodes,

    #[error("No file node in MEGA link")]
    MegaNoFileNode,

    #[error("MEGA download failed")]
    MegaDownload,

    #[error("Failed to parse filename")]
    FilenameParse,
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
                // Validate masked hosting (path: /masked/{host}/{rest...}).
                // This keeps unsupported hostings out early.
                let host = link
                    .path_segments()
                    .and_then(|mut s| s.nth(1))
                    .ok_or(DownloadError::InvalidUrl)?;
                let host_url = Url::from_str(&format!("https://{host}"))
                    .map_err(|_| DownloadError::InvalidUrl)?;
                let _: HostingSubset = host_url
                    .try_into()
                    .map_err(|_| DownloadError::UnsupportedHosting)?;

                let ans = CLIENT
                    .post(link.clone())
                    .header(
                        "Content-Type",
                        "application/x-www-form-urlencoded; charset=UTF-8",
                    )
                    .header("Cookie", cookies())
                    .body("xhr=1&download=1")
                    .send()
                    .await?;

                let text = ans.text().await?;
                let resp: MaskedRedirection =
                    serde_json::from_str(&text).map_err(|_| DownloadError::JsonParse)?;

                if resp.status == "captcha" {
                    log::warn!("Pass the captcha on {}", link);
                    return Err(DownloadError::Captcha);
                }

                let url = Url::from_str(&resp.msg)
                    .map_err(|_| DownloadError::InvalidRedirectUrl)?;
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
                .ok_or(DownloadError::DirectLinkFailed)?
        };

        // Fire request / or branch for MEGA.
        // Reuse the shared client (same one as parser) to reduce connection churn.
        let client = &CLIENT;
        let resp = match direct_req {
            DirectRequest::Http(request) => client
                .execute(request)
                .await?,
            DirectRequest::MegaPublicUrl(url) => {
                // MEGA public link handling: fetch nodes and download via mega::Client to disk.
                log::info!("downloading from {}", url.as_str());

                // 1) Init MEGA client over reqwest
                let mega_client = match mega::ClientBuilder::new().https(true).build(CLIENT.clone()) {
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
                let file_node = match nodes.iter().find(|n| n.kind().is_file()) {
                    Some(n) => n,
                    None => {
                        log::warn!("no file node found in MEGA link");
                        return Err(DownloadError::MegaNoFileNode);
                    }
                };

                // 4) Prepare output file path using node name
                let filename = file_node.name().to_string();
                let download_dir = settings::with_settings(|s| s.temp_dir.clone());
                tokio::fs::create_dir_all(&download_dir).await?;
                let filepath = download_dir.join(filename);
                let file = tokio::fs::File::create(&filepath).await?;

                // 5) Start MEGA download into writer that updates progress
                let writer = MegaFileWriter::new(file, sd.clone(), file_node.size());
                if let Err(e) = mega_client.download_node(file_node, writer).await {
                    log::error!("mega download_node error: {:?}", e);
                    return Err(DownloadError::MegaDownload);
                }

                // 6) Run extraction pipeline
                download::extract_downloaded_archive(filepath.clone(), sd.clone()).await;

                // MEGA path completes here.
                return Ok(rc);
            }
        };

        let filename_fallback = resp
            .url()
            .path_segments()
            .and_then(|segs| segs.last())
            .filter(|s| !s.is_empty());

        // Extract filename
        let filename = resp
            .headers()
            .get("content-disposition")
            .and_then(|e| e.to_str().ok())
            .and_then(parse_content_disposition_filename)
            .or_else(|| filename_fallback.map(|s| s.to_string()))
            .ok_or(DownloadError::MissingHeader("content-disposition"))?;
        let filename = sanitize_filename(&filename);

        // Prepare file (use user-configured temp dir)
        let download_dir = settings::with_settings(|s| s.temp_dir.clone());
        tokio::fs::create_dir_all(&download_dir).await?;
        let filepath = download_dir.join(filename);
        let file = tokio::fs::File::create(&filepath).await?;

        // Start streaming to disk
        log::info!("downloading from {}", resp.url().as_str());
        download::start_download_task(resp, sd, file, filepath.clone()).await?;

        Ok(rc)
    }
}
