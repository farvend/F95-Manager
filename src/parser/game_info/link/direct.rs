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
            }
            HostingSubset::Gofile => {
                let id = self.path.last()?;
                let (url, headers) = resolve_gofile_file(id).await?;
                let mut request = reqwest::Request::new(reqwest::Method::GET, url);
                *request.headers_mut() = headers;
                Some(DirectRequest::Http(request))
            }
            HostingSubset::Catbox => {
                let url = self.hosting.base().to_string() + &self.hosting.to_string() + "/" + &self.path[0];
                let mut request = reqwest::Request::new(reqwest::Method::GET, Url::from_str(&url).unwrap());
                let mut headers = HeaderMap::new();
                let value = HeaderValue::try_from("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:142.0) Gecko/20100101 Firefox/142.0").unwrap();
                headers.insert("User-Agent", value);
                *request.headers_mut() = headers;
                Some(DirectRequest::Http(request))
            }
            HostingSubset::Mega => {
                dbg!(&self.path);
                let mut path = self.path[1].clone();
                path = path[1..].replace('!', "#");
                let hosting = self.hosting.base().to_string() + &self.hosting.to_string();
                let url = hosting + "/file/" + &path;

                Some(DirectRequest::MegaPublicUrl(Url::from_str(&url).unwrap()))
            }
        }
    }

    // Visible to parent module (link) so it can construct DirectDownloadLink
    pub(super) fn new(value: Url) -> Option<DirectDownloadLink> {
        dbg!(&value);
        let mut hosting = value.domain()?
            .split('.')
            .rev()
            .nth(1)
            .unwrap()
            .to_string();
        hosting.get_mut(0..1).map(|e| e.make_ascii_uppercase());
        let hosting: HostingSubset = hosting.parse().ok()?;
        let mut path = value
            .path_segments()?
            .map(|e| e.to_owned())
            .collect::<Vec<String>>();
        value.fragment().inspect(|e| path.push(e.to_string()));

        Some(DirectDownloadLink { hosting, path })
    }
}
