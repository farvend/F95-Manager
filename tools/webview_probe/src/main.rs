//! Standalone WebView2 probe: opens a URL in a bare wry webview (same setup
//! as the app's check window) and logs every navigation step, WebView2's
//! error status, the page title and the cookies, so network/challenge issues
//! can be diagnosed without the full app.
//!
//! Usage: webview_probe [URL] [--incognito] [--data-dir DIR]

use std::time::{Duration, Instant};

use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_WEB_ERROR_STATUS, ICoreWebView2NavigationCompletedEventArgs,
};
use webview2_com::NavigationCompletedEventHandler;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};
use wry::{WebContext, WebView, WebViewBuilder, WebViewExtWindows};

struct Probe {
    url: String,
    incognito: bool,
    app_mode: bool,
    child: bool,
    late_account: bool,
    script: Option<String>,
    account_set: bool,
    context: Option<WebContext>,
    window: Option<Window>,
    webview: Option<WebView>,
    started: Instant,
    last_report: Instant,
}

fn log(started: Instant, message: impl AsRef<str>) {
    println!("[{:>6.1}s] {}", started.elapsed().as_secs_f32(), message.as_ref());
}

impl ApplicationHandler for Probe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = event_loop
            .create_window(Window::default_attributes().with_title("webview probe"))
            .expect("window");
        let started = self.started;
        let mut builder = match self.context.as_mut() {
            Some(context) => WebViewBuilder::new_with_web_context(context),
            None => WebViewBuilder::new(),
        };
        builder = builder
            .with_incognito(self.incognito)
            .with_navigation_handler(move |url| {
                log(started, format!("navigation starting: {url}"));
                true
            })
            .with_on_page_load_handler(move |event, url| {
                let event = match event {
                    wry::PageLoadEvent::Started => "started",
                    wry::PageLoadEvent::Finished => "finished",
                };
                log(started, format!("page load {event}: {url}"));
            })
            ;
        // `--app-mode`: same steps as the app's check window: no initial URL,
        // drop stale cf_clearance, set the account cookie, then load_url.
        if let Some(script) = &self.script {
            builder = builder.with_initialization_script_for_main_only(script, true);
        }
        if !self.app_mode {
            builder = builder.with_url(&self.url);
        }
        let webview = if self.child {
            builder
                .with_bounds(wry::Rect {
                    position: wry::dpi::LogicalPosition::new(0, 60).into(),
                    size: wry::dpi::LogicalSize::new(800, 540).into(),
                })
                .build_as_child(&window)
                .expect("webview")
        } else {
            builder.build(&window).expect("webview")
        };

        // wry hides WebView2's error status; read it from the raw event.
        let raw = webview.webview();
        let mut token = Default::default();
        unsafe {
            raw.add_NavigationCompleted(
                &NavigationCompletedEventHandler::create(Box::new(
                    move |_, args: Option<ICoreWebView2NavigationCompletedEventArgs>| {
                        if let Some(args) = args {
                            let mut ok = Default::default();
                            let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                            args.IsSuccess(&mut ok)?;
                            args.WebErrorStatus(&mut status)?;
                            let mut http = 0i32;
                            if let Ok(args2) = windows::core::Interface::cast::<
                                webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2NavigationCompletedEventArgs2,
                            >(&args)
                            {
                                let _ = args2.HttpStatusCode(&mut http);
                            }
                            log(
                                started,
                                format!(
                                    "navigation completed: success={} web_error_status={} http_status={http}",
                                    ok.as_bool(),
                                    status.0
                                ),
                            );
                        }
                        Ok(())
                    },
                )),
                &mut token,
            )
            .expect("NavigationCompleted");
        }
        if self.app_mode {
            for cookie in webview.cookies_for_url("https://f95zone.to/").unwrap_or_default() {
                if cookie.name() == "cf_clearance" {
                    log(started, format!("deleting stale cf_clearance: {:?}", webview.delete_cookie(&cookie)));
                }
            }
            if let (false, Ok(account)) = (self.late_account, std::env::var("PROBE_XF_USER")) {
                let cookie = wry::cookie::Cookie::build(("xf_user", account))
                    .domain("f95zone.to")
                    .path("/")
                    .secure(true)
                    .http_only(true)
                    .build();
                log(started, format!("set xf_user: {:?}", webview.set_cookie(&cookie)));
                let names: Vec<String> = webview.cookies_for_url("https://f95zone.to/").unwrap_or_default().iter().map(|c| format!("{}({}) domain={:?} expires={:?}", c.name(), c.value().len(), c.domain(), c.expires())).collect();
                log(started, format!("right after set: {names:?}"));
            }
            if let Ok(clearance) = std::env::var("PROBE_CF") {
                let cookie = wry::cookie::Cookie::build(("cf_clearance", clearance))
                    .domain(".f95zone.to").path("/").secure(true).http_only(true).build();
                log(started, format!("set cf_clearance: {:?}", webview.set_cookie(&cookie)));
            }
            log(started, format!("load_url: {:?}", webview.load_url(&self.url)));
        }
        log(
            started,
            format!(
                "loading {} (incognito={}, app_mode={}, child={})",
                self.url, self.incognito, self.app_mode, self.child
            ),
        );
        self.webview = Some(webview);
        self.window = Some(window);
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_secs(1)));
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.last_report.elapsed() >= Duration::from_millis(500)
            && let Some(webview) = &self.webview
        {
            self.last_report = Instant::now();
            let started = self.started;
            let _ = webview.evaluate_script_with_callback(
                "JSON.stringify([location.href, document.title, document.readyState, navigator.userAgent, document.documentElement.getAttribute('data-logged-in')])",
                move |value| {
                    log(started, format!("state: {value}"));
                    let _ = std::fs::write(std::env::temp_dir().join("probe_state.json"), value);
                },
            );
            match webview.cookies_for_url("https://f95zone.to/") {
                Ok(cookies) => {
                    let names: Vec<String> = cookies
                        .iter()
                        .map(|c| format!("{}({})", c.name(), c.value().len()))
                        .collect();
                    log(started, format!("cookies: {names:?}"));
                    if self.late_account && !self.account_set && cookies.iter().any(|c| c.name() == "cf_clearance") {
                        self.account_set = true;
                        let account = std::env::var("PROBE_XF_USER").unwrap_or_default();
                        log(started, format!("account len {}", account.len()));
                        let decoded = account.replace("%2C", ",");
                        let variants: [(&str, String, bool); 4] = [
                            ("v_pct", "4062081%2Cabc".to_string(), false),
                            ("v_comma", "4062081,abc".to_string(), false),
                            ("v_long_plain", "a".repeat(51), false),
                            ("v_decoded", decoded.clone(), false),
                        ];
                        for (name, value, _) in variants {
                            let cookie = wry::cookie::Cookie::build((name, value))
                                .domain("f95zone.to").path("/").build();
                            log(started, format!("set {name}: {:?}", webview.set_cookie(&cookie)));
                        }
                        let expiring = wry::cookie::Cookie::build(("v_pct_expires", account.clone()))
                            .domain("f95zone.to").path("/").secure(true).http_only(true)
                            .max_age(wry::cookie::time::Duration::days(365)).build();
                        log(started, format!("set v_pct_expires: {:?}", webview.set_cookie(&expiring)));
                        let short = wry::cookie::Cookie::build(("d_short", "hello")).domain("f95zone.to").path("/").build();
                        log(started, format!("set d_short: {:?}", webview.set_cookie(&short)));
                        let names: Vec<String> = webview.cookies().unwrap_or_default().iter().filter(|c| c.domain().unwrap_or("").contains("f95")).map(|c| format!("{}@{:?}", c.name(), c.domain())).collect();
                        log(started, format!("all f95 cookies now: {names:?}"));
                    }
                    let header = cookies
                        .iter()
                        .map(|c| format!("{}={}", c.name(), c.value()))
                        .collect::<Vec<_>>()
                        .join("; ");
                    let _ = std::fs::write(std::env::temp_dir().join("probe_cookies.txt"), header);
                }
                Err(error) => log(started, format!("cookies error: {error}")),
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_secs(1)));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(webview) = &self.webview {
                    let _ = webview.set_bounds(wry::Rect {
                        position: wry::dpi::PhysicalPosition::new(0, 0).into(),
                        size: size.into(),
                    });
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut url = "https://f95zone.to/".to_string();
    let mut incognito = false;
    let mut data_dir = None;
    let mut app_mode = false;
    let mut child = false;
    let mut late_account = false;
    let mut script = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--incognito" => incognito = true,
            "--app-mode" => app_mode = true,
            "--child" => child = true,
            "--late-account" => late_account = true,
            "--script" => script = args.next().map(|f| std::fs::read_to_string(f).expect("script")),
            "--data-dir" => data_dir = args.next(),
            other => url = other.to_string(),
        }
    }
    let now = Instant::now();
    let mut probe = Probe {
        url,
        incognito,
        app_mode,
        child,
        late_account,
        script,
        account_set: false,
        context: data_dir.map(|dir| WebContext::new(Some(dir.into()))),
        window: None,
        webview: None,
        started: now,
        last_report: now,
    };
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.run_app(&mut probe).expect("run");
}
