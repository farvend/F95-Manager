use std::collections::HashMap;

#[derive(serde::Deserialize, Debug)]
pub struct Tags {
    pub prefixes: AllPrefixes,
    /// Tag id -> tag name.
    ///
    /// Note: tags.json stores ids as JSON object keys (strings). Serde can decode those
    /// into integers, which lets us avoid `id.to_string()` allocations across the codebase.
    pub tags: HashMap<u32, String>,
    pub options: bool,
}

impl Default for Tags {
    fn default() -> Self {
        Self {
            prefixes: AllPrefixes::default(),
            tags: HashMap::new(),
            options: false,
        }
    }
}

#[derive(serde::Deserialize, Debug)]
pub struct AllPrefixes {
    pub games: Vec<PrefixesGroup>,
    pub comics: Vec<PrefixesGroup>,
    pub animations: Vec<PrefixesGroup>,
    pub assets: Vec<PrefixesGroup>,
}

impl Default for AllPrefixes {
    fn default() -> Self {
        Self {
            games: Vec::new(),
            comics: Vec::new(),
            animations: Vec::new(),
            assets: Vec::new(),
        }
    }
}

#[derive(serde::Deserialize, Debug)]
pub struct PrefixesGroup {
    pub id: u32,
    pub name: String,
    pub prefixes: Vec<Prefix>,
}

#[derive(serde::Deserialize, Debug)]
pub struct Prefix {
    pub id: u32,
    pub name: String,
    pub class: String,
}
