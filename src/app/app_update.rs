//! Checks GitHub for a newer release of the app itself.
//!
//! Betas compare with their pre-release suffix (`2.0.0-beta.3` <
//! `2.0.0-beta.4` < `2.0.0`), so a beta user hears about the next beta and
//! about the final release. Drafts are skipped; pre-release versions count only
//! while the running build is a pre-release itself. That is decided by the
//! version suffix, not GitHub's "pre-release" flag, which the beta releases do
//! not set.

use std::fmt;

use semver::Version;
use serde::Deserialize;

const RELEASES_API: &str = "https://api.github.com/repos/farvend/F95-Manager/releases?per_page=30";

/// A release that is newer than the running build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableUpdate {
    pub version: Version,
    pub url: String,
}

#[derive(Debug)]
pub enum UpdateCheckError {
    Config(crate::net::NetError),
    Request(reqwest::Error),
    Status(reqwest::StatusCode),
    Body(reqwest::Error),
    Json(serde_json::Error),
    CurrentVersion(semver::Error),
}

impl fmt::Display for UpdateCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpdateCheckError::Config(error) => write!(f, "{error}"),
            UpdateCheckError::Request(error) => write!(f, "GitHub request failed: {error}"),
            UpdateCheckError::Status(status) => write!(f, "GitHub answered {status}"),
            UpdateCheckError::Body(error) => write!(f, "GitHub answer could not be read: {error}"),
            UpdateCheckError::Json(error) => write!(f, "GitHub answer is malformed: {error}"),
            UpdateCheckError::CurrentVersion(error) => {
                write!(f, "the app's own version is not semver: {error}")
            }
        }
    }
}

impl std::error::Error for UpdateCheckError {}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    draft: bool,
}

/// Ask GitHub for releases and return the newest one above the running
/// version, if any.
pub async fn check() -> Result<Option<AvailableUpdate>, UpdateCheckError> {
    let current = Version::parse(env!("CARGO_PKG_VERSION")).map_err(UpdateCheckError::CurrentVersion)?;
    let client = crate::net::client().map_err(UpdateCheckError::Config)?;
    let response = client
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(UpdateCheckError::Request)?;
    if !response.status().is_success() {
        return Err(UpdateCheckError::Status(response.status()));
    }
    let body = response.text().await.map_err(UpdateCheckError::Body)?;
    let releases: Vec<Release> = serde_json::from_str(&body).map_err(UpdateCheckError::Json)?;
    Ok(newest_update(&current, releases))
}

fn newest_update(current: &Version, releases: Vec<Release>) -> Option<AvailableUpdate> {
    let accept_prereleases = !current.pre.is_empty();
    releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            let tag = release.tag_name.trim_start_matches(['v', 'V']);
            match Version::parse(tag) {
                Ok(version) => Some(AvailableUpdate { version, url: release.html_url }),
                Err(error) => {
                    // Old tags like "2.0.0-beta.1" without "v" still parse;
                    // anything else is not a version tag at all.
                    log::debug!("skipping release tag {:?}: {error}", release.tag_name);
                    None
                }
            }
        })
        .filter(|update| accept_prereleases || update.version.pre.is_empty())
        .filter(|update| update.version > *current)
        .max_by(|a, b| a.version.cmp(&b.version))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, draft: bool) -> Release {
        Release {
            tag_name: tag.to_string(),
            html_url: format!("https://example.test/{tag}"),
            draft,
        }
    }

    fn newest(current: &str, releases: Vec<Release>) -> Option<String> {
        newest_update(&Version::parse(current).expect("current"), releases)
            .map(|update| update.version.to_string())
    }

    #[test]
    fn beta_user_hears_about_next_beta_and_final_release() {
        let releases = vec![
            release("v2.0.0-beta.2", false),
            release("v2.0.0-beta.4", false),
            release("v1.9.0", false),
        ];
        assert_eq!(newest("2.0.0-beta.3", releases).as_deref(), Some("2.0.0-beta.4"));

        let releases = vec![release("v2.0.0-beta.4", false), release("v2.0.0", false)];
        assert_eq!(newest("2.0.0-beta.3", releases).as_deref(), Some("2.0.0"));
    }

    #[test]
    fn stable_user_ignores_prereleases() {
        let releases = vec![release("v2.0.0-beta.4", false), release("v1.9.0", false)];
        assert_eq!(newest("1.9.0", releases), None);
    }

    #[test]
    fn drafts_and_older_or_equal_versions_are_not_updates() {
        let releases = vec![
            release("v2.0.0-beta.9", true),
            release("v2.0.0-beta.3", false),
            release("not-a-version", false),
        ];
        assert_eq!(newest("2.0.0-beta.3", releases), None);
    }
}
