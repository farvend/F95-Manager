use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub cookies: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
}

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

impl AppConfig {
    pub fn load_from_file(path: &std::path::Path) -> std::io::Result<Self> {
        let data = std::fs::read_to_string(path)?;
        let s: AppConfig = serde_json::from_str(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(s)
    }

    pub fn save_to_file(&self, path: &std::path::Path) -> std::io::Result<()> {
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, data)
    }
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
    // Do not follow redirects to ensure we capture Set-Cookie from the login response itself.
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36")
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("client build error: {e}"))?;

    // Fetch CSRF token
    let page_resp = client
        .get("https://f95zone.to/")
        .send()
        .await
        .map_err(|e| format!("failed to fetch login page: {e}"))?;

    let html = page_resp
        .text()
        .await
        .map_err(|e| format!("failed to read login page: {e}"))?;

    // Extract data-csrf token
    let csrf_token = html
        .split("data-csrf=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .ok_or_else(|| "could not find data-csrf token in login page".to_string())?;

    dbg!(csrf_token);

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
        .post("https://f95zone.to/login/login")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Referer", "https://f95zone.to/")
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("login request error: {e}"))?;

    let status = resp.status();

    // Collect cookie pairs from Set-Cookie headers
    let headers = resp.headers();
    let mut cookie_map = std::collections::HashMap::<String, String>::new();
    for val in headers.get_all(reqwest::header::SET_COOKIE).iter() {
        let Ok(s) = val.to_str() else { continue };
        // Take first part "name=value" before attributes
        if let Some(first) = s.split(';').next() {
            if let Some((name, value)) = first.split_once('=') {
                let name = name.trim();
                let value = value.trim();
                if !name.is_empty() && !value.is_empty() {
                    cookie_map.insert(name.to_string(), value.to_string());
                }
            }
        }
    }

    if !cookie_map.keys().any(|e| e == "xf_session") {
        return Err(
            "login failed. Server didn't send xf_session. You probably entered wrong credentials"
                .to_string(),
        );
    }

    if cookie_map.is_empty() {
        return Err(format!(
            "login failed: server returned no Set-Cookie headers (status {})",
            status.as_u16()
        ));
    }

    // Compose Cookie header
    let mut parts: Vec<String> = cookie_map
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    parts.sort(); // stable order
    let cookie_header = parts.join("; ");

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
