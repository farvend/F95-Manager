// Parser for f95: fetch and parse one page with given filters.
// Public API:
//   - F95Filters: filters for the listing request (category + sort)
//   - SortParam: supported sort values
//   - F95Thread, Pagination, F95Msg: typed response structures
//   - fetch_list_page(page, &filters) -> Result<F95Msg, F95Error>
//
// Example:
// let filters = F95Filters::default().with_category("games").with_sort(SortParam::Date);
// let page = fetch_list_page(1, &filters).await?;
//
// Endpoint sample:
// https://f95zone.to/sam/latest_alpha/latest_data.php?cmd=list&cat=games&page=1&sort=date

use lazy_static::lazy_static;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

use crate::{
    parser::game_info::{ThreadId, cookies},
    types::{DateLimit, Sorting},
};

pub const BASE_URL: &str = "https://f95zone.to/sam/latest_alpha/latest_data.php";

lazy_static! {
    static ref CLIENT: reqwest::Client = reqwest::Client::builder()
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:68.0) Gecko/20100101 Firefox/68.0"
        )
        .build()
        .unwrap();
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

//// Download an image (cover/screenshot) with Referer and return RGBA8 bytes + size.
pub async fn fetch_image_f95_with_ref(
    url: &str,
    referer: &str,
) -> Result<(usize, usize, Vec<u8>), String> {
    let client = &CLIENT;
    log::debug!("fetch_image: GET {} referer={}", url, referer);

    let resp = match client
        .get(url)
        .header("Referer", referer)
        .header(
            "Accept",
            "image/jpeg,image/png,image/gif,image/webp,image/avif;q=0",
        )
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("fetch_image: request error for {}: {}", url, e);
            return Err(format!("request error for {}: {}", url, e));
        }
    };

    let status = resp.status();
    if !status.is_success() {
        log::warn!("fetch_image: http status {} for {}", status.as_u16(), url);
        return Err(format!("http status {} for {}", status.as_u16(), url));
    }

    let mut content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();

    // Read body first (may be AVIF)
    let mut bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            log::warn!("fetch_image: body read error for {}: {}", url, e);
            return Err(format!("body read error for {}: {}", url, e));
        }
    };

    // If server forces AVIF for attachments, try preview CDN fallback which serves WebP/JPEG
    if content_type.contains("avif") && url.starts_with("https://attachments.f95zone.to/") {
        let alt = url.replacen(
            "https://attachments.f95zone.to/",
            "https://preview.f95zone.to/",
            1,
        );
        log::info!(
            "fetch_image: AVIF from attachments, trying preview fallback: {}",
            alt
        );
        match client
            .get(&alt)
            .header("Referer", referer)
            .header("Accept", "image/jpeg,image/png,image/gif,image/webp")
            .send()
            .await
        {
            Ok(r2) => {
                if !r2.status().is_success() {
                    log::warn!(
                        "fetch_image: fallback http status {} for {}",
                        r2.status().as_u16(),
                        alt
                    );
                    return Err(format!(
                        "fallback http status {} for {}",
                        r2.status().as_u16(),
                        alt
                    ));
                }
                content_type = r2
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_owned();
                bytes = match r2.bytes().await {
                    Ok(b) => b,
                    Err(e) => {
                        log::warn!("fetch_image: fallback body read error for {}: {}", alt, e);
                        return Err(format!("fallback body read error for {}: {}", alt, e));
                    }
                }
            }
            Err(e) => {
                log::warn!("fetch_image: fallback request error for {}: {}", alt, e);
                return Err(format!("fallback request error for {}: {}", alt, e));
            }
        }
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

    #[derive(Serialize, Debug)]
    struct Query<'a> {
        cmd: &'a str,
        cat: &'a str,
        page: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sort: Option<String>,
        search: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "tags[]")]
        tags: Option<Vec<u32>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        notags: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        prefixes: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        noprefixes: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        date: Option<u32>,
        #[serde(rename = "_")]
        cache_buster: u64,
    }

    let tags = if filters.include_tags.is_empty() {
        None
    } else {
        Some(filters.include_tags.clone())
    };
    let notags = if filters.exclude_tags.is_empty() {
        None
    } else {
        Some(filters.exclude_tags.clone())
    };
    let prefixes = if filters.prefixes.is_empty() {
        None
    } else {
        Some(filters.prefixes.clone())
    };
    let noprefixes = if filters.noprefixes.is_empty() {
        None
    } else {
        Some(filters.noprefixes.clone())
    };

    let cache_buster = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let mut params: Vec<(String, String)> = vec![
        ("cmd".into(), "list".into()),
        ("cat".into(), filters.category.clone()),
        ("page".into(), page.to_string()),
    ];
    // sort
    params.push(("sort".into(), filters.sort.api_value().to_string()));
    //search
    params.push(("search".into(), filters.search_query.clone()));
    // tags[] repeated
    if let Some(ts) = tags {
        for t in ts {
            params.push(("tags[]".into(), t.to_string()));
        }
    }
    // notags/prefixes/noprefixes as repeated array params
    if let Some(ns) = notags {
        for n in ns {
            params.push(("notags[]".into(), n.to_string()));
        }
    }
    if let Some(ps) = prefixes {
        for p in ps {
            params.push(("prefixes[]".into(), p.to_string()));
        }
    }
    if let Some(nps) = noprefixes {
        for np in nps {
            params.push(("noprefixes[]".into(), np.to_string()));
        }
    }
    if let Some(d) = filters.date_days {
        params.push(("date".into(), d.to_string()));
    }
    // cache buster
    params.push(("_".into(), cache_buster.to_string()));

    // Perform request, and if server responds with 429 (Too Many Requests),
    // wait 1 second before retrying once to avoid immediate hammering.
    let mut raw_resp = client
        .get(BASE_URL)
        .header("Cookie", cookies())
        .query(&params)
        .send()
        .await?;
    dbg!(1);

    if raw_resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        log::warn!("fetch_list_page: received 429 Too Many Requests; delaying 1s before retry");
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        raw_resp = client
            .get(BASE_URL)
            .header("Cookie", cookies())
            .query(&params)
            .send()
            .await?;
    }
    dbg!(2);

    let raw_resp = raw_resp.error_for_status()?;
    dbg!(3);
    // dbg!("raw response: {:?}", &raw_resp.text().await);
    let resp: Root = match raw_resp.json().await {
        Ok(v) => v,
        Err(err) => {
            let text = format!("Failed to parse JSON response: {err}");
            log::error!("{}", text);
            return Err(F95Error::Api(text));
        }
    };
    // return todo!();
    // dbg!(4);

    match resp.msg {
        Msg::Success(msg) if resp.status == "ok" => Ok(msg),
        Msg::Error(err) => Err(F95Error::Api(err)),
        _ => Err(F95Error::Api(format!("unexpected status: {}", resp.status))),
    }
}
