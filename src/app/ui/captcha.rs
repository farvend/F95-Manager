//! Interactive fallback for F95 download pages that demand an hCaptcha.
//!
//! A small window with an embedded browser (WebView2 on Windows) opens the same
//! `/dl/` page. The user solves the check there; F95 then answers with its
//! `f95dl-go` page, and the hosting URL is taken from that page instead of
//! letting it navigate away. The rest of the download runs as usual.
//!
//! Everything that touches the window or the browser runs on the Slint event
//! loop thread; `solve` only hands the request over and awaits the result.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use reqwest::Url;
use serde::Deserialize;
use slint::ComponentHandle;
use slint::winit_030::{WinitWindowAccessor, winit};
use tokio::sync::oneshot;
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::{NewWindowResponse, Rect, WebView, WebViewBuilder};

use super::CaptchaWindow;

const F95_HOST: &str = "f95zone.to";

/// Cloudflare's cookies (`cf_clearance`, `cf_chl_*`, `__cf_bm`, ...). They
/// belong to the browser that passed the check: when the embedded browser
/// passes it again, its set replaces the stored one as a whole, so no
/// leftover from an older challenge travels with the new clearance.
pub fn is_cloudflare_cookie(name: &str) -> bool {
    name.starts_with("cf_") || name.starts_with("__cf")
}

/// Runs in the main frame of every page the check window loads.
///
/// - Reports the page to Rust: the hosting URL from F95's `f95dl-go` page, or
///   the notice of a XenForo message page (dead or misused link).
/// - Continues on its own once the hCaptcha is solved. F95 renders the widget
///   without a `callback`, so the user would otherwise have to press
///   "Continue". The wrapper adds one that submits the check form through
///   `requestSubmit`, i.e. exactly like the button: F95's `page.js` still
///   fills in the script check code and waits for it before sending.
const PAGE_SCRIPT: &str = r#"
(function () {
  function post(message) { window.ipc.postMessage(JSON.stringify(message)); }

  function inspect() {
    var go = document.querySelector("[data-xf-init='f95dl-go']");
    if (go && go.getAttribute('data-url')) {
      post({ kind: 'host-url', url: go.getAttribute('data-url') });
      return;
    }
    var root = document.documentElement;
    if (root && root.getAttribute('data-template') === 'message_page') {
      var notice = document.querySelector('.p-body-pageContent .blockMessage');
      if (notice) {
        post({ kind: 'notice', text: notice.textContent.replace(/\s+/g, ' ').trim() });
      }
    }
  }

  function continueAfterCaptcha(box) {
    var form = box && box.closest ? box.closest('form') : null;
    var button = document.querySelector('.js-f95dlContinue');
    if (!form) { form = button ? button.form : null; }
    if (!form) { return; }
    if (form.requestSubmit) { form.requestSubmit(button || undefined); }
    else if (button) { button.click(); }
  }

  function wrap(api) {
    if (!api || !api.render || api.__f95AutoContinue) { return api; }
    var render = api.render;
    api.render = function (box, options) {
      var params = Object.assign({}, options || {});
      var original = params.callback;
      params.callback = function (token) {
        if (typeof original === 'function') { original(token); }
        continueAfterCaptcha(box);
      };
      return render.call(this, box, params);
    };
    api.__f95AutoContinue = true;
    return api;
  }

  // F95 loads hCaptcha with `onload=F95dlHcaptchaLoaded` and only calls
  // `render` from that callback, so wrap the API there.
  //
  // Cloudflare's challenge page inspects `window` and never lets a browser
  // through once any of its properties is redefined, so nothing is touched
  // until the document is a real F95 page (XenForo's `XF` exists only there).
  // F95's script may still define the callback later, hence the setter.
  function hookHcaptchaOnload() {
    if (!window.XF) { return; }
    var onload;
    function hooked(value) {
      return typeof value !== 'function' ? value : function () {
        wrap(window.hcaptcha);
        return value.apply(this, arguments);
      };
    }
    var existing = window.F95dlHcaptchaLoaded;
    Object.defineProperty(window, 'F95dlHcaptchaLoaded', {
      configurable: true,
      get: function () { return onload; },
      set: function (value) { onload = hooked(value); }
    });
    if (existing !== undefined) { onload = hooked(existing); }
  }

  function onReady() {
    hookHcaptchaOnload();
    inspect();
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', onReady);
  } else {
    onReady();
  }
})();
"#;

