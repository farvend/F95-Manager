use core::fmt;
use lazy_static::lazy_static;
use regex::Regex;
use reqwest::Url;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::parser::game_info::DownloadLink;
use crate::parser::game_info::{Platform, PlatformDownloads};

lazy_static! {
    static ref RE_BR: Regex = Regex::new(r"<br\s*/?>").unwrap();
    static ref RE_DOWNLOAD_LABEL: Regex = Regex::new(r"(?i)download").unwrap();
    static ref RE_SCRIPT: Regex = Regex::new(r"(?s)<script\b[^>]*>.*?</script>").unwrap();
    static ref SEL_DOWNLOAD_BLOCK: scraper::Selector =
        scraper::Selector::parse(r#"[style="text-align: center"]"#).unwrap();
    static ref SEL_LINK: scraper::Selector = scraper::Selector::parse("a[href]").unwrap();
}

pub struct F95PageUrl(pub Url);
pub struct F95Page(pub String);

const FAILED_HTML_DIR: &str = "html_pages";

#[derive(Debug)]
pub enum GetLinksError {
    NoDownloadsBlock,
    NoPlatformLinks,
    NoSupportedHostings,
    LoginRequired,
}
impl fmt::Display for GetLinksError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GetLinksError::NoDownloadsBlock => write!(f, "Downloads block not found on page"),
            GetLinksError::NoPlatformLinks => write!(f, "No platform links found"),
            GetLinksError::NoSupportedHostings => write!(f, "No supported hostings found"),
            GetLinksError::LoginRequired => {
                write!(f, "Login required: F95zone hides download links for guests")
            }
        }
    }
}

#[derive(Debug)]
pub enum GetPageError {
    Config(crate::net::NetError),
    Request(reqwest::Error),
    /// Cloudflare answered with its "Just a moment..." challenge instead of
    /// the thread: `cf_clearance` is missing, expired, or was issued for a
    /// different User-Agent or IP address.
    CloudflareChallenge,
    /// F95 served the page to a guest: the `xf_user` login cookie is missing
    /// or expired.
    LoggedOut,
}

impl fmt::Display for GetPageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GetPageError::Config(error) => write!(f, "{error}"),
            GetPageError::Request(error) => write!(f, "Request error: {error}"),
            GetPageError::CloudflareChallenge => write!(
                f,
                "Cloudflare blocked the request: the cf_clearance cookie expired or does not \
                 match this User-Agent/IP. Copy fresh cookies and User-Agent from the browser"
            ),
            GetPageError::LoggedOut => write!(
                f,
                "F95zone session expired: the xf_user cookie is no longer valid. Log in again"
            ),
        }
    }
}

impl std::error::Error for GetPageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            GetPageError::Config(error) => Some(error),
            GetPageError::Request(error) => Some(error),
            GetPageError::CloudflareChallenge | GetPageError::LoggedOut => None,
        }
    }
}

impl F95PageUrl {
    pub async fn get_page(&self) -> Result<F95Page, GetPageError> {
        let client = crate::net::client().map_err(GetPageError::Config)?;
        let response = client
            .get(self.0.clone())
            .send()
            .await
            .map_err(GetPageError::Request)?;
        let challenged = response.headers().contains_key("cf-mitigated");
        let text = response.text().await.map_err(GetPageError::Request)?;
        check_session(challenged, &text)?;
        Ok(F95Page(text))
    }
}

/// Tells an expired cookie apart from a page the parser does not understand.
///
/// Cloudflare marks its challenge with a `cf-mitigated: challenge` header; the
/// challenge page itself is titled "Just a moment...". A real F95 page carries
/// XenForo's `data-logged-in` flag on `<html>`.
fn check_session(challenged: bool, html: &str) -> Result<(), GetPageError> {
    if challenged || html.contains("<title>Just a moment...</title>") {
        return Err(GetPageError::CloudflareChallenge);
    }
    if html.contains(r#"data-logged-in="false""#) {
        return Err(GetPageError::LoggedOut);
    }
    Ok(())
}

impl F95Page {
    pub async fn save_failed_parse_html<E: fmt::Display + ?Sized>(
        &self,
        page_url: &F95PageUrl,
        error: &E,
    ) -> std::io::Result<PathBuf> {
        let dir = PathBuf::from(FAILED_HTML_DIR);
        tokio::fs::create_dir_all(&dir).await?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        let page_name = safe_filename_part(page_url.0.path());
        let error_name = safe_filename_part(&error.to_string());
        let file_name = format!("{timestamp}_{page_name}_{error_name}.html");
        let path = dir.join(file_name);

        tokio::fs::write(&path, &self.0).await?;

        Ok(path)
    }

