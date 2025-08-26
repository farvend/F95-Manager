use regex::Regex;
use reqwest::Client;
use lazy_static::lazy_static;

use super::cookies;
use crate::tags::TAGS;

#[derive(Debug, Clone)]
pub struct ThreadMeta {
    pub title: String,
    pub cover: String,
    pub screens: Vec<String>,
    pub tag_ids: Vec<u32>,
    pub creator: String,
    pub version: String,
}
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
}

/// Fetch thread page and extract cover, screenshots and tag IDs.
/// Uses regex-based parsing as suggested and maps tag names to IDs via tags.json.
pub async fn fetch_thread_meta(thread_id: u64) -> Option<ThreadMeta> {
    let url = format!("https://f95zone.to/threads/{}/", thread_id);

    let client = Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36")
        .build()
        .ok()?;

    let text = client
        .get(&url)
        .header("Cookie", cookies())
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;

    let full_title = &RE_OG_TITLE
        .captures(&text)
        .and_then(|cap| cap.get(0))?
        .as_str()
        .rsplit_once("</span>")?
        .1;
    let mut title_parts = full_title.split('[');
    // Title from OG meta
    let title = title_parts.next()?.trim().to_string();

    let version = title_parts.next()?.trim();
    let version = version[..version.len()-1].to_string();

    let author = title_parts.next()?.trim();
    let author = author[..author.len()-1].to_string();

    // Screenshots: https://attachments.f95zone.to/2025/08/5195249_1755719682348.png
    // Allow optional query string; accept [A-Za-z0-9_\-] in filename.
    let mut screens: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for cap in RE_ATTACH.captures_iter(&text) {
        let s = cap.get(1).unwrap().as_str().to_string();
        if seen.insert(s.clone()) {
            screens.push(s);
        }
    }

    // Cover fallback = first screenshot if available
    let cover = RE_COVER
        .captures(&text)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())?;

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
            let mut name = tcap.get(1).map(|m| m.as_str()).unwrap_or("").trim().to_string();
            if name.is_empty() {
                continue;
            }
            // Basic HTML entity decoding that commonly appears
            name = name;
            let lname = name.to_lowercase();
            if seen_tags.insert(lname.clone()) {
                if let Some(id) = reverse.get(&lname) {
                    tag_ids.push(*id);
                }
            }
        }
    }

    Some(ThreadMeta { title, cover, screens, tag_ids, version, creator: author })
}
