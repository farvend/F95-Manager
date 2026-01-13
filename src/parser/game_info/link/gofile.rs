use lazy_static::lazy_static;
use regex::Regex;
use reqwest::{
    Url,
    header::{HeaderMap, HeaderValue},
};
use serde::Deserialize;
use std::str::FromStr;

use crate::parser::CLIENT;

lazy_static! {
    static ref RE_GOFILE_TOKEN: Regex = Regex::new(r#"appdata\.wt *= *".*""#).unwrap();
}

/// Resolve a GoFile folder id to a direct file download URL and required headers.
/// Returns (url, headers) on success.
pub async fn resolve_gofile_file(id: &str) -> Option<(Url, HeaderMap<HeaderValue>)> {
    // Parse `wt` token from gofile global.js (temporary token)
    let text = reqwest::get("https://gofile.io/dist/js/global.js")
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    let captures = RE_GOFILE_TOKEN.captures(&text)?;
    let token = captures
        .get(0)?
        .as_str()
        .split('"')
        .nth(1)
        .filter(|s| !s.is_empty())?;

    let url = format!(
        "https://api.gofile.io/contents/{id}?wt={token}&contentFilter=&page=1&pageSize=1000&sortField=name&sortDirection=1"
    );

    // Create free temp account, extract account token
    let resp_txt = CLIENT
        .post("https://api.gofile.io/accounts")
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    let token = serde_json::from_str::<GofileAuth>(&resp_txt)
        .ok()?
        .data
        .token;

    // Query folder contents with Authorization
    let resp = reqwest::Client::builder()
        .build()
        .ok()?
        .get(url)
        .header("authorization", format!("Bearer {token}"))
        .send()
        .await
        .ok()?;
    let text = resp.text().await.ok()?;
    let data: GofileFiles = serde_json::from_str(&text).ok()?;

    // Pick first file child link
    let url = data
        .data
        .children
        .iter()
        .filter_map(|(_, node)| match node {
            GofileNode::File { link, .. } => Some(link.clone()),
            _ => None,
        })
        .next()?;
    let url = Url::from_str(&url).ok()?;

    let mut headers = HeaderMap::new();
    headers.append(
        "Cookie",
        HeaderValue::from_str(&format!("accountToken={token}")).ok()?,
    );

    Some((url, headers))
}

#[derive(serde::Deserialize)]
struct GofileAuth {
    status: String,
    data: GofileAuthData,
}
#[derive(serde::Deserialize)]
struct GofileAuthData {
    id: String,
    #[serde(rename = "rootFolder")]
    root_folder: String,
    tier: String,
    token: String,
}

// Gofile types to reflect the GoFile API JSON structure
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum GofileNode {
    #[serde(rename = "folder")]
    #[serde(rename_all = "camelCase")]
    Folder {
        can_access: bool,
        id: String,
        name: String,
        create_time: u64,
        mod_time: u64,
        code: String,
        public: bool,
        total_download_count: u64,
        total_size: u64,
        children_count: u32,
    },
    #[serde(rename = "file")]
    #[serde(rename_all = "camelCase")]
    File {
        #[serde(rename = "canAccess")]
        can_access: bool,
        id: String,
        parent_folder: String,
        name: String,
        create_time: u64,
        mod_time: u64,
        size: u64,
        download_count: u64,
        md5: String,
        mimetype: String,
        servers: Vec<String>,
        server_selected: String,
        link: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GofileMetadata {
    total_count: u32,
    total_pages: u32,
    page: u32,
    page_size: u32,
    has_next_page: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GofileData {
    can_access: bool,
    id: String,
    #[serde(rename = "type")]
    r#type: String,
    name: String,
    create_time: u64,
    mod_time: u64,
    code: String,
    public: bool,
    total_download_count: u64,
    total_size: u64,
    children_count: u32,
    children: std::collections::HashMap<String, GofileNode>,
}

#[derive(Debug, Clone, Deserialize)]
struct GofileFiles {
    status: String,
    data: GofileData,
    metadata: GofileMetadata,
}
