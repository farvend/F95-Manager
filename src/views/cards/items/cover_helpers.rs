// Helper functions extracted from cover_hover for clarity and reuse.

use crate::app::settings as app_settings;
use crate::parser::F95Thread;
use crate::tags::TAGS;
use crate::tags::{prefix_name_by_id, tag_name_by_id};

// Resolve engine name from prefixes (Engine group)
pub fn resolve_engine_name(thread: &F95Thread) -> Option<String> {
    for group in &TAGS.prefixes.games {
        if group.name.eq_ignore_ascii_case("Engine") {
            for pfx in &group.prefixes {
                if thread.prefixes.iter().any(|id| *id == pfx.id) {
                    return Some(pfx.name.replace("&#039;", "'"));
                }
            }
        }
    }
    None
}

// Collect warnings (tags + prefixes) based on user settings
pub fn collect_warnings(thread: &F95Thread) -> (Vec<String>, Vec<String>) {
    let (tag_names, pref_names) = app_settings::with_settings(|st| {
        // tags
        let mut tag_names: Vec<String> = Vec::new();
        for id in &thread.tags {
            if st.warn_tags.contains(id) {
                // Fast path: borrow from flattened lookup table.
                tag_names.push(tag_name_by_id(*id).replace("&#039;", "'"));
            }
        }

        // prefixes
        let mut pref_names: Vec<String> = Vec::new();
        for pid in &thread.prefixes {
            if st.warn_prefixes.contains(pid) {
                pref_names.push(prefix_name_by_id(*pid).replace("&#039;", "'"));
            }
        }

        (tag_names, pref_names)
    });
    (tag_names, pref_names)
}
