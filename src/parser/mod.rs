// Parser for f95: fetch and parse one page with given filters.
// Public API:
//   - F95Filters: filters for the listing request (category + sort)
//   - Sorting: supported sort values
//   - F95Thread, Pagination, F95Msg: typed response structures
//   - fetch_list_page(page, &filters) -> Result<F95Msg, F95Error>
//
// Example:
// let filters = F95Filters::default().with_category("games").with_sort(Sorting::Date);
// let page = fetch_list_page(1, &filters).await?;
//
// Endpoint sample:
// https://f95zone.to/sam/latest_alpha/latest_data.php?cmd=list&cat=games&page=1&sort=date

use lazy_static::lazy_static;
use serde::{Deserialize, Deserializer};
use std::fmt;

use reqwest::header;

use crate::{
    parser::game_info::{ThreadId, cookies},
    types::{DateLimit, Sorting},
};

pub const BASE_URL: &str = "https://f95zone.to/sam/latest_alpha/latest_data.php";

lazy_static! {
    static ref CLIENT: reqwest::Client = {
        // Client build errors are rare, but should not crash the whole app.
        // Fallback to a default client and log the error.
        reqwest::Client::builder()
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:68.0) Gecko/20100101 Firefox/68.0",
            )
            .build()
            .unwrap_or_else(|e| {
                log::error!("Failed to build reqwest client, falling back to default: {e}");
                reqwest::Client::new()
            })
    };
}

pub mod game_info;

#[derive(Debug, Clone)]
pub struct F95Filters {
    /// Category, e.g. "games"
    pub category: String,
    /// Sort parameter
    pub sort: Sorting,
    /// Comma-separated tag IDs to include (API param: tags)
    pub include_tags: Vec<u32>,
    /// Comma-separated tag IDs to exclude (API param: notags)
    pub exclude_tags: Vec<u32>,
    /// Prefix IDs to include (API param: prefixes)
    pub prefixes: Vec<u32>,
    /// Prefix IDs to exclude (API param: noprefixes)
    pub noprefixes: Vec<u32>,
    /// Date filter in days back (API param: date). None = no limit
    pub date_days: Option<u32>,
    search_query: String,
}

impl Default for F95Filters {
    fn default() -> Self {
        Self {
            category: "games".to_string(),
            sort: Sorting::Date,
            include_tags: Vec::new(),
            exclude_tags: Vec::new(),
            prefixes: Vec::new(),
            noprefixes: Vec::new(),
            date_days: None,
            search_query: String::new(),
        }
    }
}

impl F95Filters {
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }
    pub fn with_sort(mut self, sort: Sorting) -> Self {
        self.sort = sort;
        self
    }
    pub fn with_include_tags(mut self, tags: Vec<u32>) -> Self {
        self.include_tags = tags;
        self
    }
    pub fn with_exclude_tags(mut self, tags: Vec<u32>) -> Self {
        self.exclude_tags = tags;
        self
    }
    pub fn with_prefixes(mut self, prefixes: Vec<u32>) -> Self {
        self.prefixes = prefixes;
        self
    }
    pub fn with_noprefixes(mut self, prefixes: Vec<u32>) -> Self {
        self.noprefixes = prefixes;
        self
    }
    pub fn with_date_days(mut self, days: Option<u32>) -> Self {
        self.date_days = days;
        self
    }
    pub fn with_date_limit(mut self, limit: DateLimit) -> Self {
        self.date_days = match limit {
            DateLimit::Anytime => None,
            DateLimit::Today => Some(1),
            DateLimit::Days3 => Some(3),
            DateLimit::Days7 => Some(7),
            DateLimit::Days14 => Some(14),
            DateLimit::Days30 => Some(30),
            DateLimit::Days90 => Some(90),
            DateLimit::Days180 => Some(180),
            DateLimit::Days365 => Some(365),
        };
        self
    }
    pub fn with_search_query(mut self, query: impl Into<String>) -> Self {
        self.search_query = query.into();
        self
    }
}

/// Normalize f95 URLs (covers/screens) to absolute form.
pub fn normalize_url(s: &str) -> String {
    if s.starts_with("http://") || s.starts_with("https://") {
        s.to_string()
    } else {
        format!("https://f95zone.to{}", s)
    }
}