#[derive(Debug)]
pub enum CaptchaError {
    /// The user closed the window or pressed Cancel.
    Cancelled,
    /// The window went away without an answer (application shutting down).
    Abandoned,
    EventLoop(slint::EventLoopError),
    Window(slint::PlatformError),
    /// The Slint window has no native window to embed the browser into.
    NoNativeWindow,
    WebView(wry::Error),
    InvalidHostUrl(url::ParseError),
    /// F95 showed a message page instead of the download page.
    PageNotice(String),
    /// The browser passed Cloudflare, but the page is still a guest page.
    LoggedOut,
    /// Cloudflare is still showing its challenge after the browser reported
    /// the page as loaded.
    StillChallenged,
}

/// A browser session, captured after the embedded browser passed Cloudflare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserSession {
    /// `Cookie:` header value for F95zone requests.
    pub cookies: String,
    /// User-Agent of the browser that Cloudflare issued `cf_clearance` for.
    pub user_agent: String,
}

impl fmt::Display for CaptchaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptchaError::Cancelled => write!(f, "the check window was closed"),
            CaptchaError::Abandoned => write!(f, "the check window closed without an answer"),
            CaptchaError::EventLoop(error) => write!(f, "cannot open the check window: {error}"),
            CaptchaError::Window(error) => write!(f, "cannot open the check window: {error}"),
            CaptchaError::NoNativeWindow => {
                write!(f, "cannot open the check window: no native window")
            }
            CaptchaError::WebView(error) => write!(f, "embedded browser failed: {error}"),
            CaptchaError::InvalidHostUrl(error) => {
                write!(f, "F95 returned an invalid hosting URL: {error}")
            }
            CaptchaError::PageNotice(notice) => write!(f, "F95 refused the download page: {notice}"),
            CaptchaError::LoggedOut => write!(
                f,
                "the F95zone login cookie is no longer valid: log in on the opened page and retry"
            ),
            CaptchaError::StillChallenged => {
                write!(f, "Cloudflare is still checking the page; try again")
            }
        }
    }
}

impl std::error::Error for CaptchaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CaptchaError::EventLoop(error) => Some(error),
            CaptchaError::Window(error) => Some(error),
            CaptchaError::WebView(error) => Some(error),
            CaptchaError::InvalidHostUrl(error) => Some(error),
            CaptchaError::Cancelled
            | CaptchaError::Abandoned
            | CaptchaError::NoNativeWindow
            | CaptchaError::PageNotice(_)
            | CaptchaError::LoggedOut
            | CaptchaError::StillChallenged => None,
        }
    }
}

/// What the check window was opened for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Goal {
    /// F95's hCaptcha on a `/dl/` page: wait for the hosting URL it hands out.
    HostingLink,
    /// Cloudflare challenged the plain HTTP request: let the browser pass it
    /// and take its cookies, so the app's own requests work again.
    Session,
}

type Outcome = Result<Url, CaptchaError>;

/// Open the check window for `page_url` and wait until the user passes it.
///
/// `host` is the hosting name shown on the link (e.g. "GOFILE"), used in the
/// window's hint. Returns the hosting URL F95 hands out after the check.
pub async fn solve(page_url: Url, host: String) -> Outcome {
    match run(page_url, host, Goal::HostingLink).await? {
        ReplyValue::HostingLink(url) => Ok(url),
        ReplyValue::Session(_) => Err(CaptchaError::Abandoned),
    }
}

/// Open `page_url` in the embedded browser and wait for Cloudflare to let the
/// real page through. Returns the browser's cookies and User-Agent.
pub async fn refresh_session(page_url: Url) -> Result<BrowserSession, CaptchaError> {
    match run(page_url, String::new(), Goal::Session).await? {
        ReplyValue::Session(session) => Ok(session),
        ReplyValue::HostingLink(_) => Err(CaptchaError::Abandoned),
    }
}

async fn run(page_url: Url, host: String, goal: Goal) -> Result<ReplyValue, CaptchaError> {
    let (reply, answer) = oneshot::channel();
    slint::invoke_from_event_loop(move || open(page_url, host, goal, reply))
        .map_err(CaptchaError::EventLoop)?;
    match answer.await {
        Ok(outcome) => outcome,
        Err(_) => Err(CaptchaError::Abandoned),
    }
}
type Reply = oneshot::Sender<Result<ReplyValue, CaptchaError>>;

