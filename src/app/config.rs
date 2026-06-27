use crate::app::persistable::Persistable;
use lazy_static::lazy_static;
use reqwest::cookie::{CookieStore, Jar};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use url::Url;

const BASE_URL: &str = "https://f95zone.to/";
const LOGIN_URL: &str = "https://f95zone.to/login/login";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub cookies: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
}

impl Persistable for AppConfig {}

lazy_static! {
    pub static ref APP_CONFIG: RwLock<AppConfig> = RwLock::new(AppConfig::default());
}

fn config_file_path() -> PathBuf {
    // Separate lightweight config file for authorization-related data
    // Allow override for tests via env var
    if let Ok(p) = std::env::var("F95_APP_CONFIG_PATH") {
        return PathBuf::from(p);
    }
    PathBuf::from("app_config.json")
}

impl AppConfig {}

fn extract_xf_token_from_html(html: &str) -> Option<String> {
    let document = scraper::Html::parse_document(html);
    let selector = scraper::Selector::parse(r#"input[name="_xfToken"]"#).ok()?;
    if let Some(token) = document
        .select(&selector)
        .find_map(|input| input.value().attr("value"))
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        return Some(token.to_string());
    }

    html.split("data-csrf=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
}

fn cookie_pairs_from_set_cookie_headers(
    headers: &reqwest::header::HeaderMap,
) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for val in headers.get_all(reqwest::header::SET_COOKIE).iter() {
        let Ok(s) = val.to_str() else { continue };
        if let Some(first) = s.split(';').next() {
            if let Some((name, value)) = first.split_once('=') {
                pairs.push((name.to_string(), value.to_string()));
            }
        }
    }
    pairs
}

fn cookie_header_from_pairs<I>(pairs: I) -> Option<String>
where
    I: IntoIterator<Item = (String, String)>,
{
    let mut parts: Vec<String> = pairs
        .into_iter()
        .filter_map(|(name, value)| {
            let name = name.trim();
            let value = value.trim();
            if name.is_empty() || value.is_empty() {
                None
            } else {
                Some(format!("{name}={value}"))
            }
        })
        .collect();

    if parts.is_empty() {
        None
    } else {
        parts.sort();
        Some(parts.join("; "))
    }
}

fn cookie_header_from_jar(jar: &Jar, url: &Url) -> Option<String> {
    jar.cookies(url)
        .and_then(|value| value.to_str().ok().map(str::to_string))
        .filter(|value| !value.trim().is_empty())
}

fn is_login_cookie_header(cookie_header: &str) -> bool {
    cookie_header
        .split(';')
        .map(str::trim)
        .any(|cookie| cookie.starts_with("xf_user="))
}

fn classify_login_failure(
    status: reqwest::StatusCode,
    final_url: &str,
    response_html: &str,
) -> String {
    let body = response_html.to_ascii_lowercase();
    let final_url = final_url.to_ascii_lowercase();

    if final_url.contains("/login/two-step") || body.contains("two-step verification") {
        return "login requires two-factor authentication; use cookie login fallback".to_string();
    }

    if body.contains("captcha") || body.contains("g-recaptcha") || body.contains("cf-chl") {
        return "login requires captcha or anti-bot verification; use cookie login fallback"
            .to_string();
    }

    if body.contains("incorrect password") || body.contains("requested user") {
        return "login failed: incorrect username or password".to_string();
    }

    if body.contains("security error occurred") {
        return "login failed: server rejected the security token; please try again".to_string();
    }

    format!(
        "login failed: authenticated user cookie was not received (status {})",
        status.as_u16()
    )
}

pub fn load_config_from_disk() {
    let path = config_file_path();
    match AppConfig::load_from_file(&path) {
        Ok(cfg) => {
            *APP_CONFIG.write().unwrap() = cfg;
            log::info!("Loaded app_config from {}", path.to_string_lossy());
        }
        Err(e) => {
            // Keep defaults if missing/unreadable
            log::info!(
                "Using default app_config; cannot load {}: {}",
                path.to_string_lossy(),
                e
            );
        }
    }
}

pub fn save_config_to_disk() {
    let path = config_file_path();
    let cfg = APP_CONFIG.read().unwrap().clone();
    if let Err(e) = cfg.save_to_file(&path) {
        log::error!(
            "Failed to save app_config to {}: {}",
            path.to_string_lossy(),
            e
        );
    } else {
        log::info!("Saved app_config to {}", path.to_string_lossy());
    }
}

