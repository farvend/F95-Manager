use lazy_static::lazy_static;
use regex::Regex;
use reqwest::{Client, StatusCode, Url};

use super::{
    cookies,
    page::{F95Page, F95PageUrl},
};
use crate::tags::TAGS;
use std::{fmt, time::Duration};

#[derive(Debug, Clone)]
pub struct ThreadMeta {
    pub title: String,
    pub cover: String,
    pub screens: Vec<String>,
    pub tag_ids: Vec<u32>,
    pub prefix_ids: Vec<u32>,
    pub creator: String,
    pub version: String,
}

#[derive(Debug)]
pub enum FetchThreadMetaError {
    BuildClient,
    Request(reqwest::Error),
    ReadText(reqwest::Error),
    OgTitleMissing,
    TitleMissing,
    VersionMissing,
    AuthorMissing,
    CoverMissing,
}

impl fmt::Display for FetchThreadMetaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchThreadMetaError::BuildClient => write!(f, "failed to build HTTP client"),
            FetchThreadMetaError::Request(e) => write!(f, "request error: {}", e),
            FetchThreadMetaError::ReadText(e) => write!(f, "read body error: {}", e),
            FetchThreadMetaError::OgTitleMissing => write!(f, "OG title not found or malformed"),
            FetchThreadMetaError::TitleMissing => write!(f, "thread title missing"),
            FetchThreadMetaError::VersionMissing => write!(f, "thread version missing"),
            FetchThreadMetaError::AuthorMissing => write!(f, "thread author missing"),
            FetchThreadMetaError::CoverMissing => {
                write!(f, "cover not found (no cover or screenshots)")
            }
        }
    }
}

impl std::error::Error for FetchThreadMetaError {}

