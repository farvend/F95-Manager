use reqwest::{
    Url,
    header::{HeaderMap, HeaderValue},
};
use std::str::FromStr;

use super::{
    CatboxLinkError, DirectLinkError, MegaLinkError, PixeldrainLinkError,
    gofile::resolve_gofile_file, info::DirectRequest,
};
use crate::parser::game_info::hosting::HostingSubset;

#[derive(Debug, Clone)]
pub struct DirectDownloadLink {
    pub hosting: HostingSubset,
    pub path: Vec<String>,
}

impl DirectDownloadLink {
    pub async fn get(self) -> Result<DirectRequest, DirectLinkError> {
        match self.hosting {
            HostingSubset::Pixeldrain => {
                let id = self.path.last().filter(|id| !id.is_empty()).ok_or(
                    DirectLinkError::Pixeldrain(PixeldrainLinkError::MissingFileId),
                )?;
                let path = format!("/api/file/{id}?download=");
                let url = self.hosting.base().to_owned() + &self.hosting.to_string() + &path;
                let url = Url::from_str(&url).map_err(|error| {
                    DirectLinkError::Pixeldrain(PixeldrainLinkError::InvalidUrl(error))
                })?;
                let headers: HeaderMap<HeaderValue> = HeaderMap::new();
                let mut request = reqwest::Request::new(reqwest::Method::GET, url);
                *request.headers_mut() = headers;
                Ok(DirectRequest::Http(request))
            }
            HostingSubset::Gofile => {
                let id =
                    self.path
                        .last()
                        .filter(|id| !id.is_empty())
                        .ok_or(DirectLinkError::Gofile(
                            super::gofile::GofileLinkError::MissingFolderId,
                        ))?;
                let (url, headers) = resolve_gofile_file(id)
                    .await
                    .map_err(DirectLinkError::Gofile)?;
                let mut request = reqwest::Request::new(reqwest::Method::GET, url);
                *request.headers_mut() = headers;
                Ok(DirectRequest::Http(request))
            }
            HostingSubset::Catbox => {
                let path = self
                    .path
                    .first()
                    .filter(|path| !path.is_empty())
                    .ok_or(DirectLinkError::Catbox(CatboxLinkError::MissingFilePath))?;
                let url = self.hosting.base().to_string() + &self.hosting.to_string() + "/" + path;
                let url = Url::from_str(&url)
                    .map_err(|error| DirectLinkError::Catbox(CatboxLinkError::InvalidUrl(error)))?;
                let mut request = reqwest::Request::new(reqwest::Method::GET, url);
                let mut headers = HeaderMap::new();
                let value = HeaderValue::try_from("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:142.0) Gecko/20100101 Firefox/142.0").unwrap();
                headers.insert("User-Agent", value);
                *request.headers_mut() = headers;
                Ok(DirectRequest::Http(request))
            }
            HostingSubset::Mega => {
                // MEGA URL formats:
                // OLD: mega.nz/#!{id}!{key} -> path = ["", "!{id}!{key}"]
                // NEW: mega.nz/file/{id}#{key} -> path = ["file", "{id}", "{key}"]
                let hosting = self.hosting.base().to_string() + &self.hosting.to_string();

                let url = if self.path.first().map(|s| s.as_str()) == Some("file")
                    || self.path.first().map(|s| s.as_str()) == Some("folder")
                {
                    let file_type = self
                        .path
                        .first()
                        .ok_or(DirectLinkError::Mega(MegaLinkError::MissingNodeId))?;
                    let node_id = self
                        .path
                        .get(1)
                        .ok_or(DirectLinkError::Mega(MegaLinkError::MissingNodeId))?;
                    let node_key = self
                        .path
                        .get(2)
                        .ok_or(DirectLinkError::Mega(MegaLinkError::MissingNodeKey))?;
                    format!("{hosting}/{file_type}/{node_id}#{node_key}")
                } else {
                    let fragment = self
                        .path
                        .get(1)
                        .and_then(|value| value.strip_prefix('!'))
                        .filter(|value| value.contains('!'))
                        .ok_or(DirectLinkError::Mega(
                            MegaLinkError::MalformedLegacyFragment,
                        ))?;
                    let url_path = fragment.replace('!', "#");
                    format!("{hosting}/file/{url_path}")
                };

                let url = Url::from_str(&url)
                    .map_err(|error| DirectLinkError::Mega(MegaLinkError::InvalidUrl(error)))?;
                Ok(DirectRequest::MegaPublicUrl(url))
            }
        }
    }

    // Visible to parent module (link) so it can construct DirectDownloadLink
    pub(super) fn new(value: Url) -> Option<DirectDownloadLink> {
        let hosting: HostingSubset = value.clone().try_into().ok()?;

        let path_segments = value.path_segments()?;
        let mut path = path_segments.map(|e| e.to_owned()).collect::<Vec<String>>();
        value.fragment().inspect(|e| path.push(e.to_string()));

        Some(DirectDownloadLink { hosting, path })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reports_missing_direct_link_path() {
        let link = DirectDownloadLink {
            hosting: HostingSubset::Pixeldrain,
            path: Vec::new(),
        };

        let error = link.get().await.unwrap_err();
        assert!(matches!(
            error,
            DirectLinkError::Pixeldrain(PixeldrainLinkError::MissingFileId)
        ));
    }

    #[tokio::test]
    async fn reports_malformed_mega_link() {
        let link = DirectDownloadLink {
            hosting: HostingSubset::Mega,
            path: vec!["file".to_string()],
        };

        let error = link.get().await.unwrap_err();
        assert!(matches!(
            error,
            DirectLinkError::Mega(MegaLinkError::MissingNodeId)
        ));
    }
}