/// Perform login against f95zone and persist cookies into app_config.json.
/// On success, APP_CONFIG.cookies will contain a ready-to-use "Cookie" header string.
pub async fn login_and_store(login: String, password: String) -> Result<(), String> {
    let base_url = Url::parse(BASE_URL).map_err(|e| format!("invalid base URL: {e}"))?;
    let jar = Arc::new(Jar::default());
    let client = reqwest::Client::builder()
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36"
        )
        .cookie_provider(jar.clone())
        .build()
        .map_err(|e| format!("failed to build login client: {e}"))?;

    // Fetch XenForo login token into a cookie-aware client session.
    let page_resp = client
        .get(LOGIN_URL)
        .send()
        .await
        .map_err(|e| format!("failed to fetch login page: {e}"))?;

    let html = page_resp
        .text()
        .await
        .map_err(|e| format!("failed to read login page: {e}"))?;

    // Extract XenForo request token
    let csrf_token = extract_xf_token_from_html(&html)
        .ok_or_else(|| "could not find XenForo token in login page".to_string())?;

    let mut form = std::collections::HashMap::<String, String>::new();
    form.insert("login".to_string(), login.clone());
    form.insert("url".to_string(), "".to_string());
    form.insert("password".to_string(), password);
    form.insert("password_confirm".to_string(), "".to_string());
    form.insert("additional_security".to_string(), "".to_string());
    form.insert("remember".to_string(), "1".to_string());
    form.insert("_xfRedirect".to_string(), "https://f95zone.to/".to_string());
    form.insert("website_code".to_string(), "".to_string());
    form.insert("_xfToken".to_string(), csrf_token.to_string());

    let resp = client
        .post(LOGIN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Referer", LOGIN_URL)
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("login request error: {e}"))?;

    let status = resp.status();
    let final_url = resp.url().to_string();

    let cookie_header = cookie_header_from_jar(&jar, &base_url)
        .or_else(|| cookie_header_from_pairs(cookie_pairs_from_set_cookie_headers(resp.headers())));

    let response_html = resp
        .text()
        .await
        .map_err(|e| format!("failed to read login response: {e}"))?;

    let Some(cookie_header) = cookie_header else {
        return Err(classify_login_failure(status, &final_url, &response_html));
    };

    if !is_login_cookie_header(&cookie_header) {
        return Err(classify_login_failure(status, &final_url, &response_html));
    }

    {
        let mut cfg = APP_CONFIG.write().unwrap();
        cfg.username = Some(login);
        cfg.cookies = Some(cookie_header);
    }
    save_config_to_disk();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // В тестах используем путь к временному файлу конфигурации, чтобы не перезаписать рабочий
    // app_config.json. Имя дополнительно содержит PID процесса для уникальности между запусками.
    fn temp_config_path(name: &str) -> String {
        let mut p = std::env::temp_dir();
        p.push(format!("{}_{}.json", name, std::process::id()));
        p.to_string_lossy().to_string()
    }

    // Интеграционный тест: использует реальные F95_LOGIN и F95_PASSWORD из .env или переменных окружения.
    // Если переменных нет — тест ПАДАЕТ с понятным сообщением (чтобы не "пропускался").
    #[tokio::test]
    #[ignore = "Requires sensetive data in env"]
    async fn login_from_env_integration() {
        // Перенаправляем путь конфигурации, чтобы не перетирать рабочий app_config.json
        let cfg_path = temp_config_path("app_config_test_env_ok");
        unsafe {
            std::env::set_var("F95_APP_CONFIG_PATH", &cfg_path);
        }

        // Пытаемся загрузить .env (не ошибка, если файла нет)
        let _ = dotenvy::dotenv();

        let login = std::env::var("F95_LOGIN")
            .expect("Отсутствует переменная окружения F95_LOGIN. Укажите её в .env или окружении.");
        let password = std::env::var("F95_PASSWORD").expect(
            "Отсутствует переменная окружения F95_PASSWORD. Укажите её в .env или окружении.",
        );

        let res = login_and_store(login, password).await;
        assert!(res.is_ok(), "Login failed: {res:?}");

        // Cleanup
        let _ = std::fs::remove_file(cfg_path);
    }

    #[test]
    fn extracts_xf_token_from_input_field() {
        let html = r#"
            <html>
                <body>
                    <form action="/login/login" method="post">
                        <input type="hidden" name="_xfToken" value="abc123,1700000000" />
                    </form>
                </body>
            </html>
        "#;

        assert_eq!(
            extract_xf_token_from_html(html),
            Some("abc123,1700000000".to_string())
        );
    }

    #[test]
    fn extracts_xf_token_from_data_csrf_fallback() {
        let html = r#"<html><body data-csrf="fallback-token"></body></html>"#;

        assert_eq!(
            extract_xf_token_from_html(html),
            Some("fallback-token".to_string())
        );
    }

    #[test]
    fn builds_stable_cookie_header_from_pairs() {
        let header = cookie_header_from_pairs(vec![
            ("xf_user".to_string(), "42%2Csecret".to_string()),
            ("xf_session".to_string(), "session".to_string()),
            ("".to_string(), "ignored".to_string()),
            ("empty".to_string(), "".to_string()),
        ]);

        assert_eq!(
            header,
            Some("xf_session=session; xf_user=42%2Csecret".to_string())
        );
    }

    #[test]
    fn detects_authenticated_user_cookie() {
        assert!(is_login_cookie_header(
            "xf_session=session; xf_user=42%2Csecret; xf_csrf=csrf"
        ));
        assert!(!is_login_cookie_header("xf_session=session; xf_csrf=csrf"));
    }

    #[test]
    fn classifies_captcha_response_separately() {
        let message = classify_login_failure(
            reqwest::StatusCode::OK,
            "https://f95zone.to/login/login",
            "You did not complete the CAPTCHA verification properly.",
        );

        assert!(message.contains("captcha"));
        assert!(message.contains("cookie login fallback"));
    }
}

/// Залогиниться, взяв логин/пароль из .env/переменных окружения (F95_LOGIN, F95_PASSWORD)
pub async fn login_from_env_and_store() -> Result<(), String> {
    // Загружаем .env, если есть
    let _ = dotenvy::dotenv();
    let login = match std::env::var("F95_LOGIN") {
        Ok(v) => v,
        Err(_) => {
            log::warn!(
                "Переменная окружения F95_LOGIN не задана. Укажите её в .env или окружении."
            );
            return Err("F95_LOGIN not set".to_string());
        }
    };
    let password = match std::env::var("F95_PASSWORD") {
        Ok(v) => v,
        Err(_) => {
            log::warn!(
                "Переменная окружения F95_PASSWORD не задана. Укажите её в .env или окружении."
            );
            return Err("F95_PASSWORD not set".to_string());
        }
    };
    login_and_store(login, password).await
}
