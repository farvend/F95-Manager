mod types;
pub use crate::tags::types::*;

use std::{
    borrow::Cow,
    collections::HashMap,
};

lazy_static::lazy_static! {
    pub static ref TAGS: Tags = serde_json::from_str(include_str!("tags.json")).unwrap_or_else(|e| {
        log::error!("Failed to parse embedded tags.json: {}. Falling back to empty tags.", e);
        Tags::default()
    });

    /// Fast tag ID -> name lookup without allocating `id.to_string()` on every call.
    ///
    /// We store `&'static str` references into `TAGS`, which is itself a `lazy_static` and
    /// therefore lives for the entire program lifetime.
    static ref TAG_NAME_BY_ID: HashMap<u32, &'static str> = {
        let mut map: HashMap<u32, &'static str> = HashMap::with_capacity(TAGS.tags.len());
        for (id, name) in TAGS.tags.iter() {
            map.insert(*id, name.as_str());
        }
        map
    };

    /// Fast prefix ID -> name lookup.
    /// Prefixes are nested in groups; flattening them once speeds up UI rendering.
    static ref PREFIX_NAME_BY_ID: HashMap<u32, &'static str> = {
        // Rough guess: total prefixes count is far smaller than tags.
        let mut map: HashMap<u32, &'static str> = HashMap::new();

        let mut push_group = |groups: &[PrefixesGroup]| {
            for g in groups {
                for p in &g.prefixes {
                    map.insert(p.id, p.name.as_str());
                }
            }
        };

        push_group(&TAGS.prefixes.games);
        push_group(&TAGS.prefixes.comics);
        push_group(&TAGS.prefixes.animations);
        push_group(&TAGS.prefixes.assets);
        map
    };
}

/// Helper function to get prefix name by ID.
/// DRY principle: Extracts duplicated prefix name lookup logic.
pub fn get_prefix_name_by_id(id: u32) -> String {
    prefix_name_by_id(id).into_owned()
}

/// Helper function to get tag name by ID.
pub fn get_tag_name_by_id(id: u32) -> String {
    tag_name_by_id(id).into_owned()
}

/// Borrowing variant: avoids allocations when ID exists.
/// Falls back to owned `id.to_string()` when unknown.
pub fn tag_name_by_id(id: u32) -> Cow<'static, str> {
    TAG_NAME_BY_ID
        .get(&id)
        .copied()
        .map(Cow::Borrowed)
        .unwrap_or_else(|| Cow::Owned(id.to_string()))
}

/// Borrowing variant: avoids allocations when ID exists.
/// Falls back to owned `id.to_string()` when unknown.
pub fn prefix_name_by_id(id: u32) -> Cow<'static, str> {
    PREFIX_NAME_BY_ID
        .get(&id)
        .copied()
        .map(Cow::Borrowed)
        .unwrap_or_else(|| Cow::Owned(id.to_string()))
}
