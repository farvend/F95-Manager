// Update checker: queries GitHub for the latest version and draws a bottom-right notice.
//
// - Background check runs once per app session.
// - If a newer version is available (based on Cargo package version), a "Update available!" notice
//   is drawn at the bottom-right corner.
// - This notice is clickable and opens the Releases page in the default browser.
// - The function returns the vertical pixel space it occupied so other bottom-right overlays
//   (like the Errors button) can stack above it.

use eframe::egui;
use lazy_static::lazy_static;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default, Clone)]
struct UpdateState {
    done: bool,
    available: bool,
    latest: Option<String>,
    url: Option<String>,
}

lazy_static! {
    static ref UPDATE_STATE: RwLock<UpdateState> = RwLock::new(UpdateState::default());
}

// Ensure the check runs only once
static CHECK_STARTED: AtomicBool = AtomicBool::new(false);

/// Draws the "Update available!" notice at bottom-right (if applicable) and returns the height
/// consumed (including a small margin) to allow stacking other overlays above it.
pub(super) fn draw_update_notice(ctx: &egui::Context) -> f32 {
    ensure_update_check(ctx);

    // Snapshot the state without holding the lock during painting
    let (available, url_opt) = {
        if let Ok(st) = UPDATE_STATE.read() {
            (st.available, st.url.clone())
        } else {
            (false, None)
        }
    };
    if !available {
        return 0.0;
    }

    let mut used_height: f32 = 0.0;
    egui::Area::new("update_available_floating".into())
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::Vec2::new(-12.0, -12.0))
        .interactable(true)
        .show(ctx, |ui| {
            let btn = egui::Button::new(
                egui::RichText::new("Update available!").color(egui::Color32::WHITE),
            )
            .fill(egui::Color32::from_rgb(60, 160, 60));
            let response = ui.add(btn);

            // Reserve a bit of spacing so the next overlay (e.g., Errors) can sit above
            used_height = response.rect.height() + 8.0;

            if response.clicked() {
                if let Some(u) = url_opt.as_ref() {
                    crate::app::settings::helpers::open_in_browser(u);
                } else {
                    crate::app::settings::helpers::open_in_browser(
                        "https://github.com/farvend/F95-Manager/releases",
                    );
                }
            }
        });

    used_height
}

fn ensure_update_check(ctx: &egui::Context) {
    if !CHECK_STARTED.swap(true, Ordering::SeqCst) {
        let ctx2 = ctx.clone();
        crate::app::rt().spawn(async move {
            if let Some((latest, url)) = check_latest_github().await {
                let current = env!("CARGO_PKG_VERSION");
                let available = is_version_newer(&latest, current);
                if let Ok(mut st) = UPDATE_STATE.write() {
                    st.done = true;
                    st.available = available;
                    st.latest = Some(latest);
                    st.url = Some(url);
                }
            } else {
                if let Ok(mut st) = UPDATE_STATE.write() {
                    st.done = true;
                    st.available = false;
                }
            }
            // Trigger repaint to reflect the new state
            ctx2.request_repaint();
        });
    }
}

async fn check_latest_github() -> Option<(String, String)> {
    let ua = format!("F95-Manager/{} (reqwest)", env!("CARGO_PKG_VERSION"));
    let client = reqwest::Client::builder().user_agent(ua).build().ok()?;

    // 1) Try releases/latest
    if let Ok(resp) = client
        .get("https://api.github.com/repos/farvend/F95-Manager/releases/latest")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(v) = resp.json::<serde_json::Value>().await {
                if let Some(tag) = v.get("tag_name").and_then(|t| t.as_str()) {
                    let url = v
                        .get("html_url")
                        .and_then(|t| t.as_str())
                        .unwrap_or("https://github.com/farvend/F95-Manager/releases")
                        .to_string();
                    let ver = normalize_version(tag);
                    if !ver.is_empty() {
                        return Some((ver, url));
                    }
                }
            }
        }
    }

    // 2) Fallback: tags endpoint
    if let Ok(resp) = client
        .get("https://api.github.com/repos/farvend/F95-Manager/tags?per_page=1")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(v) = resp.json::<serde_json::Value>().await {
                if let Some(first) = v.as_array().and_then(|a| a.first()) {
                    if let Some(name) = first.get("name").and_then(|n| n.as_str()) {
                        let ver = normalize_version(name);
                        if !ver.is_empty() {
                            return Some((
                                ver,
                                "https://github.com/farvend/F95-Manager/releases".to_string(),
                            ));
                        }
                    }
                }
            }
        }
    }

    None
}

fn normalize_version(s: &str) -> String {
    // Trim leading 'v'/'V' and then collect digits/dots until the first invalid char.
    let mut out = String::new();
    let mut started = false;
    for ch in s.chars() {
        if !started {
            if ch == 'v' || ch == 'V' {
                started = true;
                continue;
            }
            if ch.is_ascii_digit() {
                started = true;
                out.push(ch);
                continue;
            }
            // Skip any non-digit prefix
            continue;
        } else {
            if ch.is_ascii_digit() || ch == '.' {
                out.push(ch);
            } else {
                break;
            }
        }
    }
    out
}

fn parse_tuple(ver: &str) -> (u64, u64, u64) {
    let core = ver.split('-').next().unwrap_or(ver);
    let mut parts = core.split('.');
    let p1 = parts.next().and_then(|x| x.parse::<u64>().ok()).unwrap_or(0);
    let p2 = parts.next().and_then(|x| x.parse::<u64>().ok()).unwrap_or(0);
    let p3 = parts.next().and_then(|x| x.parse::<u64>().ok()).unwrap_or(0);
    (p1, p2, p3)
}

fn is_version_newer(latest: &str, current: &str) -> bool {
    let l = parse_tuple(latest);
    let c = parse_tuple(current);
    l > c
}
