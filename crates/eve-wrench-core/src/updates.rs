use serde::Deserialize;

use crate::esi::AGENT;

const GITHUB_REPO: &str = "eve-wrench/eve-wrench-app";

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub release_url: String,
    pub release_notes: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ReleasePayload {
    List(Vec<Release>),
    Single(Release),
}

// A semver pre-release suffix ("0.3.0-preview.1") marks preview builds.
pub fn is_preview_version(version: &str) -> bool {
    version.contains('-')
}

// Stable builds only look at the latest stable release (GitHub's
// /releases/latest never returns pre-releases). Preview builds scan the full
// release list so testers are notified about newer previews AND the next
// stable release.
pub fn check_for_update() -> Result<Option<UpdateInfo>, String> {
    let url = if is_preview_version(APP_VERSION) {
        format!(
            "https://api.github.com/repos/{}/releases?per_page=15",
            GITHUB_REPO
        )
    } else {
        format!(
            "https://api.github.com/repos/{}/releases/latest",
            GITHUB_REPO
        )
    };

    let payload: ReleasePayload = AGENT
        .get(&url)
        .call()
        .map_err(|e| format!("Failed to check for updates: {}", e))?
        .body_mut()
        .read_json()
        .map_err(|e| format!("Failed to parse release: {}", e))?;

    let releases = match payload {
        ReleasePayload::List(list) => list,
        ReleasePayload::Single(release) => vec![release],
    };

    let mut best: Option<UpdateInfo> = None;
    for release in releases.into_iter().filter(|r| !r.draft) {
        let version = release.tag_name.trim_start_matches('v');
        let newer_than_best = best
            .as_ref()
            .is_none_or(|b| is_newer_version(version, &b.latest_version));
        if is_newer_version(version, APP_VERSION) && newer_than_best {
            best = Some(UpdateInfo {
                current_version: APP_VERSION.to_string(),
                latest_version: version.to_string(),
                release_url: release.html_url,
                release_notes: release.body.unwrap_or_default(),
            });
        }
    }

    Ok(best)
}

// Semver-ordered comparison: numeric cores first; on equal cores a stable
// release beats any pre-release of it, and pre-releases compare by their
// numeric identifiers ("preview.2" > "preview.1").
fn is_newer_version(latest: &str, current: &str) -> bool {
    fn split(v: &str) -> (Vec<u32>, Option<Vec<u32>>) {
        let mut parts = v.splitn(2, '-');
        let core = parts.next().unwrap_or("");
        let pre = parts.next();
        let nums = |s: &str| {
            s.split('.')
                .filter_map(|p| p.parse().ok())
                .collect::<Vec<u32>>()
        };
        (nums(core), pre.map(nums))
    }

    fn compare(l: &[u32], c: &[u32]) -> std::cmp::Ordering {
        (0..l.len().max(c.len()))
            .map(|i| {
                l.get(i)
                    .copied()
                    .unwrap_or(0)
                    .cmp(&c.get(i).copied().unwrap_or(0))
            })
            .find(|o| o.is_ne())
            .unwrap_or(std::cmp::Ordering::Equal)
    }

    let (latest_core, latest_pre) = split(latest);
    let (current_core, current_pre) = split(current);

    match compare(&latest_core, &current_core) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => match (latest_pre, current_pre) {
            (None, Some(_)) => true, // 0.3.0 beats 0.3.0-preview.1
            (Some(l), Some(c)) => compare(&l, &c).is_gt(),
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer_version("0.3.0", "0.2.1"));
        assert!(!is_newer_version("0.2.1", "0.2.1"));
        assert!(!is_newer_version("0.2.0", "0.2.1"));
        // Pre-release ordering
        assert!(!is_newer_version("0.2.1", "0.3.0-preview.1"));
        assert!(is_newer_version("0.3.0", "0.3.0-preview.1"));
        assert!(!is_newer_version("0.3.0-preview.1", "0.3.0"));
        assert!(is_newer_version("0.3.0-preview.2", "0.3.0-preview.1"));
        assert!(!is_newer_version("0.3.0-preview.1", "0.3.0-preview.2"));
        assert!(is_newer_version("0.3.0-preview.1", "0.2.1"));
    }

    #[test]
    fn preview_detection() {
        assert!(is_preview_version("0.3.0-preview.1"));
        assert!(is_preview_version("1.0.0-rc.2"));
        assert!(!is_preview_version("0.2.1"));
    }
}
