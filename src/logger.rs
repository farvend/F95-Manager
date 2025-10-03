// Simple in-app GUI logger that mirrors log records to stderr and
// stores a bounded buffer for display inside the egui UI, with level info.
// Now also writes only warn+ lines to log.txt and installs a panic/error handler.

use lazy_static::lazy_static;
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};
use std::backtrace::Backtrace;

#[derive(Clone)]
pub struct LogEntry {
    pub level: Level,
    pub target: String,
    pub msg: String,
}

const MAX_LOG_LINES: usize = 5000;

lazy_static! {
    static ref LOGS: Mutex<VecDeque<LogEntry>> = Mutex::new(VecDeque::new());
}
lazy_static! {
    static ref MIRROR_STDERR: bool = {
        let v = std::env::var("GUI_LOG_STDERR").unwrap_or_else(|_| "0".to_string());
        matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
    };
}
lazy_static! {
    // File for persistent logging
    static ref LOG_FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);
}

static NEW_LOGS: AtomicBool = AtomicBool::new(false);

struct GuiLogger;

impl Log for GuiLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        if let Some(max) = log::max_level().to_level() {
            metadata.level() <= max
        } else {
            false
        }
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        // Compose one-line formatted record with timestamp.
        let ts = timestamp_millis();
        let line = format!(
            "[{}] [{:>5}] {}: {}",
            ts,
            record.level(),
            record.target(),
            record.args()
        );

        // Mirror to stderr for normal logging behavior (optional).
        if *MIRROR_STDERR {
            eprintln!("{}", line);
        }

        // Append to log.txt only for warn and above (Warn, Error)
        if matches!(record.level(), Level::Warn | Level::Error) {
            write_file_line(&line);
        }

        // Store to in-memory buffer for GUI.
        push_entry(LogEntry {
            level: record.level(),
            target: record.target().to_string(),
            msg: format!("{}", record.args()),
        });
    }

    fn flush(&self) {
        flush_file();
    }
}

fn push_entry(entry: LogEntry) {
    if let Ok(mut buf) = LOGS.lock() {
        buf.push_back(entry);
        if buf.len() > MAX_LOG_LINES {
            buf.pop_front();
        }
    }
    NEW_LOGS.store(true, Ordering::Relaxed);
}

fn level_from_env() -> Option<LevelFilter> {
    let Ok(val) = std::env::var("RUST_LOG") else {
        return None;
    };
    let v = val.to_lowercase();
    if v.contains("trace") {
        Some(LevelFilter::Trace)
    } else if v.contains("debug") {
        Some(LevelFilter::Debug)
    } else if v.contains("info") {
        Some(LevelFilter::Info)
    } else if v.contains("warn") {
        Some(LevelFilter::Warn)
    } else if v.contains("error") {
        Some(LevelFilter::Error)
    } else if v.contains("off") {
        Some(LevelFilter::Off)
    } else {
        None
    }
}

// Initialize logger, open log.txt, and install panic hook.
pub fn init() {
    // Install logger
    let _ = log::set_boxed_logger(Box::new(GuiLogger));

    // Log everything by default to ensure full log capture.
    // Can be overridden by RUST_LOG environment variable if set.
    let level = level_from_env().unwrap_or(LevelFilter::Trace);
    log::set_max_level(level);

    // Open (or create) log file for appending
    {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open("log.txt")
            .ok();
        if let Ok(mut lf) = LOG_FILE.lock() {
            *lf = file;
        }
    }

    // Install a panic hook to log unexpected panics to log.txt (and in-memory buffer).
    install_panic_hook();

    log::info!(
        "GUI logger initialized at level {} (persisting to log.txt)",
        display_level(level)
    );
}

fn display_level(level: LevelFilter) -> &'static str {
    match level {
        LevelFilter::Off => "off",
        LevelFilter::Error => "error",
        LevelFilter::Warn => "warn",
        LevelFilter::Info => "info",
        LevelFilter::Debug => "debug",
        LevelFilter::Trace => "trace",
    }
}

pub fn for_each_range<F: FnMut(&LogEntry)>(start: usize, end: usize, mut f: F) {
    if let Ok(buf) = LOGS.lock() {
        let len = buf.len();
        let s = start.min(len);
        let e = end.min(len);
        for idx in s..e {
            if let Some(entry) = buf.get(idx) {
                f(entry);
            }
        }
    }
}

// Back-compat helper: returns preformatted strings (not used by new UI, but kept for convenience)
pub fn get_all() -> Vec<String> {
    if let Ok(buf) = LOGS.lock() {
        buf.iter().map(format_line).collect()
    } else {
        vec![]
    }
}

fn format_line(e: &LogEntry) -> String {
    format!("[{:>5}] {}: {}", e.level, e.target, e.msg)
}

pub fn len() -> usize {
    if let Ok(buf) = LOGS.lock() {
        buf.len()
    } else {
        0
    }
}

pub fn clear() {
    if let Ok(mut buf) = LOGS.lock() {
        buf.clear();
    }
    NEW_LOGS.store(true, Ordering::Relaxed);
}

/// Returns true if new logs arrived since the last call.
pub fn take_new_flag() -> bool {
    NEW_LOGS.swap(false, Ordering::Relaxed)
}

// --- helpers: persistent log file + panic hook ---

fn timestamp_millis() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = now.as_secs();
    let ms = now.subsec_millis();
    format!("{secs}.{ms:03}")
}

fn write_file_line(line: &str) {
    if let Ok(mut lf) = LOG_FILE.lock() {
        if let Some(f) = lf.as_mut() {
            let _ = writeln!(f, "{}", line);
            let _ = f.flush();
        }
    }
}

fn flush_file() {
    if let Ok(mut lf) = LOG_FILE.lock() {
        if let Some(f) = lf.as_mut() {
            let _ = f.flush();
        }
    }
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let msg = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            *s
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.as_str()
        } else {
            "Box<Any>"
        };

        let loc = if let Some(l) = panic_info.location() {
            format!("{}:{}:{}", l.file(), l.line(), l.column())
        } else {
            "unknown".to_string()
        };

        let bt = Backtrace::force_capture();
        let header = format!(
            "[{}] [ERROR] panic at {loc}: {msg}",
            timestamp_millis()
        );

        write_file_line(&header);
        // Write backtrace lines
        for line in format!("{bt:?}").lines() {
            write_file_line(line);
        }

        // Also send through the normal logger pipeline (if enabled)
        log::error!("panic at {loc}: {msg}\n{bt:?}");
    }));
}
