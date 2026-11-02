//! F95zone download pages (`/dl/<post>/?...`).
//!
//! F95 no longer links to hostings directly. A download entry points to a page
//! that either
//! - runs a script check: the page carries `data-code`, and the host URL comes
//!   back after a POST with the decoded code (see `page.js`), or
//! - asks for an hCaptcha (`f95dl-hcaptcha`), which a plain HTTP client cannot
//!   solve.
//!
//! Only the script-check path is handled here. A captcha page is reported as an
//! error so the user sees why the download stopped.
//!
//! Session cookies (`xf_session`, `xf_csrf`) set by the first GET are kept by
//! the shared client's cookie jar and sent back on the POST; a hardcoded
//! `Cookie:` header would lose them and get "Cookies are required to use this
//! site" in return.

use std::{fmt, str::FromStr, time::Duration};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use lazy_static::lazy_static;
use reqwest::Url;
use scraper::{Html, Selector};

/// `page.js` hands the code back after 2.1s; the server wants more than one
/// whole second between the page load and the form submission.
const SCRIPT_CHECK_DELAY: Duration = Duration::from_millis(2100);

const LINK_GONE_NOTICE: &str = "That link is no longer in the post";
const CAPTCHA_MARKER: &str = "f95dl-hcaptcha";

lazy_static! {
    static ref SEL_CODE: Selector = Selector::parse("[data-code]").expect("data-code selector");
    static ref SEL_GO: Selector =
        Selector::parse("[data-xf-init='f95dl-go']").expect("f95dl-go selector");
    static ref SEL_TOKEN: Selector =
        Selector::parse("input[name='_xfToken']").expect("xfToken selector");
    static ref SEL_NOTICE: Selector =
        Selector::parse(".p-body-pageContent .blockMessage").expect("notice selector");
}

#[derive(Debug)]
pub enum F95DlError {
    Config(crate::net::NetError),
    Request(reqwest::Error),
    InvalidUrl(url::ParseError),
    /// `/dl/...` link without the post id the page needs as `Referer`.
    MissingPostId,
    /// The page advertises a script check but has no `_xfToken` to submit.
    MissingToken,
    /// Neither a script check nor a captcha: nothing the client can act on.
    NoScriptCode,
    /// `data-code` is not the expected `"c" + base64(reversed answer)` shape.
    InvalidScriptCode,
    /// The page demands hCaptcha; it has to be solved in a real browser.
    CaptchaRequired,
    /// F95 reports the link is no longer in the post.
    LinkGone,
    /// The POST came back without a host URL and without a known error marker.
    ScriptCheckRejected,
    /// F95 answered with a message page instead of a download page, e.g.
    /// "Please open this download link from the post".
    PageNotice(String),
}

impl fmt::Display for F95DlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            F95DlError::Config(error) => write!(f, "{error}"),
            F95DlError::Request(error) => write!(f, "F95 download page request error: {error}"),
            F95DlError::InvalidUrl(error) => write!(f, "F95 download page URL is invalid: {error}"),
            F95DlError::MissingPostId => write!(f, "F95 download link has no post id"),
            F95DlError::MissingToken => write!(
                f,
                "F95 download page has no _xfToken; reload the thread and try again"
            ),
            F95DlError::NoScriptCode => write!(
                f,
                "F95 download page has neither a script check code nor a captcha"
            ),
            F95DlError::InvalidScriptCode => write!(
                f,
                "F95 script check code is malformed; the download page format changed"
            ),
            F95DlError::CaptchaRequired => write!(
                f,
                "F95 requires an hCaptcha for this link; pass it in the browser and retry"
            ),
            F95DlError::LinkGone => write!(f, "F95 reports the link is no longer in the post"),
            F95DlError::ScriptCheckRejected => write!(
                f,
                "F95 rejected the script check answer; the download page format changed"
            ),
            F95DlError::PageNotice(notice) => write!(f, "F95 refused the download page: {notice}"),
        }
    }
}

impl std::error::Error for F95DlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            F95DlError::Config(error) => Some(error),
            F95DlError::Request(error) => Some(error),
            F95DlError::InvalidUrl(error) => Some(error),
            F95DlError::MissingPostId
            | F95DlError::MissingToken
            | F95DlError::NoScriptCode
            | F95DlError::InvalidScriptCode
            | F95DlError::CaptchaRequired
            | F95DlError::LinkGone
            | F95DlError::ScriptCheckRejected
            | F95DlError::PageNotice(_) => None,
        }
    }
}

impl From<reqwest::Error> for F95DlError {
    fn from(error: reqwest::Error) -> Self {
        F95DlError::Request(error)
    }
}

