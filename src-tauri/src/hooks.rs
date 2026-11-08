// Installs / removes Bobix hooks in ~/.claude/settings.json without touching anyone else's hooks. Port of
// the 0.1 (Electron) app's core/hooks.js: the same entries, so either build reads and replaces what the other wrote.
use serde_json::{json, Map, Value};
use std::{fs, path::{Path, PathBuf}, sync::LazyLock};

pub const EVENTS: [&str; 8] = ["SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PermissionRequest", "Notification", "Stop", "SessionEnd"];
const TOOL_EVENTS: [&str; 3] = ["PreToolUse", "PostToolUse", "PermissionRequest"];
const MARK: &str = "bobix";
// The app was called Session Guild before: its hooks are ours too, and get replaced on the next install.
const OLD_MARK: &str = "session-guild";
// PermissionRequest waits for an answer from Bobix (node forwarder only), so it can't run in the background. The app
// decides how long it waits; this is only the hard cap Claude Code enforces: a day, in seconds.
const ANSWER_EVENT: &str = "PermissionRequest";
pub const ANSWER_TIMEOUT: u64 = 86400;
// SessionStart runs in the foreground (still 5 s at most) so Bobix can name a session started from an issue.
const TITLE_EVENT: &str = "SessionStart";
const KEEP_BACKUPS: usize = 3;

pub fn claude_dir(home: &Path) -> PathBuf { home.join(".claude") }
fn app_dir(home: &Path) -> PathBuf { claude_dir(home).join(MARK) }
pub fn settings_path(home: &Path) -> PathBuf { claude_dir(home).join("settings.json") }
pub fn hook_target(home: &Path) -> PathBuf { app_dir(home).join("hook.js") }
fn token_path(home: &Path) -> PathBuf { app_dir(home).join("token") }
fn header_path(home: &Path) -> PathBuf { app_dir(home).join("header") } // "X-Bobix: <token>", for curl -H @file

pub const HOOK_JS: &str = include_str!("../../src/hook/hook.js");

