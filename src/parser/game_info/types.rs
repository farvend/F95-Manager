use derive_getters::Getters;
use reqwest::Url;
use serde::Deserialize;
use std::str::FromStr;

use super::link::DownloadLink;
use super::page::F95PageUrl;

#[derive(Debug, Deserialize, Clone, Hash, Copy, PartialEq, Eq)]
pub struct ThreadId(pub u64);

impl ThreadId {
    pub fn get(&self) -> u64 {
        self.0
    }
    pub fn get_page(&self) -> F95PageUrl {
        let url = format!("https://f95zone.to/threads/{}/", self.0);
        // Url::from_str on a formatted literal URL should not fail, but avoid unwrap to be safe.
        match Url::from_str(&url) {
            Ok(u) => F95PageUrl(u),
            Err(e) => {
                log::error!("Failed to construct thread page URL {}: {}", url, e);
                // Fallback to a safe default; this should rarely happen.
                // Avoid panicking even if URL parsing fails (should not happen for a literal).
                F95PageUrl(Url::from_str("https://f95zone.to/").unwrap_or_else(|e| {
                    log::error!("Failed to parse base url: {e}");
                    // As last resort, fall back to a dummy local URL.
                    Url::from_str("http://localhost/").expect("localhost url valid")
                }))
            }
        }
    }
}

use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Platform: u8 {
        const WINDOWS = 0b00001;
        const LINUX   = 0b00010;
        const MAC     = 0b00100;
        const ANDROID = 0b01000;
        const OTHER   = 0b10000;
    }
}

impl From<&str> for Platform {
    fn from(value: &str) -> Self {
        let lower = value.to_lowercase();
        let mut flags = Platform::empty();

        // Normalize common delimiters and split into tokens
        let normalized = lower
            .replace('\\', "/")
            .replace(',', "/")
            .replace('|', "/")
            .replace('&', "/");

        for token in normalized.split('/') {
            let t = token.trim();
            if t.is_empty() {
                continue;
            }

            if t.contains("win") || t == "pc" {
                flags |= Platform::WINDOWS;
            }
            if t.contains("linux") {
                flags |= Platform::LINUX;
            }
            if t.contains("mac") || t.contains("osx") {
                flags |= Platform::MAC;
            }
            if t.contains("android") {
                flags |= Platform::ANDROID;
            }
        }

        flags
    }
}

#[derive(Getters, Debug)]
pub struct PlatformDownloads {
    platform: Platform,
    links: Vec<DownloadLink>,
}

impl PlatformDownloads {
    pub fn new(platform: Platform, links: Vec<DownloadLink>) -> Self {
        Self { platform, links }
    }
}