/// Resolve an F95 download page to the hosting URL it points at.
///
/// `page_url` is the `/dl/<post>/?u=...` link taken from a thread post.
pub async fn resolve_host_url(page_url: &Url) -> Result<Url, F95DlError> {
    let client = crate::net::client().map_err(F95DlError::Config)?;

    let check_html = client
        .get(page_url.clone())
        .header("Referer", post_url(page_url)?.as_str())
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let check_page = DownloadPage::parse(&check_html)?;
    check_page.ensure_not_blocked()?;
    let check = match &check_page.script_check {
        Some(check) => check.clone(),
        None => return Err(check_page.unusable()),
    };

    tokio::time::sleep(SCRIPT_CHECK_DELAY).await;

    let go_html = client
        .post(page_url.clone())
        .header("Referer", page_url.as_str())
        .header("Origin", origin_of(page_url)?.as_str())
        .form(&[
            ("_xfToken", check.token.as_str()),
            ("had_check", "0"),
            ("w", decode_script_code(&check.code)?.as_str()),
        ])
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let go_page = DownloadPage::parse(&go_html)?;

    if let Some(host_url) = go_page.host_url {
        return Ok(host_url);
    }
    go_page.ensure_not_blocked()?;
    match go_page.notice {
        Some(notice) => Err(F95DlError::PageNotice(notice)),
        None => Err(F95DlError::ScriptCheckRejected),
    }
}

/// `https://f95zone.to` derived from the link instead of being hardcoded.
fn origin_of(page_url: &Url) -> Result<Url, F95DlError> {
    page_url.join("/").map_err(F95DlError::InvalidUrl)
}

/// Thread post the download page belongs to, used as `Referer`.
///
/// The path segment of `/dl/...` is the post id, readable at `/posts/<id>/`.
fn post_url(page_url: &Url) -> Result<Url, F95DlError> {
    let mut segments = page_url
        .path_segments()
        .ok_or(F95DlError::MissingPostId)?;
    let post_id = match segments.next() {
        Some("dl") => segments.next(),
        Some(_) | None => None,
    }
    .filter(|id| !id.is_empty())
    .ok_or(F95DlError::MissingPostId)?;
    page_url
        .join(&format!("/posts/{post_id}/"))
        .map_err(F95DlError::InvalidUrl)
}

