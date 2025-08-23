use crate::parser::F95Thread;

/// Select preferred image URL for a thread:
/// - Prefer the cover if present
/// - Otherwise fallback to the first screenshot (if any)
pub fn get_cover_or_first_screen_url(t: &F95Thread) -> Option<String> {
    if !t.cover.is_empty() {
        Some(t.cover.clone())
    } else {
        t.screens.first().cloned()
    }
}
