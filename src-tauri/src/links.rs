// Which session a Claude desktop session was started from ("side session"), read from that app's own session file:
// <sessions dir>/<org>/<user>/<local_id>.json, field spawnedFrom.sessionId. Only that field is read. Same as
// the 0.1 (Electron) app's core/links.js, with the list of Claude desktop sessions the Work tab reads.
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

// None: no readable session file yet. Some(None): the file is there and names no parent. The Claude app writes the file
// with spawnedFrom already in it (measured: both show up within 10 ms), so a file without it is not a side session.
pub fn parent_of(id: &str, root: &Path) -> Option<Option<String>> {
    if !is_host_id(id) { return None; }
    for org in dirs(root) {
        for user in dirs(&org) {
            let Ok(text) = fs::read_to_string(user.join(format!("{id}.json"))) else { continue };
            let Ok(o) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
            return Some(o["spawnedFrom"]["sessionId"].as_str().filter(|p| is_host_id(p) && *p != id).map(String::from));
        }
    }
    None
}

// A Claude desktop session that is not archived (at: last activity, ms). Only these fields are kept.
#[derive(Clone, Debug)]
pub struct ClaudeSession { pub id: String, pub cwd: String, pub title: String, pub at: f64 }

// Claude desktop's sessions. A file is parsed again only when it changed (cache: file -> (modified, record)).
pub fn claude_sessions(root: &Path) -> Vec<ClaudeSession> {
    use std::{collections::HashMap, sync::Mutex, time::SystemTime};
    static SEEN: Mutex<Option<HashMap<PathBuf, (SystemTime, Option<ClaudeSession>)>>> = Mutex::new(None);
    let mut guard = SEEN.lock().unwrap();
    let seen = guard.get_or_insert_with(HashMap::new);
    let (mut out, mut files) = (vec![], std::collections::HashSet::new());
    for org in dirs(root) {
        for user in dirs(&org) {
            let Ok(rd) = fs::read_dir(&user) else { continue };
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                let Some(id) = n.strip_suffix(".json").filter(|id| is_host_id(id)) else { continue };
                let file = e.path();
                let Ok(m) = fs::metadata(&file).and_then(|m| m.modified()) else { continue };
                files.insert(file.clone());
                if seen.get(&file).is_none_or(|c| c.0 != m) {
                    let o: Option<serde_json::Value> = fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok());
                    let ms = m.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0);
                    let rec = o.filter(|o| o["isArchived"] != true).and_then(|o| Some(ClaudeSession {
                        id: id.into(), cwd: o["cwd"].as_str()?.into(), title: o["title"].as_str()?.into(),
                        at: Some(crate::js::js_number(&o["lastActivityAt"])).filter(|n| *n != 0.0 && n.is_finite()).unwrap_or(ms),
                    }));
                    seen.insert(file.clone(), (m, rec));
                }
                if let Some(r) = &seen[&file].1 { out.push(r.clone()); }
            }
        }
    }
    seen.retain(|f, _| files.contains(f));
    out
}

// The newest of those sessions in folder `dir` whose name `named` accepts (an issue's, see model::names_issue).
pub fn issue_session(list: &[ClaudeSession], dir: &str, named: impl Fn(&str) -> bool) -> Option<String> {
    list.iter().filter(|s| crate::paths::same_dir(&s.cwd, dir) && named(&s.title)).max_by(|a, b| a.at.total_cmp(&b.at)).map(|s| s.id.clone())
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
        assert_eq!(parent_of("local_child-1", &root), Some(Some("local_parent-1".into())));
        assert_eq!(parent_of("local_parent-1", &root), Some(None));
        assert_eq!(parent_of("local_weird", &root), Some(None));
        assert_eq!(parent_of("local_missing", &root), None);
        assert_eq!(parent_of("../org-1/user-1/local_child-1", &root), None);
        assert_eq!(parent_of("local_child-1", &root.join("nope")), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_issues_claude_session_is_found_by_its_name() {
        let root = std::env::temp_dir().join(format!("bobix-cs-{}", std::process::id()));
        let u = root.join("org/user");
        fs::create_dir_all(&u).unwrap();
        let put = |id: &str, o: serde_json::Value| fs::write(u.join(format!("local_{id}.json")), o.to_string()).unwrap();
        put("a", serde_json::json!({ "cwd": "/src/shop", "title": "shop #23 Item list...", "lastActivityAt": 100 }));
        put("b", serde_json::json!({ "cwd": "/src/shop", "title": "[SHO-23] Item list...", "lastActivityAt": 300 }));
        put("c", serde_json::json!({ "cwd": "/src/shop", "title": "shop #23 newer but archived", "lastActivityAt": 900, "isArchived": true }));
        put("d", serde_json::json!({ "cwd": "/src/shop", "title": "shop #231 other issue", "lastActivityAt": 950 }));
        put("e", serde_json::json!({ "cwd": "/src/other", "title": "shop #23 other folder", "lastActivityAt": 990 }));
        put("f", serde_json::json!({ "cwd": "/src/shop", "title": "shop #24", "lastActivityAt": 10 }));
        fs::write(u.join("notes.json"), "{}").unwrap();
        let named = |iid| move |t: &str| crate::model::names_issue(t, "SHO", "g/shop", iid);
        let list = claude_sessions(&root);
        assert_eq!(list.len(), 5);
        assert_eq!(issue_session(&list, "/src/shop", named(23)).as_deref(), Some("local_b"));
        assert_eq!(issue_session(&list, "/src/shop", named(24)).as_deref(), Some("local_f"));
        assert_eq!(issue_session(&list, "/src/shop", named(25)), None);
        put("b", serde_json::json!({ "cwd": "/src/shop", "title": "renamed by hand", "lastActivityAt": 400 }));
        let f = fs::File::options().write(true).open(u.join("local_b.json")).unwrap();
        f.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(issue_session(&claude_sessions(&root), "/src/shop", named(23)).as_deref(), Some("local_a"), "a changed file is read again");
        let _ = fs::remove_dir_all(&root);
    }
}
