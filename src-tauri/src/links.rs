// Which session a Claude desktop session was started from ("side session"), read from that app's own session file:
// <sessions dir>/<org>/<user>/<local_id>.json, field spawnedFrom.sessionId. Only that field is read. Same as
// src/core/links.js; the issue/session list for the Work tab comes with it later.
use std::{fs, path::{Path, PathBuf}};

pub fn is_host_id(id: &str) -> bool {
    id.strip_prefix("local_").is_some_and(|r| (1..=64).contains(&r.len()) && r.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
}

// Verified on macOS. Windows and the Linux beta follow Electron's usual app-data folders (not verified yet).
pub fn sessions_dir() -> PathBuf {
    let home = crate::paths::home();
    if cfg!(target_os = "macos") { return home.join("Library/Application Support/Claude/claude-code-sessions"); }
    if cfg!(windows) {
        return std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| home.join("AppData/Roaming")).join("Claude/claude-code-sessions");
    }
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config")).join("Claude/claude-code-sessions")
}

fn dirs(d: &Path) -> Vec<PathBuf> {
    fs::read_dir(d).map(|r| r.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()).collect()).unwrap_or_default()
}

pub fn parent_of(id: &str, root: &Path) -> Option<String> {
    if !is_host_id(id) { return None; }
    for org in dirs(root) {
        for user in dirs(&org) {
            let Ok(text) = fs::read_to_string(user.join(format!("{id}.json"))) else { continue };
            let Ok(o) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
            let p = o["spawnedFrom"]["sessionId"].as_str()?;
            return (is_host_id(p) && p != id).then(|| p.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_session_knows_its_parent() {
        let root = std::env::temp_dir().join(format!("bobix-ccd-{}", std::process::id()));
        let dir = root.join("org-1/user-1");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("local_child-1.json"), r#"{"sessionId":"local_child-1","title":"secret","spawnedFrom":{"sessionId":"local_parent-1","sideSession":true}}"#).unwrap();
        fs::write(dir.join("local_parent-1.json"), r#"{"sessionId":"local_parent-1"}"#).unwrap();
        fs::write(dir.join("local_weird.json"), r#"{"spawnedFrom":{"sessionId":"../../etc/passwd"}}"#).unwrap();
        assert_eq!(parent_of("local_child-1", &root).as_deref(), Some("local_parent-1"));
        assert_eq!(parent_of("local_parent-1", &root), None);
        assert_eq!(parent_of("local_weird", &root), None);
        assert_eq!(parent_of("local_missing", &root), None);
        assert_eq!(parent_of("../org-1/user-1/local_child-1", &root), None);
        assert_eq!(parent_of("local_child-1", &root.join("nope")), None);
        let _ = fs::remove_dir_all(&root);
    }
}