lazy_static! {
    static ref RE_OG_TITLE: Regex = Regex::new(r#"</span>.* *\[.*\] *\[.*\]<"#).unwrap();
    static ref RE_ATTACH: Regex = Regex::new(
        r#"href="(https://attachments\.f95zone\.to/\d+/\d+/\d+_[A-Za-z0-9_\-]+\.[A-Za-z0-9]+(?:\?[^\s"'<>]*)?)""#
    ).unwrap();
    static ref RE_COVER: Regex = Regex::new(
        r#"src="(https://attachments\.f95zone\.to/\d+/\d+/\d+_[A-Za-z0-9_\-]+\.[A-Za-z0-9]+(?:\?[^\s"'<>]*)?)""#
    ).unwrap();
    static ref RE_TAG_BLOCK: Regex = Regex::new(r#"(?s)<span class="js-tagList">(.+?)</span>"#).unwrap();
    static ref RE_TAG_TEXT: Regex = Regex::new(r#">([^<>]+)<"#).unwrap();
    static ref RE_TITLE_BLOCK: Regex =
        Regex::new(r#"(?s)<h1 class="p-title-value">(.+?)</h1>"#).unwrap();
    static ref RE_PREFIX_ID: Regex = Regex::new(r#"[?&]prefix_id=(\d+)"#).unwrap();
}

/// Fetch thread page and extract cover, screenshots and tag IDs.
/// Returns typed errors for better diagnostics. If a cover is not found,
/// falls back to the first screenshot if available.
pub async fn fetch_thread_meta(thread_id: u64) -> Result<ThreadMeta, FetchThreadMetaError> {
    let url = format!("https://f95zone.to/threads/{}/", thread_id);

    let client = Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36")
        .build()
        .map_err(|_| FetchThreadMetaError::BuildClient)?;

    let resp: reqwest::Response = client
        .get(&url)
        .header("Cookie", cookies())
        .send()
        .await
        .map_err(FetchThreadMetaError::Request)?;

    if resp.status() == StatusCode::TOO_MANY_REQUESTS {
        tokio::time::sleep(Duration::from_secs(1)).await;
        return Box::pin(fetch_thread_meta(thread_id)).await;
    }

    let text = resp.text().await.map_err(FetchThreadMetaError::ReadText)?;

    let mut prefix_ids = RE_TITLE_BLOCK
        .captures(&text)
        .and_then(|capture| capture.get(1))
        .map(|title| {
            RE_PREFIX_ID
                .captures_iter(title.as_str())
                .filter_map(|capture| capture.get(1)?.as_str().parse::<u32>().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let engine_ids = TAGS
        .prefixes
        .games
        .iter()
        .find(|group| group.name == "Engine")
        .map(|group| {
            group
                .prefixes
                .iter()
                .map(|prefix| prefix.id as u32)
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    prefix_ids.sort_by_key(|id| !engine_ids.contains(id));
    prefix_ids.dedup();

    let full_title_html = match RE_OG_TITLE
        .captures(&text)
        .and_then(|cap| cap.get(0))
        .map(|m| m.as_str().to_string())
    {
        Some(title) => title,
        None => {
            return parse_error(thread_id, &url, &text, FetchThreadMetaError::OgTitleMissing).await;
        }
    };

    let full_title = match full_title_html.rsplit_once("</span>").map(|(_, r)| r) {
        Some(title) => title,
        None => {
            return parse_error(thread_id, &url, &text, FetchThreadMetaError::OgTitleMissing).await;
        }
    };

    let mut title_parts = full_title.split('[');

    // Title
    let Some(title_raw) = title_parts.next() else {
        return parse_error(thread_id, &url, &text, FetchThreadMetaError::TitleMissing).await;
    };
    let title = title_raw.trim().to_string();

    // Version (strip trailing ']')
    let Some(version_raw) = title_parts.next() else {
        return parse_error(thread_id, &url, &text, FetchThreadMetaError::VersionMissing).await;
    };
    let version = version_raw.trim().trim_end_matches(']').trim().to_string();

    // Author (strip trailing markers like ']' and '<')
    let Some(author_raw) = title_parts.next() else {
        return parse_error(thread_id, &url, &text, FetchThreadMetaError::AuthorMissing).await;
    };
    let author_raw = author_raw.trim();
    let creator = author_raw
        .trim_end_matches('<')
        .trim()
        .trim_end_matches(']')
        .trim()
        .to_string();

    // Screenshots: https://attachments.f95zone.to/2025/08/5195249_1755719682348.png
    // Allow optional query string; accept [A-Za-z0-9_\-] in filename.
    // Filter to only include image files (exclude .zip, .rar, etc.)
    let image_extensions = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];
    let mut screens: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for cap in RE_ATTACH.captures_iter(&text) {
        let s = cap.get(1).unwrap().as_str().to_string();
        // Extract extension (handle query strings like "image.png?hash=123")
        let ext = s
            .split('?')
            .next()
            .unwrap_or(&s)
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_lowercase();
        if image_extensions.contains(&ext.as_str()) && seen.insert(s.clone()) {
            screens.push(s);
        }
    }

    // Cover: prefer explicit cover; fallback to first screenshot if available.
    let cover = match RE_COVER
        .captures(&text)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())
        .or_else(|| screens.first().cloned())
    {
        Some(cover) => cover,
        None => {
            return parse_error(thread_id, &url, &text, FetchThreadMetaError::CoverMissing).await;
        }
    };

    // Tags block: <span class="js-tagList"> ... </span> (non-greedy + dotall)
    let mut tag_ids: Vec<u32> = Vec::new();
    if let Some(cap) = RE_TAG_BLOCK.captures(&text) {
        let block = cap.get(1).map(|m| m.as_str()).unwrap_or("");

        // Inner texts: >3d game< etc.
        let mut seen_tags = std::collections::HashSet::new();

        // Build reverse map name (lowercased) -> id from tags.json
        let mut reverse: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for (id_str, name) in &TAGS.tags {
            if let Ok(id) = id_str.parse::<u32>() {
                reverse.insert(name.to_lowercase(), id);
            }
        }

        for tcap in RE_TAG_TEXT.captures_iter(block) {
            let name = tcap
                .get(1)
                .map(|m| m.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if name.is_empty() {
                continue;
            }
            let lname = name.to_lowercase();
            if seen_tags.insert(lname.clone()) {
                if let Some(id) = reverse.get(&lname) {
                    tag_ids.push(*id);
                }
            }
        }
    }

    Ok(ThreadMeta {
        title,
        cover,
        screens,
        tag_ids,
        prefix_ids,
        version,
        creator,
    })
}

async fn parse_error<T>(
    thread_id: u64,
    url: &str,
    html: &str,
    error: FetchThreadMetaError,
) -> Result<T, FetchThreadMetaError> {
    save_failed_thread_meta_html(thread_id, url, html, &error).await;
    Err(error)
}

async fn save_failed_thread_meta_html(
    thread_id: u64,
    url: &str,
    html: &str,
    error: &FetchThreadMetaError,
) {
    let parsed_url = match Url::parse(url) {
        Ok(url) => url,
        Err(parse_err) => {
            log::error!("Failed to parse thread URL for HTML dump: {parse_err}");
            return;
        }
    };

    let page = F95Page(html.to_string());
    match page
        .save_failed_parse_html(&F95PageUrl(parsed_url), error)
        .await
    {
        Ok(path) => {
            log::warn!(
                "Saved failed thread meta HTML for {} to {}",
                thread_id,
                path.to_string_lossy()
            );
        }
        Err(save_err) => {
            log::error!(
                "Failed to save thread meta HTML for {}: {}",
                thread_id,
                save_err
            );
        }
    }
}
