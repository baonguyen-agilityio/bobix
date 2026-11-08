// "A new version is available": reads the latest release from GitHub's public API (no account, nothing sent but the
// request itself) and compares its version with the running one. Pre-releases are never offered. Port of
// the 0.1 (Electron) app's core/update.js.
use fancy_regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

pub const REPO: &str = "baonguyen-agilityio/bobix";

fn parse(v: &str) -> Option<[u64; 3]> {
    static V: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^v?(\d+)\.(\d+)\.(\d+)(?:-[0-9A-Za-z.-]+)?$").unwrap());
    let m = V.captures(v).ok()??;
    Some([m[1].parse().ok()?, m[2].parse().ok()?, m[3].parse().ok()?])
}

// True when `latest` (a tag like "v0.1.2") is a newer release than `current` ("0.1.1" or "0.1.1-rc.3").
pub fn is_newer(latest: &str, current: &str) -> bool {
    let (Some(a), Some(b)) = (parse(latest), parse(current)) else { return false };
    if a != b { return a > b; }
    current.contains('-') && !latest.contains('-') // 0.1.1 is newer than 0.1.1-rc.3
}

// GitHub's answer → (version, release page) when it is a newer release.
pub fn pick(r: &Value, current: &str) -> Option<(String, String)> {
    let tag = r["tag_name"].as_str()?;
    if r["draft"] == true || r["prerelease"] == true || !is_newer(tag, current) { return None; }
    let page = format!("https://github.com/{REPO}/releases/");
    let url = r["html_url"].as_str().filter(|u| u.starts_with(&page)).map(String::from).unwrap_or(page + "latest");
    Some((tag.trim_start_matches('v').to_string(), url))
}

// Never fails loudly: offline or GitHub down → None.
// ponytail: the system's curl (macOS, Windows 10+, Linux) instead of an HTTP client in the app.
pub fn check_latest(current: &str) -> Option<(String, String)> {
    let ua = format!("User-Agent: Bobix/{current}");
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let r = crate::procs::exec("curl", &["-fsSL", "--max-time", "10", "-H", "Accept: application/vnd.github+json", "-H", &ua, &url], 15, &[]);
    if !r.ok { return None; }
    pick(&serde_json::from_str(&r.out).ok()?, current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn newer_releases_only() {
        assert!(is_newer("v0.1.2", "0.1.1"));
        assert!(!is_newer("v0.1.1", "0.1.1"));
        assert!(!is_newer("v0.1.0", "0.1.1"));
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("v0.1.1", "0.1.1-rc.3"), "the release beats its own release candidates");
        assert!(!is_newer("garbage", "0.1.1"));
        let page = format!("https://github.com/{REPO}/releases/tag/v0.1.2");
        assert_eq!(pick(&json!({ "tag_name": "v0.1.2", "html_url": page }), "0.1.1"), Some(("0.1.2".into(), page)));
        assert_eq!(pick(&json!({ "tag_name": "v0.1.2", "html_url": "https://evil.example/x" }), "0.1.1").unwrap().1,
            format!("https://github.com/{REPO}/releases/latest"), "only opens the project's own release page");
        assert_eq!(pick(&json!({ "tag_name": "v0.1.2", "prerelease": true }), "0.1.1"), None);
        assert_eq!(pick(&json!({ "message": "Not Found" }), "0.1.1"), None);
    }

    // GitHub's real answer (a public GET): cargo test live_update -- --ignored
    #[test]
    #[ignore]
    fn live_update() {
        let (v, url) = check_latest("0.0.1").expect("a release newer than 0.0.1");
        assert!(parse(&v).is_some() && url.starts_with(&format!("https://github.com/{REPO}/releases/")), "{v} {url}");
        assert_eq!(check_latest("999.0.0"), None);
    }
}
