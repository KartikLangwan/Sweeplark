//! Update check against GitHub Releases. Never installs anything without the user saying yes.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ai::download::{self, Progress};
use crate::branding::UPDATE_REPO;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
    pub published_at: String,
    pub page_url: String,
    pub setup_url: Option<String>,
    pub setup_size: u64,
    pub setup_sha256: Option<String>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    body: Option<String>,
    published_at: Option<String>,
    html_url: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

fn parse(v: &str) -> Vec<u64> {
    v.trim_start_matches(['v', 'V']).split(['.', '-']).take(3).map(|p| p.parse().unwrap_or(0)).collect()
}

/// `true` if `latest` is newer than `current` ("1.2" > "1.1.0").
pub fn is_newer(latest: &str, current: &str) -> bool {
    let (mut a, mut b) = (parse(latest), parse(current));
    a.resize(3, 0);
    b.resize(3, 0);
    a > b
}

/// Latest published (non-draft, non-prerelease) release, if newer than `current`.
pub fn check(current: &str) -> Result<Option<UpdateInfo>> {
    let url = format!("https://api.github.com/repos/{UPDATE_REPO}/releases/latest");
    let mut resp = download::web_agent()
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| Error::Other(format!("update check failed: {e}")))?;
    match resp.status().as_u16() {
        200 => {}
        404 => return Ok(None), // no public release yet
        s => return Err(Error::Other(format!("update check failed: HTTP {s}"))),
    }
    let r: Release = resp.body_mut().read_json().map_err(|e| Error::Other(format!("bad release info: {e}")))?;
    if r.draft || r.prerelease || !is_newer(&r.tag_name, current) {
        return Ok(None);
    }
    let setup = r.assets.iter().find(|a| a.name.to_lowercase().ends_with("setup.exe"));
    Ok(Some(UpdateInfo {
        version: r.tag_name.trim_start_matches('v').to_string(),
        notes: r.body.unwrap_or_default().chars().take(1500).collect(),
        published_at: r.published_at.unwrap_or_default(),
        page_url: r.html_url,
        setup_url: setup.map(|a| a.browser_download_url.clone()),
        setup_size: setup.map(|a| a.size).unwrap_or(0),
        setup_sha256: setup.and_then(|a| a.digest.as_deref()).and_then(|d| d.strip_prefix("sha256:")).map(String::from),
    }))
}

/// Download the installer (SHA-256 checked) into `dir`. Returns its path.
pub fn download_setup(info: &UpdateInfo, dir: &Path, progress: Progress) -> Result<PathBuf> {
    let url = info.setup_url.as_deref().ok_or_else(|| Error::NotFound("installer in this release".into()))?;
    let sha = info.setup_sha256.as_deref().ok_or_else(|| Error::Other("release has no checksum; refusing".into()))?;
    let dest = dir.join(format!("Sweeplark-{}-Setup.exe", info.version));
    download::download(url, &dest, Some(info.setup_size), sha, progress)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("v1.2", "1.1.0"));
        assert!(is_newer("1.1.1", "1.1.0"));
        assert!(!is_newer("v1.1.0", "1.1.0"));
        assert!(!is_newer("1.0.9", "1.1.0"));
        assert!(is_newer("2.0", "1.9.9"));
    }
}