enum ReplyValue {
    HostingLink(Url),
    Session(BrowserSession),
}

struct Session {
    window: CaptchaWindow,
    webview: Option<WebView>,
    goal: Goal,
    reply: Reply,
}

thread_local! {
    static SESSIONS: RefCell<HashMap<u64, Session>> = RefCell::new(HashMap::new());
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
}

fn open(page_url: Url, host: String, goal: Goal, reply: Reply) {
    let window = match CaptchaWindow::new() {
        Ok(window) => window,
        Err(error) => {
            let _ = reply.send(Err(CaptchaError::Window(error)));
            return;
        }
    };
    let id = NEXT_ID.with(|next| {
        let id = next.get();
        next.set(id + 1);
        id
    });

    window.set_window_title(crate::localization::translate("captcha-window-title").into());
    let hint_key = match goal {
        Goal::HostingLink => "captcha-hint",
        Goal::Session => "captcha-session-hint",
    };
    window.set_hint(crate::localization::translate_with(hint_key, &[("host", host)]).into());
    window.set_cancel_text(crate::localization::translate("captcha-cancel").into());
    window.on_cancel(move || finish_later(id, Err(CaptchaError::Cancelled)));
    window.window().on_close_requested(move || {
        finish_later(id, Err(CaptchaError::Cancelled));
        slint::CloseRequestResponse::HideWindow
    });
    window.on_resized(move |width, height| resize(id, width, height));

    if let Err(error) = window.show() {
        let _ = reply.send(Err(CaptchaError::Window(error)));
        return;
    }
    SESSIONS.with_borrow_mut(|sessions| {
        sessions.insert(
            id,
            Session {
                window,
                webview: None,
                goal,
                reply,
            },
        )
    });
    // The native window only exists once the event loop has handled `show()`.
    slint::Timer::single_shot(Duration::ZERO, move || attach_webview(id, page_url));
}

fn attach_webview(id: u64, page_url: Url) {
    let built = SESSIONS.with_borrow(|sessions| {
        let session = sessions.get(&id)?;
        let window = session.window.window();
        let size = window.size().to_logical(window.scale_factor());
        let header = session.window.get_header_height();
        let bounds = browser_bounds(header, size.width, size.height);
        Some((session.goal, window.with_winit_window(|native| build_webview(native, id, bounds, session.goal))))
    });
    let (goal, result) = match built {
        // Cancelled before the browser was attached.
        None => return,
        Some((goal, result)) => (goal, result),
    };
    let result = match result {
        None => Err(CaptchaError::NoNativeWindow),
        Some(Err(error)) => Err(CaptchaError::WebView(error)),
        Some(Ok(webview)) => Ok(webview),
    };
    let webview = match result.and_then(|webview| load_page(webview, &page_url)) {
        Ok(webview) => webview,
        Err(error) => {
            finish(id, Err(error));
            return;
        }
    };
    SESSIONS.with_borrow_mut(|sessions| {
        if let Some(session) = sessions.get_mut(&id) {
            session.webview = Some(webview);
        }
    });
    if goal == Goal::Session {
        slint::Timer::single_shot(POLL_INTERVAL, move || poll_session(id, 0));
    }
}

fn build_webview(
    native: &winit::window::Window,
    id: u64,
    bounds: Rect,
    goal: Goal,
) -> Result<WebView, wry::Error> {
    // Incognito: WebView2's persistent profile kept a cf_clearance from a
    // half-finished challenge, and Cloudflare then looped its interstitial
    // forever. A fresh store per window passes the check in a few seconds.
    let builder = WebViewBuilder::new()
        .with_bounds(bounds)
        .with_incognito(true)
        .with_navigation_handler(|url| stays_on_f95(&url))
        // Ads and "open in new tab" links are not part of the check.
        .with_new_window_req_handler(|_, _| NewWindowResponse::Deny);

    if goal == Goal::Session {
        // Cloudflare's own script must see a plain browser: no injected script.
        return builder.build_as_child(native);
    }

    builder
        .with_initialization_script_for_main_only(PAGE_SCRIPT, true)
        .with_ipc_handler(move |request| on_page_message(id, request.body()))
        .with_url("about:blank")
        .build_as_child(native)
}