const ACCEPT_IMAGE: &str = "image/jpeg,image/png,image/gif,image/webp,image/avif;q=0";
const ACCEPT_IMAGE_FALLBACK: &str = "image/jpeg,image/png,image/gif,image/webp";

async fn get_ok_image_response(
    client: &reqwest::Client,
    url: &str,
    referer: &str,
    accept: &str,
) -> Result<reqwest::Response, String> {
    let resp = client
        .get(url)
        .header(header::REFERER, referer)
        .header(header::ACCEPT, accept)
        .send()
        .await
        .map_err(|e| {
            log::warn!("fetch_image: request error for {}: {}", url, e);
            format!("request error for {}: {}", url, e)
        })?;

    let status = resp.status();
    if !status.is_success() {
        log::warn!("fetch_image: http status {} for {}", status.as_u16(), url);
        return Err(format!("http status {} for {}", status.as_u16(), url));
    }

    Ok(resp)
}

fn response_content_type(resp: &reqwest::Response) -> String {
    resp.headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned()
}

async fn response_bytes(resp: reqwest::Response, url_for_log: &str) -> Result<Vec<u8>, String> {
    resp.bytes().await.map(|b| b.to_vec()).map_err(|e| {
        log::warn!("fetch_image: body read error for {}: {}", url_for_log, e);
        format!("body read error for {}: {}", url_for_log, e)
    })
}

fn is_attachment(url: &str) -> bool {
    url.starts_with("https://attachments.f95zone.to/")
}

/// Download an image (cover/screenshot) with Referer and return RGBA8 bytes + size.
pub async fn fetch_image_f95_with_ref(
    url: &str,
    referer: &str,
) -> Result<(usize, usize, Vec<u8>), String> {
    let client = &CLIENT;
    log::debug!("fetch_image: GET {} referer={}", url, referer);

    let resp = get_ok_image_response(client, url, referer, ACCEPT_IMAGE).await?;
    let mut content_type = response_content_type(&resp);
    // Read body first (may be AVIF)
    let mut bytes = response_bytes(resp, url).await?;

    // If server forces AVIF for attachments, try preview CDN fallback which serves WebP/JPEG
    if content_type.contains("avif") && is_attachment(url) {
        let alt = url.replacen(
            "https://attachments.f95zone.to/",
            "https://preview.f95zone.to/",
            1,
        );
        log::info!(
            "fetch_image: AVIF from attachments, trying preview fallback: {}",
            alt
        );
        let resp2 = get_ok_image_response(client, &alt, referer, ACCEPT_IMAGE_FALLBACK).await?;
        content_type = response_content_type(&resp2);
        bytes = response_bytes(resp2, &alt).await?;
    }
    if content_type.contains("avif") || content_type.contains("webp") {
        log::info!(
            "fetch_image: content-type={} (modern), url={}",
            content_type,
            url
        );
    } else {
        log::debug!(
            "fetch_image: {} content-type={} size={}B",
            url,
            content_type,
            bytes.len()
        );
    }

    let img = match image::load_from_memory(&bytes) {
        Ok(i) => i,
        Err(e) => {
            let msg = format!(
                "decode error for {}: {} (content-type={})",
                url, e, content_type
            );
            log::warn!("fetch_image: {}", msg);
            return Err(msg);
        }
    };
    let rgba8 = img.to_rgba8();
    let (w, h) = rgba8.dimensions();
    Ok((w as usize, h as usize, rgba8.into_raw()))
}

/// Backwards-compatible helper that uses site root as referer.
pub async fn fetch_image_f95(url: &str) -> Result<(usize, usize, Vec<u8>), String> {
    fetch_image_f95_with_ref(url, "https://f95zone.to/").await
}

#[derive(Debug)]
pub enum F95Error {
    Reqwest(reqwest::Error),
    Api(String),
}

impl fmt::Display for F95Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            F95Error::Reqwest(e) => write!(f, "Request/Decode error: {}", e),
            F95Error::Api(msg) => write!(f, "API error: {}", msg),
        }
    }
}

