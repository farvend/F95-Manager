use reqwest::{
    header::{HeaderMap, HeaderValue},
    Url,
};
use std::str::FromStr;

use crate::parser::game_info::hosting::HostingSubset;
use super::{gofile::resolve_gofile_file, info::DirectRequest};

#[derive(Debug, Clone)]
pub struct DirectDownloadLink {
    pub hosting: HostingSubset,
    pub path: Vec<String>,
    pub url: Url,
}

impl DirectDownloadLink {
    pub async fn get(self) -> Option<DirectRequest> {
        match self.hosting {
            HostingSubset::Pixeldrain => {
                let id = self.path.last()?;
                let path = format!("/api/file/{id}?download=");
                let url = self.hosting.base().to_owned() + &self.hosting.to_string() + &path;
                let url = Url::from_str(&url).ok()?;
                let headers: HeaderMap<HeaderValue> = HeaderMap::new();
                let mut request = reqwest::Request::new(reqwest::Method::GET, url);
                *request.headers_mut() = headers;
                Some(DirectRequest::Http(request))
                // Some(DownloadLinkInfo {
                //     url,
                //     method: reqwest::Method::GET,
                //     headers,
                // })
            }
            HostingSubset::Gofile => {
                let id = self.path.last()?;
                let (url, headers) = resolve_gofile_file(id).await?;
                let mut request = reqwest::Request::new(reqwest::Method::GET, url);
                *request.headers_mut() = headers;
                Some(DirectRequest::Http(request))
            }
            HostingSubset::Mega => {
                Some(DirectRequest::MegaPublicUrl(self.url.clone()))
            }
        }
    }

    // Visible to parent module (link) so it can construct DirectDownloadLink
    pub(super) fn new(value: Url) -> Option<DirectDownloadLink> {
        let mut hosting = value.domain()?.split('.').next()?.to_string();
        hosting.get_mut(0..1).map(|e| e.make_ascii_uppercase());
        let hosting: HostingSubset = hosting.parse().ok()?;
        let path = value
            .path_segments()?
            .map(|e| e.to_owned())
            .collect::<Vec<String>>();
        Some(DirectDownloadLink { hosting, path, url: value })
    }
}
