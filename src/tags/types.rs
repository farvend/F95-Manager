use std::collections::HashMap;

#[derive(serde::Deserialize, Debug)]
#[allow(dead_code)]
pub struct Tags {
    pub prefixes: AllPrefixes,
    pub tags: HashMap<String, String>,
    pub options: bool,
}

#[derive(serde::Deserialize, Debug)]
#[allow(dead_code)]
pub struct AllPrefixes {
    pub games: Vec<PrefixesGroup>,
    pub comics: Vec<PrefixesGroup>,
    pub animations: Vec<PrefixesGroup>,
    pub assets: Vec<PrefixesGroup>,
}

#[derive(serde::Deserialize, Debug)]
#[allow(dead_code)]
pub struct PrefixesGroup {
    pub id: usize,
    pub name: String,
    pub prefixes: Vec<Prefix>,
}

#[derive(serde::Deserialize, Debug)]
#[allow(dead_code)]
pub struct Prefix {
    pub id: usize,
    pub name: String,
    pub class: String,
}
