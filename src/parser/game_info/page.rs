use lazy_static::lazy_static;
use regex::Regex;
use reqwest::Url;
use std::str::FromStr;

use super::link::DownloadLink;
use super::types::{Platform, PlatformDownloads};
use super::cookies;

lazy_static! {
    static ref RE_DOWNLOADS: Regex = Regex::new(r#"DOWNLOAD.*\n.*\n.*\n.*\n.*"#).unwrap();
    static ref RE_PLATFORM_LINKS: Regex =
        Regex::new(r" *<.*href.*").unwrap();
    static ref RE_LINK: Regex = Regex::new(r#"https://[\w./]*"#).unwrap();
    static ref RE_PLATFORM: Regex = Regex::new(r"<b>.*</b>*").unwrap();
}

pub struct F95Page(pub Url);

#[derive(Debug)]
pub enum GetLinksError {
    BuildClient,
    Request(reqwest::Error),
    ReadText(reqwest::Error),
    NoDownloadsBlock,
    PlatformLineFormat,
    PlatformNameMissing,
    NoPlatformLinks,
}

impl F95Page {
    pub async fn get_download_links(self) -> Result<Vec<PlatformDownloads>, GetLinksError> {
        let client = reqwest::Client::builder()
            .build()
            .map_err(|_| GetLinksError::BuildClient)?;

        let text = client
            .get(self.0)
            .header("Cookie", cookies())
            .send()
            .await
            .map_err(GetLinksError::Request)?
            .text()
            .await
            .map_err(GetLinksError::ReadText)?;

        let cap = RE_DOWNLOADS
            .captures(&text)
            .ok_or(GetLinksError::NoDownloadsBlock)?;
        let span_html = cap
            .get(0)
            .map(|m| m.as_str())
            .ok_or(GetLinksError::NoDownloadsBlock)?;

        let mut downloads = Vec::new();

        for platform_links in RE_PLATFORM_LINKS.captures_iter(span_html) {
            let platform_downloads = platform_links
                .get(0)
                .ok_or(GetLinksError::PlatformLineFormat)?
                .as_str();

            let platform = RE_PLATFORM
                .captures(platform_downloads)
                .and_then(|e| e.get(0))
                .ok_or(GetLinksError::PlatformNameMissing)?
                .as_str();
            let platform = Platform::from(&platform[3..platform.len()-4]);

            let links = RE_LINK
                .captures_iter(platform_downloads)
                .filter_map(|link| {
                    let url = link.get(0).map(|m| m.as_str())?;
                    DownloadLink::new(Url::from_str(url).ok()?)
                })
                .collect::<Vec<DownloadLink>>();

            downloads.push(PlatformDownloads::new(platform, links));
        }

        if downloads.is_empty() {
            return Err(GetLinksError::NoPlatformLinks);
        }

        Ok(downloads)
    }
}