/// Decode the value handed back as `w` in the script check form.
///
/// `page.js` stores it as `"c" + base64(reversed answer)`; the server compares
/// the reversed result with the code it generated.
pub fn decode_script_code(hidden: &str) -> Result<String, F95DlError> {
    let payload = hidden.strip_prefix('c').ok_or(F95DlError::InvalidScriptCode)?;
    let bytes = BASE64
        .decode(payload)
        .map_err(|_| F95DlError::InvalidScriptCode)?;
    let text = String::from_utf8(bytes).map_err(|_| F95DlError::InvalidScriptCode)?;
    Ok(text.chars().rev().collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptCheck {
    pub token: String,
    pub code: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct DownloadPage {
    pub script_check: Option<ScriptCheck>,
    pub has_captcha: bool,
    pub host_url: Option<Url>,
    pub gone: bool,
    /// Main message of an XenForo message page, whitespace-collapsed.
    pub notice: Option<String>,
}

impl DownloadPage {
    pub fn parse(html: &str) -> Result<Self, F95DlError> {
        let document = Html::parse_document(html);
        let code = attribute(&document, &SEL_CODE, "data-code");
        let token = attribute(&document, &SEL_TOKEN, "value");
        if code.is_some() && token.is_none() {
            return Err(F95DlError::MissingToken);
        }
        let script_check = match (token, code) {
            (Some(token), Some(code)) => Some(ScriptCheck { token, code }),
            _ => None,
        };
        let host_url = match attribute(&document, &SEL_GO, "data-url")
            .filter(|raw| !raw.is_empty())
        {
            Some(raw) => Some(Url::from_str(&raw).map_err(F95DlError::InvalidUrl)?),
            None => None,
        };
        let notice = document
            .select(&SEL_NOTICE)
            .map(|element| {
                element
                    .text()
                    .flat_map(str::split_whitespace)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .find(|text| !text.is_empty());

        Ok(Self {
            script_check,
            has_captcha: html.contains(CAPTCHA_MARKER),
            host_url,
            gone: html.contains(LINK_GONE_NOTICE),
            notice,
        })
    }

    /// Captcha and dead-link states the client cannot get past on its own.
    fn ensure_not_blocked(&self) -> Result<(), F95DlError> {
        if self.has_captcha {
            return Err(F95DlError::CaptchaRequired);
        }
        if self.gone {
            return Err(F95DlError::LinkGone);
        }
        Ok(())
    }

    /// Why a page without a script check cannot be continued: the server's own
    /// message when it shows one, otherwise the generic error.
    fn unusable(&self) -> F95DlError {
        match &self.notice {
            Some(notice) => F95DlError::PageNotice(notice.clone()),
            None => F95DlError::NoScriptCode,
        }
    }
}

fn attribute(document: &Html, selector: &Selector, attribute_name: &str) -> Option<String> {
    document
        .select(selector)
        .find_map(|element| element.value().attr(attribute_name))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::game_info::DownloadLink;

    const CHECK_PAGE: &str = r#"
        <html data-template="f95dl_check">
        <div class="f95dl-page" data-code="cRXJ3ak5COXh4NXRvMWY0cC4yMzgxMDQzOTcx"></div>
        <form action="/dl/21783444/?u=1&amp;k=2&amp;r=3&amp;t=4" method="post">
            <input type="hidden" name="_xfToken" value="1793401832,7631e0cfbb7c58d4cc31c31e6467e65a" />
            <input type="hidden" name="had_check" value="0" />
            <input type="hidden" name="w" value="" class="js-f95dlCode" />
        </form>
        </html>
    "#;

    const GO_PAGE: &str = r#"
        <div class="blockMessage f95dl-going" data-xf-init="f95dl-go"
             data-url="https://mega.nz/file/mUFyCDZZ#-9A47OMGtkKqfxQyPCOvrh9" role="status"></div>
    "#;

    const CAPTCHA_PAGE: &str = r#"
        <div class="f95dl-check-box" data-xf-init="f95dl-hcaptcha"
             data-sitekey="c3266d70-c784-4c86-9842-af98deb61432"></div>
    "#;

    const GONE_PAGE: &str = r#"
        <div class="blockMessage">That link is no longer in the post. Here's
        <a href="/posts/21981509/">the post</a> as it is now.</div>
    "#;

    /// Real pages captured from `/dl/` live under `page/pages/` (git-ignored,
    /// together with the thread pages used by `test_all_pages`).
    #[allow(dead_code)]
    fn fixture(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/parser/game_info/page/pages")
            .join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
    }

    #[test]
    fn decodes_script_check_code() {
        // Values captured from real `/dl/` pages: `data-code` is
        // `"c" + base64(reversed answer)`.
        assert_eq!(
            decode_script_code("cRXJ3ak5COXh4NXRvMWY0cC4yMzgxMDQzOTcx").expect("code"),
            "1793401832.p4f1ot5xx9BNjwrE"
        );
        assert_eq!(
            decode_script_code("ceF9maXVicmNFWTVJeENHUC4yNzgxMDQzOTcx").expect("code"),
            "1793401872.PGCxI5YEcrbuif_x"
        );
    }

    #[test]
    fn rejects_malformed_script_code() {
        assert!(matches!(
            decode_script_code("not-base64"),
            Err(F95DlError::InvalidScriptCode)
        ));
        assert!(matches!(
            decode_script_code("c!!!"),
            Err(F95DlError::InvalidScriptCode)
        ));
    }

    #[test]
    fn parses_check_page() {
        let page = DownloadPage::parse(CHECK_PAGE).expect("check page");

        let check = page.script_check.expect("script check");
        assert_eq!(check.code, "cRXJ3ak5COXh4NXRvMWY0cC4yMzgxMDQzOTcx");
        assert_eq!(
            check.token,
            "1793401832,7631e0cfbb7c58d4cc31c31e6467e65a"
        );
        assert!(!page.has_captcha);
        assert!(page.host_url.is_none());
    }

    #[test]
    fn parses_host_url_page() {
        let page = DownloadPage::parse(GO_PAGE).expect("go page");

        let host = page.host_url.expect("host url");
        assert_eq!(
            host.as_str(),
            "https://mega.nz/file/mUFyCDZZ#-9A47OMGtkKqfxQyPCOvrh9"
        );
        assert!(page.script_check.is_none());
    }

    #[test]
    fn detects_captcha_page() {
        let page = DownloadPage::parse(CAPTCHA_PAGE).expect("captcha page");

        assert!(page.has_captcha);
        assert!(matches!(
            page.ensure_not_blocked(),
            Err(F95DlError::CaptchaRequired)
        ));
    }

    #[test]
    fn detects_gone_link_page() {
        let page = DownloadPage::parse(GONE_PAGE).expect("gone page");

        assert!(page.gone);
        assert!(matches!(
            page.ensure_not_blocked(),
            Err(F95DlError::LinkGone)
        ));
    }

    #[test]
    fn missing_token_is_reported() {
        let html = r#"<div data-code="cRXJ3ak5COXh4NXRvMWY0cC4yMzgxMDQzOTcx"></div>"#;

        assert!(matches!(
            DownloadPage::parse(html),
            Err(F95DlError::MissingToken)
        ));
    }

    #[test]
    fn message_page_reports_server_notice() {
        // What F95 answers when the /dl/ link loses its query string.
        let html = r#"
            <div class="p-body-pageContent">
                <div class="blockMessage">
                    Please open this download link from <a href="/posts/22078057/">the post</a>.
                </div>
            </div>
        "#;
        let page = DownloadPage::parse(html).expect("message page");

        assert!(page.ensure_not_blocked().is_ok());
        match page.unusable() {
            F95DlError::PageNotice(notice) => {
                assert_eq!(notice, "Please open this download link from the post .")
            }
            other => panic!("expected PageNotice, got {other:?}"),
        }
    }

    #[test]
    fn builds_post_referer() {
        let page_url = Url::from_str(
            "https://f95zone.to/dl/21783444/?u=4062081&k=17ab55cf329e&r=1793401643&t=hN7BjNgMGVZfqokd",
        )
        .expect("page url");

        assert_eq!(
            post_url(&page_url).expect("post url").as_str(),
            "https://f95zone.to/posts/21783444/"
        );
        assert_eq!(
            origin_of(&page_url).expect("origin").as_str(),
            "https://f95zone.to/"
        );
    }

    #[test]
    fn download_page_url_becomes_f95_link() {
        let url = Url::from_str(
            "https://f95zone.to/dl/21687385/?u=4062081&k=52d16c17b507&r=1793403860&t=cN1Qqw-a9RRyNABx",
        )
        .expect("page url");

        let link = DownloadLink::new(url, " MEGA ").expect("download link");
        match link {
            DownloadLink::F95Page(page) => {
                assert_eq!(page.host, "MEGA");
                assert_eq!(page.url.query(), Some("u=4062081&k=52d16c17b507&r=1793403860&t=cN1Qqw-a9RRyNABx"));
            }
            other => panic!("expected F95Page, got {other:?}"),
        }
    }

    #[test]
    fn thread_url_is_not_a_download_page() {
        let url = Url::from_str("https://f95zone.to/threads/some-game.315720/").expect("url");

        assert!(DownloadLink::new(url, "thread").is_none());
    }

    #[test]
    #[ignore = "Requires captured /dl/ pages under src/parser/game_info/page/pages"]
    fn parses_captured_script_check_page() {
        let page = DownloadPage::parse(&fixture("dl_check_with_captcha.html")).expect("page");

        assert!(page.has_captcha);
        assert!(!page.gone);
        let check = page.script_check.as_ref().expect("script check");
        assert!(check.token.contains(','));
        assert!(check.code.starts_with('c'));
        assert_eq!(
            decode_script_code(&check.code).expect("code"),
            "1793403867.P7bHmQ3BuATNRVbz"
        );
        assert!(matches!(
            page.ensure_not_blocked(),
            Err(F95DlError::CaptchaRequired)
        ));
    }

    #[test]
    #[ignore = "Requires captured /dl/ pages under src/parser/game_info/page/pages"]
    fn parses_captured_bzzhr_page() {
        let page = DownloadPage::parse(&fixture("dl_check_with_captcha_bzzhr.html")).expect("page");

        assert!(page.has_captcha);
        assert!(page.script_check.is_some());
        assert!(page.host_url.is_none());
    }

    #[test]
    #[ignore = "Requires captured /dl/ pages under src/parser/game_info/page/pages"]
    fn parses_captured_gone_link_page() {
        let page = DownloadPage::parse(&fixture("dl_link_gone.html")).expect("page");

        assert!(page.gone);
        assert!(page.script_check.is_none());
        assert!(matches!(
            page.ensure_not_blocked(),
            Err(F95DlError::LinkGone)
        ));
    }

    #[test]
    #[ignore = "Requires captured /dl/ pages under src/parser/game_info/page/pages"]
    fn parses_captured_cookie_error_page() {
        let page = DownloadPage::parse(&fixture("dl_error_cookies_required.html")).expect("page");

        assert!(page.script_check.is_none());
        assert!(!page.has_captcha);
        assert!(!page.gone);
        assert!(page.ensure_not_blocked().is_ok());
    }
}