// Exact fingerprints only, so a foreign hook that merely mentions "bobix" or a local port is never touched.
static LEGACY: LazyLock<fancy_regex::Regex> = LazyLock::new(|| fancy_regex::Regex::new(
    r#"^curl -s -m 1 -o /dev/null -X POST -H "Content-Type: application/json" --data-binary @- http://127\.0\.0\.1:[0-9]+/event( 2>/dev/null)? \|\| true$"#).unwrap());

fn mark_of(h: &Value) -> Option<&'static str> {
    let o = h.as_object()?;
    let args: Vec<&str> = o.get("args").and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
    let cmd = o.get("command").and_then(|c| c.as_str());
    for m in [MARK, OLD_MARK] {
        if args.iter().any(|a| a.replace('\\', "/").ends_with(&format!("/.claude/{m}/hook.js"))) { return Some(m); } // node
        if cmd == Some("curl") && args.iter().any(|a| a.ends_with(&format!("/event#{m}"))) { return Some(m); }
        if cmd == Some("powershell") && args.iter().any(|a| a.ends_with(&format!("# {m}"))) { return Some(m); }
    }
    // shell-string hook from early builds
    let legacy = cmd.is_some_and(|c| args.is_empty() && LEGACY.is_match(c.trim()).unwrap_or(false));
    legacy.then_some(OLD_MARK)
}
pub fn is_ours(h: &Value) -> bool { mark_of(h).is_some() }

// Exec form (command + args, no shell) works the same in bash, zsh, PowerShell and cmd.
pub fn hook_entry(port: u16, node_path: Option<&str>, windows: bool, home: &Path, event: &str) -> Value {
    if let Some(node) = node_path {
        let mut e = json!({ "type": "command", "async": true, "timeout": 5, "command": node, "args": [hook_target(home).to_string_lossy(), port.to_string()] });
        let o = e.as_object_mut().unwrap();
        if event == ANSWER_EVENT { o.shift_remove("async"); o.insert("timeout".into(), json!(ANSWER_TIMEOUT)); }
        if event == TITLE_EVENT { o.shift_remove("async"); }
        return e;
    }
    let url = format!("http://127.0.0.1:{port}/event");
    if windows {
        // No node: PowerShell forwards stdin as UTF-8.
        let tok = token_path(home).to_string_lossy().replace('\'', "''");
        let cmd = format!("try {{ [Console]::InputEncoding = [Text.Encoding]::UTF8; $t = (Get-Content -Raw '{tok}').Trim(); \
Invoke-RestMethod -Uri '{url}' -Method Post -ContentType 'application/json; charset=utf-8' -Headers @{{'x-bobix'=$t}} \
-Body ([Text.Encoding]::UTF8.GetBytes([Console]::In.ReadToEnd())) -TimeoutSec 2 | Out-Null }} catch {{}} # {MARK}");
        return json!({ "type": "command", "async": true, "timeout": 5, "command": "powershell", "args": ["-NoProfile", "-Command", cmd] });
    }
    json!({ "type": "command", "async": true, "timeout": 5, "command": "curl", "args": ["-s", "-m", "1", "-o", "/dev/null", "-X", "POST", "-H", "Content-Type: application/json",
        "-H", format!("@{}", header_path(home).to_string_lossy()), "--data-binary", "@-", format!("{url}#{MARK}")] })
}

// Invalid JSON is an error: never overwrite a file we cannot read.
fn read_settings(file: &Path) -> Result<Map<String, Value>, String> {
    let Ok(txt) = fs::read_to_string(file) else { return Ok(Map::new()) };
    if txt.trim().is_empty() { return Ok(Map::new()); }
    match serde_json::from_str::<Value>(&txt).map_err(|e| e.to_string())? {
        Value::Object(o) => Ok(o),
        _ => Err("settings.json is not a JSON object".into()),
    }
}

// Writes through a symlink (dotfiles repos) and keeps the file's permissions (it may hold tokens in "env").
fn write_atomic(file: &Path, obj: &Value) -> Result<(), String> {
    let real = fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    if let Some(d) = real.parent() { fs::create_dir_all(d).map_err(|e| e.to_string())?; }
    let tmp = PathBuf::from(format!("{}.{}.tmp", real.display(), std::process::id()));
    let text = serde_json::to_string_pretty(obj).map_err(|e| e.to_string())? + "\n";
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&real).map(|m| m.permissions().mode() & 0o777).unwrap_or(0o600);
        fs::set_permissions(&tmp, fs::Permissions::from_mode(mode)).map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp, &real).map_err(|e| e.to_string())
}

fn groups<'a>(hooks: &'a Value, ev: &str) -> &'a [Value] { hooks.get(ev).and_then(|g| g.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) }
fn entries(g: &Value) -> &[Value] { g.get("hooks").and_then(|h| h.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) }

// Removes our hooks; drops groups/events that become empty only because of us.
pub fn strip(settings: &mut Map<String, Value>) {
    let Some(Value::Object(hooks)) = settings.get_mut("hooks") else { return };
    let keys: Vec<String> = hooks.keys().cloned().collect();
    for ev in keys {
        let Some(Value::Array(list)) = hooks.get(&ev) else { continue };
        let before = list.len();
        let kept: Vec<Value> = list.iter().filter_map(|g| {
            let es = entries(g);
            let left: Vec<Value> = es.iter().filter(|h| !is_ours(h)).cloned().collect();
            if left.len() == es.len() { return Some(g.clone()); }
            if left.is_empty() { return None; }
            let mut g = g.clone();
            g["hooks"] = Value::Array(left);
            Some(g)
        }).collect();
        if kept.is_empty() && before > 0 { hooks.shift_remove(&ev); } else { hooks.insert(ev, Value::Array(kept)); }
    }
    if hooks.is_empty() { settings.shift_remove("hooks"); }
}

