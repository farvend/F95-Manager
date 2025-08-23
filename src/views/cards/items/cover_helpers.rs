// Helper functions extracted from cover_hover for clarity and reuse.

use crate::app::settings as app_settings;
use crate::parser::F95Thread;
use crate::tags::TAGS;

// Resolve engine name from prefixes (Engine group)
pub fn resolve_engine_name(thread: &F95Thread) -> Option<String> {
    for group in &TAGS.prefixes.games {
        if group.name.eq_ignore_ascii_case("Engine") {
            for pfx in &group.prefixes {
                if thread.prefixes.iter().any(|id| *id == pfx.id as u32) {
                    return Some(pfx.name.replace("&#039;", "'"));
                }
            }
        }
    }
    None
}

// Collect warnings (tags + prefixes) based on user settings
pub fn collect_warnings(thread: &F95Thread) -> (Vec<String>, Vec<String>) {
    let st = app_settings::APP_SETTINGS.read().unwrap();

    // tags
    let mut tag_names: Vec<String> = Vec::new();
    for id in &thread.tags {
        if st.warn_tags.contains(id) {
            if let Some(name) = TAGS.tags.get(&id.to_string()) {
                tag_names.push(name.clone());
            }
        }
    }

    // prefixes
    let mut pref_names: Vec<String> = Vec::new();
    for pid in &thread.prefixes {
        if st.warn_prefixes.contains(pid) {
            // lookup prefix name in "games" groups
            let mut found: Option<String> = None;
            for g in &TAGS.prefixes.games {
                if let Some(p) = g.prefixes.iter().find(|p| p.id as u32 == *pid) {
                    found = Some(p.name.clone());
                    break;
                }
            }
            if let Some(n) = found {
                pref_names.push(n.replace("&#039;", "'"));
            }
        }
    }

    (tag_names, pref_names)
}
