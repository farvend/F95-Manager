use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use std::cell::RefCell;
use std::collections::HashMap;
use thiserror::Error;
use unic_langid::LanguageIdentifier;

type Bundle = FluentBundle<FluentResource>;

const SUPPORTED_LANGS: [&str; 2] = ["en", "ru"];
const FALLBACK_LANG: &str = "en";

fn load_ftl_source(lang: &str) -> &'static str {
    match lang {
        "en" => include_str!("resources/en.ftl"),
        "ru" => include_str!("resources/ru.ftl"),
        _ => include_str!("resources/en.ftl"),
    }
}

fn parse_lang(lang_code: &str) -> LanguageIdentifier {
    lang_code
        .parse::<LanguageIdentifier>()
        .unwrap_or_else(|_| FALLBACK_LANG.parse().unwrap())
}

fn normalize_lang(mut code: String) -> String {
    code.make_ascii_lowercase();
    let sep = code.find(['-', '_']).unwrap_or(code.len());
    let short = &code[..sep];
    if SUPPORTED_LANGS.contains(&short) {
        short.to_string()
    } else {
        FALLBACK_LANG.to_string()
    }
}

fn detect_system_lang() -> String {
    let sys = sys_locale::get_locale().unwrap_or_default();
    normalize_lang(sys)
}

struct LocalizationManager {
    current: String,
    fallback: String,
    bundles: HashMap<String, Bundle>,
}

impl LocalizationManager {
    fn new() -> Self {
        let mut bundles: HashMap<String, Bundle> = HashMap::new();
        for &code in SUPPORTED_LANGS.iter() {
            let langid = parse_lang(code);
            let mut bundle: Bundle = FluentBundle::new(vec![langid]);
            let res_str = load_ftl_source(code);
            let res = FluentResource::try_new(res_str.to_string())
                .expect("Failed to parse embedded FTL resource");
            bundle.add_resource(res).expect("Failed to add FTL to bundle");
            bundles.insert(code.to_string(), bundle);
        }
        Self {
            current: FALLBACK_LANG.to_string(),
            fallback: FALLBACK_LANG.to_string(),
            bundles,
        }
    }

    fn set_current(&mut self, code: &str) -> Result<(), LocalizationError> {
        let code = normalize_lang(code.to_string());
        if !self.bundles.contains_key(&code) {
            return Err(LocalizationError::UnsupportedLanguage(code));
        }
        self.current = code;
        Ok(())
    }

    fn set_auto(&mut self) -> Result<(), LocalizationError> {
        let detected = detect_system_lang();
        self.current = detected;
        Ok(())
    }

    fn get_bundle(&self, code: &str) -> Option<&Bundle> {
        self.bundles.get(code)
    }

    fn format_no_args(&self, id: &str) -> String {
        self.format_with_args(id, None)
    }

    fn format_with_args(&self, id: &str, args: Option<&FluentArgs>) -> String {
        // Try current
        if let Some(b) = self.get_bundle(&self.current) {
            if let Some(msg) = b.get_message(id) {
                if let Some(pat) = msg.value() {
                    let mut errors = vec![];
                    let s = b.format_pattern(pat, args, &mut errors).to_string();
                    return s;
                }
            }
        }
        // Fallback
        if let Some(b) = self.get_bundle(self.fallback.as_str()) {
            if let Some(msg) = b.get_message(id) {
                if let Some(pat) = msg.value() {
                    let mut errors = vec![];
                    let s = b.format_pattern(pat, args, &mut errors).to_string();
                    return s;
                }
            }
        }
        format!("[missing: {}]", id)
    }
}

thread_local! {
    static LOCALIZATION: RefCell<LocalizationManager> = RefCell::new(LocalizationManager::new());
}

#[derive(Debug, Error)]
pub enum LocalizationError {
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),
    #[error("Initialization error: {0}")]
    InitError(String),
}

/// Initialize localization system. If preferred_lang is None, system locale will be used.
/// If preferred/lang is unsupported, fallback to "en".
pub fn initialize_localization(preferred_lang: Option<&str>) -> Result<(), LocalizationError> {
    LOCALIZATION.with(|cell| {
        let mut mgr = cell.borrow_mut();
        match preferred_lang {
            Some(code) => mgr.set_current(code).or_else(|_| mgr.set_current(FALLBACK_LANG)),
            None => mgr.set_auto(),
        }
    })
}

/// Explicitly set current language to a supported code like "en" or "ru".
pub fn set_current_language(lang_code: &str) -> Result<(), LocalizationError> {
    LOCALIZATION.with(|cell| cell.borrow_mut().set_current(lang_code))
}

/// Set language from system locale (auto-detect).
pub fn set_language_auto() -> Result<(), LocalizationError> {
    LOCALIZATION.with(|cell| cell.borrow_mut().set_auto())
}

/// Return current language code ("en", "ru").
pub fn get_current_language() -> String {
    LOCALIZATION.with(|cell| cell.borrow().current.clone())
}

/// Return list of available languages.
pub fn available_languages() -> Vec<String> {
    SUPPORTED_LANGS.iter().map(|s| s.to_string()).collect()
}

/// Translate a message without arguments. Returns owned String.
pub fn translate(message_id: &str) -> String {
    LOCALIZATION.with(|cell| cell.borrow().format_no_args(message_id))
}

/// Translate a message with arguments given as (&str, String) pairs.
pub fn translate_with(message_id: &str, args: &[(&str, String)]) -> String {
    let mut fargs = FluentArgs::new();
    for (k, v) in args {
        fargs.set(*k, v.clone());
    }
    LOCALIZATION.with(|cell| cell.borrow().format_with_args(message_id, Some(&fargs)))
}
