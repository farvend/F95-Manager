use reqwest::Url;
use std::str::FromStr;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::{game_download::GameDownloadStatus, parser::CLIENT};
use crate::app::settings::APP_SETTINGS;

use super::cookies;

pub mod info;
pub mod gofile;
pub mod direct;
mod archive;
mod download;

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

        // Resolve direct download info
        let info = {
            let direct = self.clone().get_direct().await?;
            direct.clone().get().await.ok_or(DownloadError::UnexpectedResponse)?
        };

        // Fire request
        let client = reqwest::Client::builder()
            .build()
            .map_err(DownloadError::ClientBuild)?;
        let resp = info
            .clone()
            .build(client)
            .send()
            .await
            .map_err(DownloadError::Request)?;

        // Extract filename
        let header = resp
            .headers()
            .get("content-disposition")
            .ok_or(DownloadError::MissingHeader("content-disposition"))?;
        let filename = header
            .to_str()
            .map_err(|_| DownloadError::UnexpectedResponse)?
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
        log::info!("downloading from {}", info.url.as_str());
        if !download::start_download_task(resp, sd, file, filepath.clone()).await {
            return Err(DownloadError::StartTask);
        }

        Ok(rc)
    }
}