/// Open the page in the embedded browser with the stored F95 cookies set.
///
/// The account cookie opens the thread logged in. The stored `cf_clearance`
/// was earned by this same embedded browser (see `update_cookies_from_browser`),
/// so while it is valid Cloudflare lets the window straight through instead of
/// starting its check again; an expired one just gets replaced by the check.
/// Session mode loads the page with no injected script at all, so
/// Cloudflare's own JavaScript runs in an untouched browser.
fn load_page(webview: WebView, page_url: &Url) -> Result<WebView, CaptchaError> {
    if let Some(cookies) = crate::app::config::configured_cookies() {
        for pair in cookies.split(';') {
            let Some((name, value)) = pair.split_once('=') else {
                continue;
            };
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            let cookie = cookie::Cookie::build((name.to_string(), value.trim().to_string()))
                .domain(F95_HOST)
                .path("/")
                .secure(true)
                .http_only(true)
                .build();
            webview.set_cookie(&cookie).map_err(CaptchaError::WebView)?;
        }
    }
    webview
        .load_url(page_url.as_str())
        .map_err(CaptchaError::WebView)?;
    Ok(webview)
}

const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// About a minute: Cloudflare usually lets the browser through in seconds.
const POLL_ATTEMPTS: u32 = 120;

/// Reports `[data-logged-in, userAgent]`. Only a real F95 page carries
/// `data-logged-in`; Cloudflare's interstitial does not.
const PAGE_STATE_SCRIPT: &str =
    "JSON.stringify([document.documentElement.getAttribute('data-logged-in'), navigator.userAgent])";

/// In session mode there is no injected script, so ask the page directly.
///
/// The check is over once a real F95 page is shown, not when `cf_clearance`
/// appears: Cloudflare sets and replaces that cookie while its challenge is
/// still running.
///
/// The next poll is scheduled here, not from the script callback: WebView2
/// drops a script that runs while the page navigates (Cloudflare reloads the
/// page right after its check), and wry then never calls the callback.
fn poll_session(id: u64, attempts: u32) {
    let asked = SESSIONS.with_borrow(|sessions| {
        let webview = sessions.get(&id)?.webview.as_ref()?;
        Some(webview.evaluate_script_with_callback(PAGE_STATE_SCRIPT, move |value| {
            on_page_state(id, &value)
        }))
    });
    match asked {
        // The window is gone.
        None => {}
        Some(Err(error)) => finish_later(id, Err(CaptchaError::WebView(error))),
        Some(Ok(())) if attempts >= POLL_ATTEMPTS => {
            finish_later(id, Err(CaptchaError::StillChallenged))
        }
        Some(Ok(())) => {
            slint::Timer::single_shot(POLL_INTERVAL, move || poll_session(id, attempts + 1))
        }
    }
}

fn on_page_state(id: u64, value: &str) {
    log::debug!("check window: page state {value}");
    // WebView2 hands back an empty string when the script ran into a page
    // that was being replaced (Cloudflare reloads right after its check);
    // the next poll asks again.
    if value.is_empty() {
        return;
    }
    // The result arrives as JSON, i.e. a JSON string holding the script's
    // own JSON.
    let state = serde_json::from_str::<String>(value).and_then(|inner| {
        serde_json::from_str::<(Option<String>, String)>(&inner)
    });
    let (logged_in, user_agent) = match state {
        Ok(state) => state,
        Err(error) => {
            log::warn!("check window: unexpected page state {value:?}: {error}");
            return;
        }
    };
    // No `data-logged-in`: still Cloudflare's page.
    let Some(logged_in) = logged_in else {
        return;
    };
    // This runs inside WebView2's script completion handler. Reading the
    // cookie store waits for another WebView2 callback, which WebView2 never
    // delivers while a handler is running, so leave it first.
    slint::Timer::single_shot(Duration::ZERO, move || {
        let outcome = harvest_session(id, logged_in == "true", user_agent);
        finish(id, outcome.map(ReplyValue::Session));
    });
}

