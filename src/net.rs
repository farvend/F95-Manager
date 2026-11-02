use core::fmt;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use lazy_static::lazy_static;
use reqwest::cookie::CookieStore;
use reqwest::header::HeaderValue;
use reqwest::Url;

/// Host the account cookies belong to. Cookies are never attached to requests
/// to other hosts (gofile, MEGA, catbox), so pasting a Cookie header does not
/// leak credentials to download mirrors.
const F95_HOST: &str = "f95zone.to";

/// Failure to prepare the shared HTTP client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetError {
    /// `app_config.json` has no `user_agent` value.
    MissingUserAgent,
    /// reqwest refused to build the client (details are logged).
    ClientBuild,
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetError::MissingUserAgent => write!(
                f,
                "app_config.json: user_agent is not set; it must match the User-Agent of the browser whose cf_clearance cookie is stored in cookies"
            ),
            NetError::ClientBuild => write!(f, "failed to build HTTP client"),
        }
    }
}

impl std::error::Error for NetError {}

/// Session cookies F95zone sets during a request/response cycle.
///
/// The `/dl/` download pages issue `xf_session`/`xf_csrf` on the first GET and
/// require exactly those values on the following POST, otherwise the POST is
/// rejected with "Cookies are required to use this site". Keeping them in a
/// jar mirrors what a browser does; a manually pinned `Cookie:` header cannot
/// express this.
#[derive(Debug, Default)]
pub struct F95Cookies {
    session: RwLock<HashMap<String, String>>,
}

impl F95Cookies {
    /// Account cookies from the config, re-read for every request so a new
    /// login or a freshly pasted cookie string works without a restart.
    fn account_header() -> Option<String> {
        crate::app::config::configured_cookies()
    }

    fn session_header(&self) -> Option<String> {
        let session = self.session.read().expect("session cookie lock poisoned");
        if session.is_empty() {
            return None;
        }
        let mut pairs: Vec<(&str, &str)> = session
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        pairs.sort_unstable();
        Some(
            pairs
                .into_iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }
}

impl CookieStore for F95Cookies {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, _url: &Url) {
        for header in cookie_headers {
            let Ok(text) = header.to_str() else {
                continue;
            };
            let Some((pair, _attributes)) = text.split_once(';') else {
                continue;
            };
            let Some((name, value)) = pair.split_once('=') else {
                continue;
            };
            let name = name.trim();
            let value = value.trim();
            if name.is_empty() {
                continue;
            }
            let mut session = self.session.write().expect("session cookie lock poisoned");
            if value.is_empty() {
                session.remove(name);
            } else {
                session.insert(name.to_string(), value.to_string());
            }
        }
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        if url.host_str() != Some(F95_HOST) {
            return None;
        }
        match (Self::account_header(), self.session_header()) {
            (Some(account), Some(session)) => {
                HeaderValue::from_str(&format!("{account}; {session}")).ok()
            }
            (Some(account), None) => HeaderValue::from_str(&account).ok(),
            (None, Some(session)) => HeaderValue::from_str(&session).ok(),
            (None, None) => None,
        }
    }
}

lazy_static! {
    static ref COOKIES: Arc<F95Cookies> = Arc::new(F95Cookies::default());
    /// Cached per User-Agent: after the embedded browser passes Cloudflare its
    /// own UA replaces the stored one, and the old client would send a UA the
    /// fresh `cf_clearance` was not issued for.
    static ref CLIENT: RwLock<Option<(String, reqwest::Client)>> = RwLock::new(None);
}

/// User-Agent for f95zone and hosting APIs.
///
/// There is deliberately no default: Cloudflare binds `cf_clearance` to the
/// exact User-Agent string, so a missing value in the config is an error, not
/// something to guess.
pub fn user_agent() -> Result<String, NetError> {
    match crate::app::config::configured_user_agent() {
        Some(user_agent) => Ok(user_agent),
        None => Err(NetError::MissingUserAgent),
    }
}

fn build_client(user_agent: &str) -> Result<reqwest::Client, NetError> {
    reqwest::Client::builder()
        .user_agent(user_agent)
        .cookie_provider(Arc::clone(&COOKIES))
        .build()
        .map_err(|error| {
            log::error!("HTTP client build failed: {error}");
            NetError::ClientBuild
        })
}

/// Shared HTTP client for the currently configured User-Agent.
pub fn client() -> Result<reqwest::Client, NetError> {
    let user_agent = user_agent()?;
    if let Some(client) = CLIENT
        .read()
        .expect("client lock poisoned")
        .as_ref()
        .filter(|(ua, _)| *ua == user_agent)
        .map(|(_, client)| client.clone())
    {
        return Ok(client);
    }
    let client = build_client(&user_agent)?;
    *CLIENT.write().expect("client lock poisoned") = Some((user_agent, client.clone()));
    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn cookies_are_not_sent_to_download_mirrors() {
        let store = F95Cookies::default();
        let gofile = Url::from_str("https://api.gofile.io/contents").expect("gofile url");

        assert!(store.cookies(&gofile).is_none());
    }

    #[test]
    fn session_cookie_is_stored_and_sent_back() {
        let store = F95Cookies::default();
        let url = Url::from_str("https://f95zone.to/dl/21783444/").expect("f95 url");
        let header =
            HeaderValue::from_str("xf_session=session123; path=/; secure; HttpOnly").expect("set-cookie");

        store.set_cookies(&mut std::iter::once(&header), &url);

        let sent = store.cookies(&url).expect("cookie header");
        let text = sent.to_str().expect("cookie header text");
        assert!(text.contains("xf_session=session123"));
    }

    #[test]
    fn empty_set_cookie_removes_session_value() {
        let store = F95Cookies::default();
        let url = Url::from_str("https://f95zone.to/").expect("f95 url");
        let set = HeaderValue::from_str("xf_session=session123").expect("set-cookie");
        store.set_cookies(&mut std::iter::once(&set), &url);
        let clear = HeaderValue::from_str("xf_session=").expect("delete-cookie");
        store.set_cookies(&mut std::iter::once(&clear), &url);

        assert!(store.session_header().is_none());
    }
}
