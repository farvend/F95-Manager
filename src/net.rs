use lazy_static::lazy_static;

pub const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:68.0) Gecko/20100101 Firefox/68.0";

lazy_static! {
    static ref CLIENT: reqwest::Client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .unwrap();
}

pub fn client() -> &'static reqwest::Client {
    &CLIENT
}