/// Keep the browser on F95. The final redirect to the hosting is blocked on
/// purpose: its URL is read from the page and the app downloads it itself.
fn stays_on_f95(url: &str) -> bool {
    let Ok(url) = Url::from_str(url) else {
        return false;
    };
    match url.scheme() {
        "about" | "data" | "blob" => true,
        _ => match url.host_str() {
            Some(host) => host == F95_HOST || host.ends_with(".f95zone.to"),
            None => false,
        },
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum PageMessage {
    HostUrl { url: String },
    Notice { text: String },
}

fn on_page_message(id: u64, body: &str) {
    let message: PageMessage = match serde_json::from_str(body) {
        Ok(message) => message,
        Err(error) => {
            log::warn!("captcha window: unexpected page message {body:?}: {error}");
            return;
        }
    };
    let outcome = match message {
        PageMessage::HostUrl { url } => Url::from_str(&url)
            .map(ReplyValue::HostingLink)
            .map_err(CaptchaError::InvalidHostUrl),
        PageMessage::Notice { text } => Err(CaptchaError::PageNotice(text)),
    };
    finish_later(id, outcome);
}

/// Read the browser's cookies once a real F95 page is displayed.
///
/// `cf_clearance` is `HttpOnly`, so it cannot come from `document.cookie`;
/// the WebView's cookie store returns it. The User-Agent must be the one the
/// page was loaded with, because Cloudflare binds the clearance to it.
fn harvest_session(
    id: u64,
    logged_in: bool,
    user_agent: String,
) -> Result<BrowserSession, CaptchaError> {
    if !logged_in {
        return Err(CaptchaError::LoggedOut);
    }
    SESSIONS.with_borrow(|sessions| {
        let Some(webview) = sessions.get(&id).and_then(|session| session.webview.as_ref()) else {
            return Err(CaptchaError::Abandoned);
        };
        let cookies = webview
            .cookies_for_url(&format!("https://{F95_HOST}/"))
            .map_err(CaptchaError::WebView)?;
        let mut names: Vec<&cookie::Cookie<'_>> = cookies.iter().collect();
        names.sort_by_key(|cookie| cookie.name());
        let header = names
            .into_iter()
            .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
            .collect::<Vec<_>>()
            .join("; ");
        if header.is_empty() {
            return Err(CaptchaError::StillChallenged);
        }
        log::info!(
            "check window: captured browser session ({} cookies, user agent {})",
            header.split("; ").count(),
            user_agent
        );
        Ok(BrowserSession {
            cookies: header,
            user_agent,
        })
    })
}

fn resize(id: u64, width: f32, height: f32) {
    SESSIONS.with_borrow(|sessions| {
        let Some(session) = sessions.get(&id) else {
            return;
        };
        let Some(webview) = &session.webview else {
            return;
        };
        let bounds = browser_bounds(session.window.get_header_height(), width, height);
        if let Err(error) = webview.set_bounds(bounds) {
            log::warn!("captcha window: cannot resize the browser: {error}");
        }
    });
}

fn browser_bounds(header: f32, width: f32, height: f32) -> Rect {
    Rect {
        position: LogicalPosition::new(0.0, f64::from(header)).into(),
        size: LogicalSize::new(f64::from(width), f64::from((height - header).max(0.0))).into(),
    }
}

/// Close the session from a browser or window callback. Dropping the browser
/// inside its own callback is not allowed, so this waits for the next loop turn.
fn finish_later(id: u64, outcome: Result<ReplyValue, CaptchaError>) {
    slint::Timer::single_shot(Duration::ZERO, move || finish(id, outcome));
}

fn finish(id: u64, outcome: Result<ReplyValue, CaptchaError>) {
    let Some(session) = SESSIONS.with_borrow_mut(|sessions| sessions.remove(&id)) else {
        return;
    };
    if let Err(error) = session.window.hide() {
        log::warn!("captcha window: cannot hide the window: {error}");
    }
    drop(session.webview);
    let _ = session.reply.send(outcome);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_may_only_navigate_within_f95() {
        assert!(stays_on_f95("https://f95zone.to/dl/22078057/?u=1&k=2"));
        assert!(stays_on_f95("https://attachments.f95zone.to/2026/10/x.png"));
        assert!(stays_on_f95("about:blank"));
        assert!(!stays_on_f95("https://gofile.io/d/abc"));
        assert!(!stays_on_f95("https://evil.f95zone.to.example.com/"));
        assert!(!stays_on_f95("not a url"));
    }

    #[test]
    fn page_messages_are_parsed() {
        let host: PageMessage =
            serde_json::from_str(r#"{"kind":"host-url","url":"https://mega.nz/file/x#y"}"#)
                .expect("host message");
        assert!(matches!(host, PageMessage::HostUrl { url } if url == "https://mega.nz/file/x#y"));

        let notice: PageMessage =
            serde_json::from_str(r#"{"kind":"notice","text":"That link is no longer in the post."}"#)
                .expect("notice message");
        assert!(matches!(notice, PageMessage::Notice { .. }));
    }
}
