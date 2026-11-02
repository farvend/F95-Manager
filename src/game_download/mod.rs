use std::fmt;
use std::path::PathBuf;
use std::sync::mpsc;

use tokio::sync::mpsc::UnboundedReceiver;

use crate::app::ui::captcha::{self, CaptchaError};
use crate::parser::game_info::link::f95dl::F95DlError;
use crate::parser::game_info::link::{DownloadError, DownloadLink};
use crate::parser::game_info::page::GetPageError;
use crate::parser::game_info::{F95PageUrl, Platform, PlatformDownloads};

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

/// Why one hosting could not be downloaded.
#[derive(Debug)]
pub enum AttemptError {
    Download(DownloadError),
    /// F95 wanted an hCaptcha and the check window did not produce a link.
    Captcha(CaptchaError),
}

impl fmt::Display for AttemptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AttemptError::Download(error) => write!(f, "{error}"),
            AttemptError::Captcha(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for AttemptError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AttemptError::Download(error) => Some(error),
            AttemptError::Captcha(error) => Some(error),
        }
    }
}

impl AttemptError {
    /// The user closed the check window: do not open it again for the next
    /// mirror of the same game.
    fn stops_other_attempts(&self) -> bool {
        matches!(self, AttemptError::Captcha(CaptchaError::Cancelled))
    }
}

/// One hosting tried for a game and why it failed.
#[derive(Debug)]
pub struct HostAttemptError {
    pub host: String,
    pub error: AttemptError,
}

/// Start downloading from `link`. When F95 wants an hCaptcha for its download
/// page, the user passes it in a small browser window and the hosting URL from
/// that window is downloaded instead.
async fn start_download(
    link: &DownloadLink,
) -> Result<UnboundedReceiver<GameDownloadStatus>, AttemptError> {
    let first_try = link.download().await;
    match (link, first_try) {
        (
            DownloadLink::F95Page(page),
            Err(DownloadError::F95Page(F95DlError::CaptchaRequired)),
        ) => {
            log::info!("F95 wants a check for {}; opening the check window", page.host);
            let host_url = captcha::solve(page.url.clone(), page.host.clone())
                .await
                .map_err(AttemptError::Captcha)?;
            log::info!("check passed for {}: {}", page.host, host_url.as_str());
            let hosting_link = DownloadLink::new(host_url, &page.host)
                .ok_or(AttemptError::Download(DownloadError::UnsupportedHosting))?;
            hosting_link
                .download()
                .await
                .map_err(AttemptError::Download)
        }
        (_, outcome) => outcome.map_err(AttemptError::Download),
    }
}

/// Every hosting of the selected platform failed.
///
/// Mirrors of one post usually fail for the same reason (the same captcha, the
/// same dead post), so hosts sharing an error are listed together instead of
/// repeating the message once per mirror.
#[derive(Debug)]
pub struct AllHostsFailed(pub Vec<HostAttemptError>);

impl fmt::Display for AllHostsFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            return write!(f, "no download links to try");
        }
        let mut groups: Vec<(String, Vec<&str>)> = Vec::new();
        for attempt in &self.0 {
            let message = attempt.error.to_string();
            match groups.iter_mut().find(|(known, _)| *known == message) {
                Some((_, hosts)) => hosts.push(&attempt.host),
                None => groups.push((message, vec![&attempt.host])),
            }
        }
        write!(f, "all hostings failed: ")?;
        for (index, (message, hosts)) in groups.iter().enumerate() {
            if index > 0 {
                write!(f, "; ")?;
            }
            write!(f, "{}: {message}", hosts.join(", "))?;
        }
        Ok(())
    }
}

impl std::error::Error for AllHostsFailed {}

