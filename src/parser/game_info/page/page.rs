use lazy_static::lazy_static;
use regex::Regex;
use reqwest::Url;
use std::str::FromStr;
use thiserror::Error;

use crate::parser::game_info::DownloadLink;
use crate::parser::game_info::cookies;
use crate::parser::game_info::{Platform, PlatformDownloads};

lazy_static! {
    static ref RE_LINK: Regex = Regex::new(r#"https://[\w./]*"#)
        .expect("RE_LINK regex should compile");
    static ref RE_PLATFORM: Regex = Regex::new(r">[\w/]+<")
        .expect("RE_PLATFORM regex should compile");
    static ref RE_BR: Regex = Regex::new(r"<br\s*/?>")
        .expect("RE_BR regex should compile");
}

pub struct F95PageUrl(pub Url);
pub struct F95Page(pub String);

#[derive(Debug, Error)]
pub enum GetLinksError {
    #[error(transparent)]
    Request(#[from] reqwest::Error),

    #[error("Downloads block not found on page")]
    NoDownloadsBlock,

    #[error("No platform links found")]
    NoPlatformLinks,
}

impl F95PageUrl {
    pub async fn get_page(&self) -> Result<F95Page, reqwest::Error> {
        let client = reqwest::Client::builder().build()?;
        let text = client
            .get(self.0.clone())
            .header("Cookie", cookies())
            .send()
            .await?
            .text()
            .await?;
        Ok(F95Page(text))
    }
}

impl F95Page {
    pub fn get_download_links(&self) -> Result<Vec<PlatformDownloads>, GetLinksError> {
        let html = scraper::Html::parse_document(&self.0);
        let selector = scraper::Selector::parse(r#"[style="text-align: center"]"#)
            .expect("selector should be a valid CSS selector");
        let span_html = &html
            .select(&selector)
            .filter(|e| e.html().contains("DOWNLOAD"))
            .next()
            .ok_or(GetLinksError::NoDownloadsBlock)?
            .html();
        let span_html = span_html.split_once("DOWNLOAD").map(|(_, r)| r).ok_or(GetLinksError::NoDownloadsBlock)?;
        let parts: Vec<&str> = RE_BR.split(span_html).collect();

        let mut downloads = Vec::new();

        for platform_downloads in parts.iter().skip(1) {
            let platform = match RE_PLATFORM
                .captures(platform_downloads)
                .and_then(|e| e.get(0))
            {
                Some(m) => m.as_str(),
                None => continue,
            };
            let platform = Platform::from(&platform[1..platform.len() - 1]);

            let links: Vec<DownloadLink> = RE_LINK
                .captures_iter(platform_downloads)
                .filter_map(|link| {
                    let url = link.get(0).map(|m| m.as_str())?;
                    DownloadLink::new(Url::from_str(url).ok()?)
                })
                .collect();

            if links.is_empty() {
                continue;
            }

            downloads.push(PlatformDownloads::new(platform, links));
        }

        if downloads.is_empty() {
            return Err(GetLinksError::NoPlatformLinks);
        }

        Ok(downloads)
    }
}