impl std::error::Error for F95Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            F95Error::Reqwest(e) => Some(e),
            F95Error::Api(_) => None,
        }
    }
}

impl From<reqwest::Error> for F95Error {
    fn from(e: reqwest::Error) -> Self {
        F95Error::Reqwest(e)
    }
}

fn deserialize_version<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum VersionValue {
        Str(String),
        Num(f64),
    }

    match VersionValue::deserialize(deserializer)? {
        VersionValue::Str(s) => Ok(s),
        VersionValue::Num(n) => Ok(n.to_string()),
    }
}


#[derive(Debug, Deserialize, Clone)]
pub struct F95Thread {
    pub thread_id: ThreadId,
    pub title: String,
    pub creator: String,
    #[serde(deserialize_with = "deserialize_version")]
    pub version: String,
    pub views: u64,
    pub likes: u64,
    pub prefixes: Vec<u32>,
    pub tags: Vec<u32>,
    pub rating: f32,
    pub cover: String,
    pub screens: Vec<String>,
    pub date: String,
    pub watched: bool,
    pub ignored: bool,
    #[serde(rename = "new")]
    pub is_new: bool,
    pub ts: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Pagination {
    pub page: u32,
    pub total: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct F95Msg {
    pub data: Vec<F95Thread>,
    pub pagination: Pagination,
    pub count: u64,
}

// Top-level response may return either msg object on success or string on error.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Msg {
    Success(F95Msg),
    Error(String),
}

#[derive(Debug, Deserialize)]
struct Root {
    status: String,
    msg: Msg,
}

/// Fetch and parse one listing page from f95 with provided filters.
/// Returns the 'msg' object which contains data, pagination, and total count.
///
/// Note: uses async reqwest client. Ensure Cargo.toml enables reqwest features:
/// reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
pub async fn fetch_list_page(page: u32, filters: &F95Filters) -> Result<F95Msg, F95Error> {
    let client = &CLIENT;

    let cache_buster = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);


    // Use static keys to avoid repeated key allocations.
    // 6 base params + (include/exclude tags/prefixes) + optional date
    let mut params: Vec<(&'static str, String)> = Vec::with_capacity(
        6 + filters.include_tags.len() + filters.exclude_tags.len() + filters.prefixes.len()
            + filters.noprefixes.len()
            + usize::from(filters.date_days.is_some()),
    );

    params.push(("cmd", "list".to_string()));
    params.push(("cat", filters.category.clone()));
    params.push(("page", page.to_string()));
    params.push(("sort", filters.sort.api_value().to_string()));
    // Always send `search` param (API expects it; empty string means no search)
    params.push(("search", filters.search_query.clone()));

    let mut push_u32_array = |key: &'static str, values: &[u32]| {
        params.extend(values.iter().map(|v| (key, v.to_string())));
    };
    push_u32_array("tags[]", &filters.include_tags);
    push_u32_array("notags[]", &filters.exclude_tags);
    push_u32_array("prefixes[]", &filters.prefixes);
    push_u32_array("noprefixes[]", &filters.noprefixes);

    if let Some(d) = filters.date_days {
        params.push(("date", d.to_string()));
    }
    params.push(("_", cache_buster.to_string()));

    // Perform request, and if server responds with 429 (Too Many Requests),
    // wait 1 second before retrying once to avoid immediate hammering.
    let mut make_request = || {
        client
            .get(BASE_URL)
            .header(header::COOKIE, cookies())
            .query(&params)
            .send()
    };

    let mut raw_resp = make_request().await?;

    if raw_resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        log::warn!("fetch_list_page: received 429 Too Many Requests; delaying 1s before retry");
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        raw_resp = make_request().await?;
    }

    let raw_resp = raw_resp.error_for_status()?;
    let resp: Root = match raw_resp.json().await {
        Ok(v) => v,
        Err(err) => {
            let text = format!("Failed to parse JSON response: {err}");
            log::error!("{}", text);
            return Err(F95Error::Api(text));
        }
    };
    match resp.msg {
        Msg::Success(msg) if resp.status == "ok" => Ok(msg),
        Msg::Error(err) => Err(F95Error::Api(err)),
        _ => Err(F95Error::Api(format!("unexpected status: {}", resp.status))),
    }
}