pub fn create_download_task(page: F95PageUrl) -> mpsc::Receiver<GameDownloadStatus> {
    let rt = crate::app::RUNTIME.get().unwrap();

    // Создаем канал для передачи статусов загрузки
    let (tx, rx) = mpsc::channel();

    rt.spawn(async move {
        let downloads = match load_download_page(&page).await {
            Ok(downloads) => downloads,
            Err(err) => {
                log::error!("err getting links: {err}");
                let _ = tx.send(GameDownloadStatus::Downloading(Progress::Error(err)));
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

        let mut failures = Vec::new();
        for link in links {
            match start_download(link).await {
                Ok(mut download_recv) => {
                    while let Some(status) = download_recv.recv().await {
                        if tx.send(status).is_err() {
                            return;
                        }
                    }
                    return;
                }
                Err(error) => {
                    let host = link.host_label();
                    log::error!("download from {host} failed: {error}");
                    let stop = error.stops_other_attempts();
                    failures.push(HostAttemptError { host, error });
                    if stop {
                        break;
                    }
                }
            }
        }

        let failed = AllHostsFailed(failures);
        let _ = tx.send(GameDownloadStatus::Downloading(Progress::Error(
            failed.to_string(),
        )));
    });
    rx
}

/// Fetch the thread and its download links, falling back to the embedded
/// browser when Cloudflare challenges the app's own HTTP request.
///
/// The browser gets its own `cf_clearance`, which is then stored and reused by
/// the app, so the user only has to pass the check once per clearance expiry.
async fn load_download_page(page: &F95PageUrl) -> Result<Vec<PlatformDownloads>, String> {
    let body = match page.get_page().await {
        Ok(body) => body,
        Err(GetPageError::CloudflareChallenge) => {
            log::info!("Cloudflare challenged the request; opening the check window");
            let session = crate::app::ui::captcha::refresh_session(page.0.clone())
                .await
                .map_err(|error| format!("Cloudflare check: {error}"))?;
            crate::app::config::update_cookies_from_browser(&session.cookies, &session.user_agent);
            page.get_page().await.map_err(|error| error.to_string())?
        }
        Err(error) => return Err(error.to_string()),
    };
    match body.get_download_links() {
        Ok(links) => Ok(links),
        Err(error) => {
            log::error!("err getting links: {error}");
            if let Err(save_err) = body.save_failed_parse_html(page, &error).await {
                log::error!("Failed to save parser HTML: {save_err}");
            }
            Err(error.to_string())
        }
    }
}

pub fn create_download_from_link(link: DownloadLink) -> mpsc::Receiver<GameDownloadStatus> {
    let rt = crate::app::RUNTIME.get().unwrap();
    let (tx, rx) = mpsc::channel();

    rt.spawn(async move {
        match start_download(&link).await {
            Ok(mut download_recv) => {
                while let Some(status) = download_recv.recv().await {
                    if tx.send(status).is_err() {
                        return; // receiver dropped
                    }
                }
            }
            Err(error) => {
                let failed = AllHostsFailed(vec![HostAttemptError {
                    host: link.host_label(),
                    error,
                }]);
                let _ = tx.send(GameDownloadStatus::Downloading(Progress::Error(
                    failed.to_string(),
                )));
            }
        }
    });

    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attempt(host: &str, error: DownloadError) -> HostAttemptError {
        HostAttemptError {
            host: host.to_string(),
            error: AttemptError::Download(error),
        }
    }

    #[test]
    fn groups_hosts_that_fail_the_same_way() {
        let failed = AllHostsFailed(vec![
            attempt("GOFILE", DownloadError::F95Page(F95DlError::CaptchaRequired)),
            attempt("PIXELDRAIN", DownloadError::F95Page(F95DlError::CaptchaRequired)),
            attempt("MIXDROP", DownloadError::UnsupportedHosting),
            attempt("MEGA", DownloadError::F95Page(F95DlError::CaptchaRequired)),
        ]);

        assert_eq!(
            failed.to_string(),
            "all hostings failed: GOFILE, PIXELDRAIN, MEGA: F95 requires an hCaptcha for this link; \
             pass it in the browser and retry; MIXDROP: download host is not supported"
        );
    }
}