// "2026-11-07T10:25:22.918Z" (UTC), as JavaScript's toISOString.
pub fn iso(ms: u64) -> String {
    let (secs, ms) = (ms / 1000, ms % 1000);
    let (days, rem) = ((secs / 86400) as i64, secs % 86400);
    // civil_from_days (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{ms:03}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}
// The same with ":" and "." made "-", as the Electron build names its backups.
fn iso_stamp(ms: u64) -> String { iso(ms).replace([':', '.'], "-") }

fn backup(file: &Path) -> Option<PathBuf> {
    if !file.exists() { return None; }
    let name = file.file_name()?.to_string_lossy().into_owned();
    let prefix = format!("{name}.bak-{MARK}-");
    let b = file.with_file_name(format!("{prefix}{}", iso_stamp(crate::js::now_ms())));
    fs::copy(file, &b).ok()?; // copies the permissions too
    // Keep only the newest few of our own backups.
    let dir = file.parent()?;
    let mut ours: Vec<String> = fs::read_dir(dir).ok()?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|f| f.starts_with(&prefix)).collect();
    ours.sort();
    let drop = ours.len().saturating_sub(KEEP_BACKUPS);
    for f in &ours[..drop] { let _ = fs::remove_file(dir.join(f)); }
    Some(b)
}

