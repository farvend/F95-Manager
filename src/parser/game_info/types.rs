use derive_getters::Getters;
use reqwest::Url;
use serde::Deserialize;
use std::str::FromStr;

use super::link::DownloadLink;
use super::page::F95Page;

#[derive(Debug, Deserialize, Clone, Hash, Copy, PartialEq, Eq)]
pub struct ThreadId(pub u64);

impl ThreadId {
    pub fn get(&self) -> u64 {
        self.0
    }
    pub fn get_page(&self) -> F95Page {
        let url = format!("https://f95zone.to/threads/{}/", self.0);
        F95Page(Url::from_str(&url).unwrap())
    }
}

use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Platform: u8 {
        const WINDOWS = 0b0001;
        const LINUX   = 0b0010;
        const MAC     = 0b0100;
        const ANDROID = 0b1000;
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
            if t.is_empty() { continue; }

            if t.contains("win") || t == "pc" || t.contains("windows") {
                flags |= Platform::WINDOWS;
            }
            if t.contains("linux") {
                flags |= Platform::LINUX;
            }
            if t.contains("mac") || t.contains("osx") || t.contains("macos") {
                flags |= Platform::MAC;
            }
            if t.contains("android") {
                flags |= Platform::ANDROID;
            }
        }

        if flags.is_empty() {
            // Fallback: infer from the whole string
            if lower.contains("win") { flags |= Platform::WINDOWS; }
            if lower.contains("linux") { flags |= Platform::LINUX; }
            if lower.contains("mac") || lower.contains("osx") || lower.contains("macos") { flags |= Platform::MAC; }
            if lower.contains("android") { flags |= Platform::ANDROID; }
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
