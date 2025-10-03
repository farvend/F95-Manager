use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use thiserror::Error;
use unic_langid::{langid, LanguageIdentifier};

type Bundle = FluentBundle<FluentResource>;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum SupportedLang {
    #[serde(rename = "en")]
    English,
    #[serde(rename = "ru")]
    Russian,
}

// Map incoming strings to enum without allocating, ignoring case and suffixes like "-US"/"_RU".
impl From<&str> for SupportedLang {
    fn from(code: &str) -> Self {
        let mut string = code.to_string();
        if let Some(idx) = string.find(['-', '_']) {
            string = string[..idx].to_string();
        }
        string.make_ascii_lowercase();
        SupportedLang::English
    }
}

// Convert enum directly to LanguageIdentifier via macro, no string roundtrips.
impl From<SupportedLang> for LanguageIdentifier {
    fn from(lang: SupportedLang) -> Self {
        match lang {
            SupportedLang::English => langid!("en"),
            SupportedLang::Russian => langid!("ru"),
        }
    }
}

impl SupportedLang {
    fn ftl(self) -> &'static str {
        match self {
            SupportedLang::English => include_str!("resources/en.ftl"),
            SupportedLang::Russian => include_str!("resources/ru.ftl"),
        }
    }
}

fn detect_system_lang() -> SupportedLang {
    let sys = sys_locale::get_locale().unwrap_or_default();
    SupportedLang::from(sys.as_str())
}

// Global current language stored as the enum itself (no TLS, no integer mapping).
static CURRENT_LANG: OnceCell<RwLock<SupportedLang>> = OnceCell::new();

fn lang_lock() -> &'static RwLock<SupportedLang> {
    CURRENT_LANG.get_or_init(|| RwLock::new(SupportedLang::English))
}

#[derive(Debug, Error)]
pub enum LocalizationError {
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),
    #[error("Initialization error: {0}")]
    InitError(String),
}

fn make_bundle(lang: SupportedLang) -> Bundle {
    let mut bundle: Bundle = FluentBundle::new(vec![LanguageIdentifier::from(lang)]);
    let res_str = lang.ftl();
    let res = FluentResource::try_new(res_str.to_string()).expect("Failed to parse embedded FTL resource");
    bundle.add_resource(res).expect("Failed to add FTL to bundle");
    bundle
}

fn try_format(bundle: &Bundle, id: &str, args: Option<&FluentArgs>) -> Option<String> {
    let msg = bundle.get_message(id)?;
    let pat = msg.value()?;
    let mut errors = vec![];
    let s = bundle.format_pattern(pat, args, &mut errors).to_string();
    Some(s)
}

/// Initialize localization system. If preferred_lang is None, system locale will be used.
pub fn initialize_localization(preferred_lang: Option<SupportedLang>) -> Result<(), LocalizationError> {
    match preferred_lang {
        Some(lang) => set_current_language(lang)?,
        None => set_language_auto()?,
    }
    Ok(())
}

/// Explicitly set current language.
pub fn set_current_language(lang: SupportedLang) -> Result<(), LocalizationError> {
    let lock = lang_lock();
    *lock.write().expect("lang write lock") = lang;
    Ok(())
}

/// Set language from system locale (auto-detect).
pub fn set_language_auto() -> Result<(), LocalizationError> {
    let detected = detect_system_lang();
    let lock = lang_lock();
    *lock.write().expect("lang write lock") = detected;
    Ok(())
}

/// Return current language as enum.
pub fn get_current_language() -> SupportedLang {
    let lock = lang_lock();
    *lock.read().expect("lang read lock")
}

/// Translate a message without arguments. Returns owned String.
pub fn translate(message_id: &str) -> String {
    translate_with(message_id, &[])
}

/// Translate a message with arguments given as (&str, String) pairs.
pub fn translate_with(message_id: &str, args: &[(&str, String)]) -> String {
    let cur = get_current_language();

    let mut fargs = FluentArgs::new();
    for (k, v) in args {
        fargs.set(*k, v.clone());
    }
    let opt_args = if args.is_empty() { None } else { Some(&fargs) };

    // Try current language
    let cur_bundle = make_bundle(cur);
    if let Some(s) = try_format(&cur_bundle, message_id, opt_args) {
        return s;
    }

    // Fallback
    let fallback = SupportedLang::English;
    if cur != fallback {
        let fb_bundle = make_bundle(fallback);
        if let Some(s) = try_format(&fb_bundle, message_id, opt_args) {
            return s;
        }
    }

    format!("[missing: {}]", message_id)
}