    pub fn get_download_links(&self) -> Result<Vec<PlatformDownloads>, GetLinksError> {
        let html = scraper::Html::parse_document(&self.0);
        // Match the label case-insensitively, but ignore inline scripts:
        // XenForo's `js-extraPhrases` JSON contains `"lightbox_download":
        // "Download"` and sits above the real DOWNLOAD block.
        let block_html = html
            .select(&SEL_DOWNLOAD_BLOCK)
            .find(|element| {
                RE_DOWNLOAD_LABEL.is_match(&RE_SCRIPT.replace_all(&element.html(), ""))
            })
            .ok_or(GetLinksError::NoDownloadsBlock)?
            .html();
        let after_label = RE_DOWNLOAD_LABEL
            .find(&block_html)
            .map(|download_label| &block_html[download_label.end()..])
            .ok_or(GetLinksError::NoDownloadsBlock)?;

        let mut downloads = Vec::new();
        let mut found_download_urls = false;

        // One `<br>`-separated line per platform: "Win: GOFILE - MEGA - ...".
        for line in RE_BR.split(after_label).skip(1) {
            let fragment = scraper::Html::parse_fragment(line);
            let platform = fragment
                .root_element()
                .text()
                .map(Platform::from)
                .find(|platform| !platform.is_empty())
                .unwrap_or(Platform::OTHER);

            let anchors: Vec<_> = fragment
                .select(&SEL_LINK)
                .filter(|anchor| !is_hidden(*anchor))
                .collect();
            found_download_urls |= !anchors.is_empty();

            let links: Vec<DownloadLink> = anchors
                .into_iter()
                .filter_map(|anchor| {
                    // `a[href]` guarantees the attribute; scraper already
                    // decodes `&amp;`, so the query string survives intact.
                    let href = anchor.value().attr("href")?;
                    let url = match Url::from_str(href) {
                        Ok(url) => url,
                        Err(error) => {
                            log::debug!("skipping non-absolute download link {href}: {error}");
                            return None;
                        }
                    };
                    let label: String = anchor.text().collect();
                    DownloadLink::new(url, &label)
                })
                .collect();

            if links.is_empty() {
                continue;
            }

            downloads.push(PlatformDownloads::new(platform, links));
        }

        if downloads.is_empty() {
            if page_has_hidden_guest_links(&self.0) {
                return Err(GetLinksError::LoginRequired);
            }
            if found_download_urls {
                return Err(GetLinksError::NoSupportedHostings);
            }
            return Err(GetLinksError::NoPlatformLinks);
        }

        Ok(downloads)
    }
}

/// F95 puts decoy links next to the real ones, hidden with `u-hidden` /
/// `aria-hidden="true"`. Following them only yields "That link is no longer in
/// the post", so they are not download links.
fn is_hidden(anchor: scraper::ElementRef<'_>) -> bool {
    let hides = |element: scraper::ElementRef<'_>| {
        element.value().attr("aria-hidden") == Some("true")
            || element.value().classes().any(|class| class == "u-hidden")
    };
    hides(anchor)
        || anchor
            .ancestors()
            .filter_map(scraper::ElementRef::wrap)
            .any(hides)
}

fn page_has_hidden_guest_links(html: &str) -> bool {
    html.contains(r#"data-logged-in="false""#)
        || html.contains("messageHide--link")
        || html.contains("You must be registered to see the links")
}

fn safe_filename_part(value: &str) -> String {
    let mut name = String::with_capacity(value.len());

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            name.push(ch);
        } else if !name.ends_with('_') {
            name.push('_');
        }
    }

    let name = name.trim_matches('_');
    if name.is_empty() {
        "page".to_string()
    } else {
        name.chars().take(120).collect()
    }
}

#[cfg(test)]
mod session_tests {
    use super::*;

    const CHALLENGE: &str = r#"<!DOCTYPE html><html lang="en-US"><head><title>Just a moment...</title></head><body><noscript>Enable JavaScript and cookies to continue</noscript></body></html>"#;

    #[test]
    fn cloudflare_challenge_is_reported_as_cookie_problem() {
        assert!(matches!(
            check_session(false, CHALLENGE),
            Err(GetPageError::CloudflareChallenge)
        ));
        // The header alone is enough, whatever the body looks like.
        assert!(matches!(
            check_session(true, "<html></html>"),
            Err(GetPageError::CloudflareChallenge)
        ));
    }

    #[test]
    fn guest_page_is_reported_as_expired_login() {
        let html = r#"<html data-logged-in="false"><head><title>Game | F95zone</title></head></html>"#;
        assert!(matches!(check_session(false, html), Err(GetPageError::LoggedOut)));
    }

    #[test]
    fn logged_in_page_passes_to_the_parser() {
        let html = r#"<html data-logged-in="true"><head><title>Game | F95zone</title></head></html>"#;
        assert!(check_session(false, html).is_ok());
    }
}