#[derive(Debug)]
pub struct Installed { pub file: PathBuf, pub backup: Option<PathBuf>, pub mode: &'static str }

pub fn install(port: u16, node_path: Option<&str>, home: &Path) -> Result<Installed, String> {
    let file = settings_path(home);
    let mut settings = read_settings(&file)?;
    strip(&mut settings);
    if settings.get("hooks").is_some_and(|h| !h.is_object()) { return Err("\"hooks\" in settings.json is not an object; fix it by hand first".into()); }
    crate::paths::ensure_token_in(home).map_err(|e| e.to_string())?;
    if node_path.is_some() { fs::write(hook_target(home), HOOK_JS).map_err(|e| e.to_string())?; } // stable path: survives app updates and moves
    let hooks = settings.entry("hooks").or_insert_with(|| json!({}));
    for ev in EVENTS {
        if hooks.get(ev).is_some_and(|g| !g.is_array()) { return Err(format!("hooks.{ev} in settings.json is not a list; fix it by hand first")); }
        let entry = hook_entry(port, node_path, cfg!(windows), home, ev);
        let group = if TOOL_EVENTS.contains(&ev) { json!({ "matcher": "*", "hooks": [entry] }) } else { json!({ "hooks": [entry] }) };
        let mut list = groups(hooks, ev).to_vec();
        list.push(group);
        hooks[ev] = Value::Array(list);
    }
    let bak = backup(&file);
    write_atomic(&file, &Value::Object(settings))?;
    let _ = fs::remove_dir_all(claude_dir(home).join(OLD_MARK)); // old build's forwarder + token
    Ok(Installed { file, backup: bak, mode: if node_path.is_some() { "node" } else if cfg!(windows) { "powershell" } else { "curl" } })
}

pub fn uninstall(home: &Path) -> Result<Option<PathBuf>, String> {
    let file = settings_path(home);
    let mut bak = None;
    if file.exists() {
        let mut settings = read_settings(&file)?;
        strip(&mut settings);
        bak = backup(&file);
        write_atomic(&file, &Value::Object(settings))?;
    }
    for f in [hook_target(home), header_path(home)] { let _ = fs::remove_file(f); } // token stays: the running app uses it
    Ok(bak)
}

// What is installed, as status() in hooks.js returns it (the pages read these fields).
pub fn status(home: &Path) -> Value {
    let file = settings_path(home);
    let settings = match read_settings(&file) {
        Ok(s) => s,
        Err(e) => return json!({ "file": file, "error": format!("settings.json: {e}"), "events": [], "complete": false, "mode": null, "port": null, "stale": false }),
    };
    let hooks = settings.get("hooks").cloned().unwrap_or(Value::Null);
    let ours = |ev: &str| -> Vec<Value> { groups(&hooks, ev).iter().flat_map(entries).filter(|h| is_ours(h)).cloned().collect() };
    let events: Vec<&str> = EVENTS.iter().copied().filter(|ev| !ours(ev).is_empty()).collect();
    let one = EVENTS.iter().find_map(|ev| ours(ev).into_iter().next());
    let mode = one.as_ref().map(|o| match o["command"].as_str() {
        Some("curl") => "curl",
        Some("powershell") => "powershell",
        _ if o["args"].is_array() => "node",
        _ => "legacy",
    });
    static PORT: LazyLock<fancy_regex::Regex> = LazyLock::new(|| fancy_regex::Regex::new(r"127\.0\.0\.1:([0-9]+)").unwrap());
    let port = one.as_ref().and_then(|o| if mode == Some("node") { o["args"][1].as_str().and_then(|p| p.parse::<u64>().ok()) } else {
        PORT.captures(&o.to_string()).ok().flatten().and_then(|c| c[1].parse().ok())
    }).filter(|p| *p > 0);
    let node = if mode == Some("node") { one.as_ref().and_then(|o| o["command"].as_str()).map(String::from) } else { None };
    // node mode records an absolute node path: it goes stale when nvm / Homebrew remove that version.
    let stale = node.as_ref().is_some_and(|n| !Path::new(n).exists());
    // Installed by the old build, before Bobix could answer permission requests, or before it named sessions.
    let ask = ours(ANSWER_EVENT).into_iter().next();
    let start = ours(TITLE_EVENT).into_iter().next();
    let bg = |h: &Value| h.get("async").is_some_and(|a| a.as_bool() != Some(false) && !a.is_null());
    let outdated = one.as_ref().is_some_and(|o| mark_of(o) != Some(MARK) || (mode == Some("node")
        && (ask.as_ref().is_some_and(|a| bg(a) || !(a["timeout"].as_f64().unwrap_or(0.0) >= ANSWER_TIMEOUT as f64)) || start.as_ref().is_some_and(bg))));
    json!({ "file": file, "events": events, "complete": events.len() == EVENTS.len(), "mode": mode, "port": port, "nodePath": node, "stale": stale, "outdated": outdated })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home(tag: &str) -> PathBuf {
        let h = std::env::temp_dir().join(format!("bobix-hooks-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&h);
        fs::create_dir_all(claude_dir(&h)).unwrap();
        h
    }
    fn read(file: &Path) -> Value { serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap() }

    #[test]
    fn install_keeps_foreign_hooks_and_is_idempotent() {
        let h = home("keep");
        let file = settings_path(&h);
        let foreign = json!({ "type": "command", "command": "echo hi" });
        let fork = json!({ "type": "command", "command": "node", "args": [h.join("code/bobix-fork/lint.js")] });
        let other_curl = json!({ "type": "command", "command": "curl -s --data-binary @- http://127.0.0.1:9999/event" });
        let legacy = json!({ "type": "command", "command": "curl -s -m 1 -o /dev/null -X POST -H \"Content-Type: application/json\" --data-binary @- http://127.0.0.1:4747/event 2>/dev/null || true" });
        fs::write(&file, json!({ "model": "x", "hooks": { "Stop": [{ "hooks": [foreign, fork, other_curl] }, { "hooks": [legacy] }] } }).to_string()).unwrap();
        install(5000, Some("/usr/bin/node"), &h).unwrap();
        install(5000, Some("/usr/bin/node"), &h).unwrap();
        let s = read(&file);
        assert_eq!(s["model"], "x");
        assert_eq!(s["hooks"]["Stop"][0]["hooks"], json!([foreign, fork, other_curl]));
        assert_eq!(s["hooks"]["Stop"].as_array().unwrap().len(), 2, "legacy hook replaced, not duplicated");
        assert!(token_path(&h).exists() && hook_target(&h).exists());
        let st = status(&h);
        assert_eq!(st["complete"], true);
        assert_eq!(st["mode"], "node");
        assert_eq!(st["port"], 5000);
        uninstall(&h).unwrap();
        assert_eq!(read(&file)["hooks"], json!({ "Stop": [{ "hooks": [foreign, fork, other_curl] }] }));
        fs::write(&file, "{broken").unwrap();
        assert!(install(5000, None, &h).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "{broken", "unreadable settings are never overwritten");
        fs::write(&file, json!({ "hooks": { "Stop": { "not": "a list" }, "PreToolUse": [null, { "hooks": [null] }] } }).to_string()).unwrap();
        let _ = status(&h);
        assert!(install(5000, None, &h).unwrap_err().contains("not a list"));
        // backups: only the newest three of ours stay
        let n = fs::read_dir(claude_dir(&h)).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("settings.json.bak-bobix-")).count();
        assert!(n <= 3);
        let _ = fs::remove_dir_all(&h);
    }

    #[cfg(unix)]
    #[test]
    fn writes_through_a_symlink_and_keeps_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let h = home("link");
        let dot = h.join("dotfiles/claude-settings.json");
        fs::create_dir_all(dot.parent().unwrap()).unwrap();
        fs::write(&dot, r#"{"env":{"SECRET":"x"}}"#).unwrap();
        fs::set_permissions(&dot, fs::Permissions::from_mode(0o600)).unwrap();
        std::os::unix::fs::symlink(&dot, settings_path(&h)).unwrap();
        install(5000, None, &h).unwrap();
        assert!(fs::symlink_metadata(settings_path(&h)).unwrap().file_type().is_symlink());
        assert_eq!(fs::metadata(&dot).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(read(&dot)["hooks"]["Stop"].is_array());
        let _ = fs::remove_dir_all(&h);
    }

    #[test]
    fn foreground_hooks_and_outdated_rules() {
        let h = home("fg");
        install(4747, Some("/usr/bin/node"), &h).unwrap();
        let file = settings_path(&h);
        let mut cfg = read(&file);
        let (ask, tool, start) = (&cfg["hooks"]["PermissionRequest"][0]["hooks"][0], &cfg["hooks"]["PreToolUse"][0]["hooks"][0], &cfg["hooks"]["SessionStart"][0]["hooks"][0]);
        assert!(ask.get("async").is_none());
        assert_eq!(ask["timeout"], 86400);
        assert_eq!(tool["async"], true);
        assert!(start.get("async").is_none());
        assert_eq!(start["timeout"], 5);
        assert_eq!(status(&h)["outdated"], false);
        cfg["hooks"]["SessionStart"][0]["hooks"][0]["async"] = json!(true);
        fs::write(&file, cfg.to_string()).unwrap();
        assert_eq!(status(&h)["outdated"], true, "installed before session titles");
        cfg["hooks"]["SessionStart"][0]["hooks"][0].as_object_mut().unwrap().remove("async");
        cfg["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"] = json!(330);
        fs::write(&file, cfg.to_string()).unwrap();
        assert_eq!(status(&h)["outdated"], true, "a cap shorter than a day");
        let _ = fs::remove_dir_all(&h);
    }

    #[test]
    fn session_guild_upgrade() {
        let h = home("guild");
        let file = settings_path(&h);
        let old_dir = claude_dir(&h).join("session-guild");
        fs::create_dir_all(&old_dir).unwrap();
        fs::write(old_dir.join("token"), "old").unwrap();
        let foreign = json!({ "type": "command", "command": "echo hi" });
        let old = json!({ "type": "command", "command": "/usr/bin/node", "args": [old_dir.join("hook.js"), "4747"], "async": true });
        fs::write(&file, json!({ "hooks": { "Stop": [{ "hooks": [foreign] }, { "hooks": [old] }], "PreToolUse": [{ "matcher": "*", "hooks": [old] }] } }).to_string()).unwrap();
        assert_eq!(status(&h)["outdated"], true);
        install(4747, Some("/usr/bin/node"), &h).unwrap();
        let s = read(&file);
        assert!(!s["hooks"].to_string().contains("session-guild"));
        assert_eq!(s["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
        assert_eq!(s["hooks"]["Stop"][0]["hooks"], json!([foreign]));
        assert_eq!(status(&h)["outdated"], false);
        assert!(!old_dir.exists());
        let _ = fs::remove_dir_all(&h);
    }

    #[test]
    fn curl_and_powershell_entries() {
        let h = PathBuf::from("/home/u");
        let c = hook_entry(4747, None, false, &h, "Stop");
        assert_eq!(c["command"], "curl");
        assert!(is_ours(&c));
        let p = hook_entry(4747, None, true, &h, "Stop");
        assert_eq!(p["command"], "powershell");
        assert!(p["args"][2].as_str().unwrap().ends_with("# bobix"));
        assert!(is_ours(&p));
        assert_eq!(iso_stamp(1_762_511_122_918), "2025-11-07T10-25-22-918Z");
    }
}
