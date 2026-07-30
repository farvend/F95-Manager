use reqwest::{
    Url,
    header::{HeaderMap, HeaderValue},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::OnceCell;

const WEBSITE_TOKEN_SECRET: &str = "9844d94d963d30";
const WEBSITE_TOKEN_PERIOD_SECS: u64 = 4 * 60 * 60;
static ACCOUNT_TOKEN: OnceCell<String> = OnceCell::const_new();

#[derive(Debug)]
pub enum GofileLinkError {
    MissingFolderId,
    AccountRequest(reqwest::Error),
    AccountRead(reqwest::Error),
    AccountJson(serde_json::Error),
    ContentsRequest(reqwest::Error),
    ContentsRead(reqwest::Error),
    ContentsJson(serde_json::Error),
    NoFileInFolder,
    InvalidFileUrl(url::ParseError),
    InvalidCookieHeader(reqwest::header::InvalidHeaderValue),
}

/// Resolve a GoFile folder id to a direct file download URL and required headers.
pub async fn resolve_gofile_file(
    id: &str,
) -> Result<(Url, HeaderMap<HeaderValue>), GofileLinkError> {
    let token = account_token().await?;

    // Gofile replaced the old `wt` query parameter with a time-limited
    // X-Website-Token derived from the account token and request headers.
    let website_token = website_token(&token, SystemTime::now());
    let url = format!(
        "https://api.gofile.io/contents/{id}?contentFilter=&page=1&pageSize=1000&sortField=name&sortDirection=1"
    );

    // Query folder contents with Authorization
    let resp = crate::net::client()
        .get(url)
        .header("authorization", format!("Bearer {token}"))
        .header("x-website-token", website_token)
        .header("x-bl", "")
        .send()
        .await
        .map_err(GofileLinkError::ContentsRequest)?;
    let text = resp.text().await.map_err(GofileLinkError::ContentsRead)?;
    let data: GofileFiles = serde_json::from_str(&text).map_err(GofileLinkError::ContentsJson)?;

    // Pick first file child link
    let url = data
        .data
        .children
        .iter()
        .filter_map(|(_, node)| match node {
            GofileNode::File { link, .. } => Some(link.clone()),
            _ => None,
        })
        .next()
        .ok_or(GofileLinkError::NoFileInFolder)?;
    let url = Url::from_str(&url).map_err(GofileLinkError::InvalidFileUrl)?;

    let mut headers = HeaderMap::new();
    headers.append(
        "Cookie",
        HeaderValue::from_str(&format!("accountToken={token}"))
            .map_err(GofileLinkError::InvalidCookieHeader)?,
    );

    Ok((url, headers))
}

async fn account_token() -> Result<&'static str, GofileLinkError> {
    ACCOUNT_TOKEN
        .get_or_try_init(|| async {
            let response = crate::net::client()
                .post("https://api.gofile.io/accounts")
                .send()
                .await
                .map_err(GofileLinkError::AccountRequest)?;
            let body = response
                .text()
                .await
                .map_err(GofileLinkError::AccountRead)?;
            serde_json::from_str::<GofileAuth>(&body)
                .map(|auth| auth.data.token)
                .map_err(GofileLinkError::AccountJson)
        })
        .await
        .map(String::as_str)
}

fn website_token(account_token: &str, now: SystemTime) -> String {
    let time_bucket =
        now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() / WEBSITE_TOKEN_PERIOD_SECS;
    let input = format!(
        "{}::::{account_token}::{time_bucket}::{WEBSITE_TOKEN_SECRET}",
        crate::net::USER_AGENT
    );
    format!("{:x}", Sha256::digest(input.as_bytes()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn website_token_matches_gofile_generator() {
        let now = UNIX_EPOCH + Duration::from_secs(123_983 * WEBSITE_TOKEN_PERIOD_SECS);

        assert_eq!(
            website_token("test-token", now),
            "a26ce316e244cf605cf24c3fc80e999372c365b96742e29155b87765da234021"
        );
    }
}
