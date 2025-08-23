use reqwest::{
    header::{HeaderMap, HeaderValue},
    Url,
};

#[derive(Debug, Clone)]
pub struct DownloadLinkInfo {
    pub url: Url,
    pub method: reqwest::Method,
    pub headers: HeaderMap<HeaderValue>,
}

impl DownloadLinkInfo {
    pub fn build(self, client: reqwest::Client) -> reqwest::RequestBuilder {
        let url = self.url;
        let rb = match self.method {
            reqwest::Method::GET => client.get(url),
            reqwest::Method::POST => client.post(url),
            _ => client.get(url),
        };
        rb.headers(self.headers)
    }
}
