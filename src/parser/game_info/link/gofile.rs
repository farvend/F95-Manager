use reqwest::{
    Url,
    header::{HeaderMap, HeaderValue},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fmt,
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::OnceCell;

const WEBSITE_TOKEN_SECRET: &str = "9844d94d963d30";
const WEBSITE_TOKEN_PERIOD_SECS: u64 = 4 * 60 * 60;
/// Language sent as `X-BL` and hashed into the website token.
const WEBSITE_LANGUAGE: &str = "en-US";
static ACCOUNT_TOKEN: OnceCell<String> = OnceCell::const_new();

#[derive(Debug)]
pub enum GofileLinkError {
    Config(crate::net::NetError),
    MissingFolderId,
    AccountRequest(reqwest::Error),
    AccountRead(reqwest::Error),
    AccountJson(serde_json::Error),
    ContentsRequest(reqwest::Error),
    ContentsRead(reqwest::Error),
    ContentsJson(serde_json::Error),
    /// Gofile answered with a non-`ok` status, e.g. `error-rateLimit` or
    /// `error-notFound`.
    Api(String),
    NoFileInFolder,
    InvalidFileUrl(url::ParseError),
    InvalidCookieHeader(reqwest::header::InvalidHeaderValue),
}

impl fmt::Display for GofileLinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GofileLinkError::Config(error) => write!(f, "{error}"),
            GofileLinkError::MissingFolderId => write!(f, "Gofile link has no folder id"),
            GofileLinkError::AccountRequest(error) | GofileLinkError::AccountRead(error) => {
                write!(f, "Gofile guest account request failed: {error}")
            }
            GofileLinkError::AccountJson(error) => {
                write!(f, "Gofile guest account answer is malformed: {error}")
            }
            GofileLinkError::ContentsRequest(error) | GofileLinkError::ContentsRead(error) => {
                write!(f, "Gofile folder request failed: {error}")
            }
            GofileLinkError::ContentsJson(error) => {
                write!(f, "Gofile folder answer is malformed: {error}")
            }
            GofileLinkError::Api(status) if status == "error-rateLimit" => {
                write!(f, "Gofile is rate limiting requests; try again in a few minutes")
            }
            GofileLinkError::Api(status) => write!(f, "Gofile refused the request: {status}"),
            GofileLinkError::NoFileInFolder => write!(f, "Gofile folder has no files"),
            GofileLinkError::InvalidFileUrl(error) => {
                write!(f, "Gofile returned an invalid file URL: {error}")
            }
            GofileLinkError::InvalidCookieHeader(error) => {
                write!(f, "Gofile token cannot be sent as a cookie: {error}")
            }
        }
    }
}

impl std::error::Error for GofileLinkError {}

/// Gofile wraps every answer as `{"status": "...", "data": ...}`; `data` only
/// has the documented shape when `status` is `ok`.
fn parse_api<T: serde::de::DeserializeOwned>(
    body: &str,
    malformed: fn(serde_json::Error) -> GofileLinkError,
) -> Result<T, GofileLinkError> {
    #[derive(Deserialize)]
    struct Envelope {
        status: String,
    }
    let envelope: Envelope = serde_json::from_str(body).map_err(malformed)?;
    if envelope.status != "ok" {
        return Err(GofileLinkError::Api(envelope.status));
    }
    serde_json::from_str(body).map_err(malformed)
}

/// Resolve a GoFile folder id to a direct file download URL and required headers.
pub async fn resolve_gofile_file(
    id: &str,
) -> Result<(Url, HeaderMap<HeaderValue>), GofileLinkError> {
    let token = account_token().await?;

    // Gofile validates X-Website-Token by recomputing it from the request's
    // User-Agent and X-BL (language), so both headers must match the hash.
    let user_agent = crate::net::user_agent();
    let website_token = website_token(token, &user_agent, WEBSITE_LANGUAGE, SystemTime::now());
    let url = format!(
        "https://api.gofile.io/contents/{id}?contentFilter=&page=1&pageSize=1000&sortField=name&sortDirection=1"
    );

    // Query folder contents with Authorization
    let client = crate::net::client().map_err(GofileLinkError::Config)?;
    let resp = client
        .get(url)
        .header("authorization", format!("Bearer {token}"))
        .header("x-website-token", website_token)
        .header("x-bl", WEBSITE_LANGUAGE)
        .send()
        .await
        .map_err(GofileLinkError::ContentsRequest)?;
    let text = resp.text().await.map_err(GofileLinkError::ContentsRead)?;
    let data: GofileFiles = parse_api(&text, GofileLinkError::ContentsJson)?;

    // Pick first file child link
    let url = data
        .data
        .children
        .values()
        .filter_map(|node| match node {
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
            let client = crate::net::client().map_err(GofileLinkError::Config)?;
            let response = client
                .post("https://api.gofile.io/accounts")
                .send()
                .await
                .map_err(GofileLinkError::AccountRequest)?;
            let body = response
                .text()
                .await
                .map_err(GofileLinkError::AccountRead)?;
            parse_api::<GofileAuth>(&body, GofileLinkError::AccountJson)
                .map(|auth| auth.data.token)
        })
        .await
        .map(String::as_str)
}

/// Token Gofile's web client (`wt.obf.js`) computes for every API request:
/// `sha256("{userAgent}::{language}::{accountToken}::{4h window}::{salt}")`.
fn website_token(account_token: &str, user_agent: &str, language: &str, now: SystemTime) -> String {
    let time_bucket =
        now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() / WEBSITE_TOKEN_PERIOD_SECS;
    let input = format!(
        "{user_agent}::{language}::{account_token}::{time_bucket}::{WEBSITE_TOKEN_SECRET}"
    );
    format!("{:x}", Sha256::digest(input.as_bytes()))
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct GofileAuth {
    status: String,
    data: GofileAuthData,
}
#[derive(serde::Deserialize)]
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
struct GofileMetadata {
    total_count: u32,
    total_pages: u32,
    page: u32,
    page_size: u32,
    has_next_page: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
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
#[allow(dead_code)]
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
            website_token(
                "test-token",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:68.0) Gecko/20100101 Firefox/68.0",
                "en-US",
                now
            ),
            "3fc045ad7661b1b730f27572d4f5a7cb43465bf255e581ba66a407dbe5947976"
        );
    }
}
