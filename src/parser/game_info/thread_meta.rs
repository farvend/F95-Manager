use lazy_static::lazy_static;
use regex::Regex;
use reqwest::{Client, StatusCode};
use std::time::Duration;
use thiserror::Error;

use super::cookies;
use crate::tags::TAGS;

fn tags_reverse_map() -> &'static std::collections::HashMap<String, u32> {
    // Lowercased tag name -> tag id for fast lookups.
    lazy_static! {
        static ref REVERSE: std::collections::HashMap<String, u32> = {
            let mut m = std::collections::HashMap::with_capacity(TAGS.tags.len());
            for (id, name) in &TAGS.tags {
                m.insert(name.to_lowercase(), *id);
            }
            m
        };
    }
    &REVERSE
}

#[derive(Debug, Clone)]
pub struct ThreadMeta {
    pub title: String,
    pub cover: String,
    pub screens: Vec<String>,
    pub tag_ids: Vec<u32>,
    pub creator: String,
    pub version: String,
}

#[derive(Debug, Error)]
pub enum FetchThreadMetaError {
    #[error(transparent)]
    Request(#[from] reqwest::Error),

    #[error("OG title not found or malformed")]
    OgTitleMissing,
    #[error("thread title missing")]
    TitleMissing,
    #[error("thread version missing")]
    VersionMissing,
    #[error("thread author missing")]
    AuthorMissing,
    #[error("cover not found (no cover or screenshots)")]
    CoverMissing,
}

lazy_static! {
    static ref RE_OG_TITLE: Regex = Regex::new(r#"</span>.* *\[.*\] *\[.*\]<"#)
        .expect("RE_OG_TITLE regex should compile");
    static ref RE_ATTACH: Regex = Regex::new(
        r#"href=\"(https://attachments\.f95zone\.to/\d+/\d+/\d+_[A-Za-z0-9_\-]+\.[A-Za-z0-9]+(?:\?[^\s'\"<>]*)?)\""#,
    )
    .expect("RE_ATTACH regex should compile");
    static ref RE_COVER: Regex = Regex::new(
        r#"src=\"(https://attachments\.f95zone\.to/\d+/\d+/\d+_[A-Za-z0-9_\-]+\.[A-Za-z0-9]+(?:\?[^\s'\"<>]*)?)\""#,
    )
    .expect("RE_COVER regex should compile");
    static ref RE_TAG_BLOCK: Regex = Regex::new(r#"(?s)<span class=\"js-tagList\">(.+?)</span>"#)
        .expect("RE_TAG_BLOCK regex should compile");
    static ref RE_TAG_TEXT: Regex = Regex::new(r#">([^<>]+)<"#)
        .expect("RE_TAG_TEXT regex should compile");
}

/// Fetch thread page and extract cover, screenshots and tag IDs.
/// If cover is not found, falls back to the first screenshot if available.
pub async fn fetch_thread_meta(thread_id: u64) -> Result<ThreadMeta, FetchThreadMetaError> {
    let url = format!("https://f95zone.to/threads/{}/", thread_id);

    let client = Client::builder()
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36",
        )
        .build()?;

    // Simple retry for rate-limits.
    let mut attempts = 0u8;
    loop {
        let resp: reqwest::Response = client
            .get(&url)
            .header("Cookie", cookies())
            .send()
            .await?;

        if resp.status() == StatusCode::TOO_MANY_REQUESTS && attempts < 3 {
            attempts += 1;
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        }

        let text = resp.text().await?;

        let full_title_html = RE_OG_TITLE
            .captures(&text)
            .and_then(|cap| cap.get(0))
            .map(|m| m.as_str().to_string())
            .ok_or(FetchThreadMetaError::OgTitleMissing)?;

        let full_title = full_title_html
            .rsplit_once("</span>")
            .map(|(_, r)| r)
            .ok_or(FetchThreadMetaError::OgTitleMissing)?;

        let mut title_parts = full_title.split('[');

        // Title
        let title = title_parts
            .next()
            .ok_or(FetchThreadMetaError::TitleMissing)?
            .trim()
            .to_string();

        // Version (strip trailing ']')
        let version = title_parts
            .next()
            .ok_or(FetchThreadMetaError::VersionMissing)?
            .trim()
            .trim_end_matches(']')
            .trim()
            .to_string();

        // Author (strip trailing markers like ']' and '<')
        let author_raw = title_parts
            .next()
            .ok_or(FetchThreadMetaError::AuthorMissing)?
            .trim();
        let creator = author_raw
            .trim_end_matches('<')
            .trim()
            .trim_end_matches(']')
            .trim()
            .to_string();

        // Screenshots: only include images.
        let image_extensions = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];
        let mut screens: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for cap in RE_ATTACH.captures_iter(&text) {
            let s = match cap.get(1) {
                Some(m) => m.as_str().to_string(),
                None => continue,
            };
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
        let cover = RE_COVER
            .captures(&text)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .or_else(|| screens.get(0).cloned())
            .ok_or(FetchThreadMetaError::CoverMissing)?;

        // Tags
        let mut tag_ids: Vec<u32> = Vec::new();
        if let Some(cap) = RE_TAG_BLOCK.captures(&text) {
            let block = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let mut seen_tags = std::collections::HashSet::new();

            let reverse = tags_reverse_map();

            for tcap in RE_TAG_TEXT.captures_iter(block) {
                let name = tcap.get(1).map(|m| m.as_str()).unwrap_or("").trim();
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

        return Ok(ThreadMeta {
            title,
            cover,
            screens,
            tag_ids,
            creator,
            version,
        });
    }
}
