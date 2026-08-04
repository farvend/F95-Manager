use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use thiserror::Error;
use unic_langid::{LanguageIdentifier, langid};

type Bundle = FluentBundle<FluentResource>;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
enum SupportedLang {
    English,
    Russian,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum LanguageChoice {
    #[default]
    #[serde(rename = "auto")]
    Automatic,
    #[serde(rename = "en")]
    English,
    #[serde(rename = "ru")]
    Russian,
}

// Map incoming strings to enum without allocating, ignoring case and suffixes like "-US"/"_RU".
impl From<&str> for SupportedLang {
    fn from(code: &str) -> Self {
        let language = code
            .split(['-', '_'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match language.as_str() {
            "ru" => SupportedLang::Russian,
            _ => SupportedLang::English,
        }
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
    let res = FluentResource::try_new(res_str.to_string())
        .expect("Failed to parse embedded FTL resource");
    bundle
        .add_resource(res)
        .expect("Failed to add FTL to bundle");
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
pub fn initialize_localization(choice: LanguageChoice) -> Result<(), LocalizationError> {
    set_language_choice(choice)
}

pub fn set_language_choice(choice: LanguageChoice) -> Result<(), LocalizationError> {
    match choice {
        LanguageChoice::Automatic => set_language_auto(),
        LanguageChoice::English => set_current_language(SupportedLang::English),
        LanguageChoice::Russian => set_current_language(SupportedLang::Russian),
    }
}

/// Explicitly set current language.
fn set_current_language(lang: SupportedLang) -> Result<(), LocalizationError> {
    let lock = lang_lock();
    *lock.write().expect("lang write lock") = lang;
    Ok(())
}

/// Set language from system locale (auto-detect).
fn set_language_auto() -> Result<(), LocalizationError> {
    let detected = detect_system_lang();
    let lock = lang_lock();
    *lock.write().expect("lang write lock") = detected;
    Ok(())
}

/// Return current language as enum.
fn get_current_language() -> SupportedLang {
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

#[cfg(test)]
mod tests {
    use super::{SupportedLang, make_bundle, try_format};

    #[test]
    fn locale_code_selects_supported_language() {
        assert_eq!(SupportedLang::from("ru-RU"), SupportedLang::Russian);
        assert_eq!(SupportedLang::from("ru_AM"), SupportedLang::Russian);
        assert_eq!(SupportedLang::from("en-US"), SupportedLang::English);
        assert_eq!(SupportedLang::from("de-DE"), SupportedLang::English);
    }

    #[test]
    fn both_embedded_translation_bundles_are_valid() {
        for language in [SupportedLang::English, SupportedLang::Russian] {
            let bundle = make_bundle(language);
            assert!(try_format(&bundle, "filters-title", None).is_some());
            assert!(try_format(&bundle, "settings-startup-filters", None).is_some());
        }
    }
}
