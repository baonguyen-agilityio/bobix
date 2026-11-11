// What Bobix remembers of your work with Claude, while Settings → "Bobix nhớ gì về bạn" is on (off by default): activity
// only, one JSON line per event in a file per day (UTC) in Bobix's own data folder, the last 30 days. Never a prompt,
// chat text, code, file contents or Claude's memory; nothing is written to Claude's files or sent anywhere. The panel
// turns it into suggestions (src/ui/habits.js) with simple local rules.
use crate::state::{Session, Sessions};
use serde_json::{json, Value};
use std::{collections::HashMap, fs, io::Write, path::{Path, PathBuf}};

pub const KEEP_DAYS: u64 = 30;
const DAY_MS: u64 = 86_400_000;

pub fn on(settings: &Value) -> bool { settings["habitsOn"] == true }
pub fn dir() -> PathBuf { crate::paths::data_dir().join("habits") }

// A session as it was before an event: to tell a state change and how long a permission request waited.
pub struct Before { state: String, since: u64, dir: String, branch: Option<String>, issue: Option<i64> }

pub fn before(sessions: &Sessions, e: &Value) -> Option<Before> { sessions.get(e["session_id"].as_str()?).map(before_of) }
fn before_of(s: &Session) -> Before { Before { state: s.state.clone(), since: s.since, dir: dir_of(s), branch: branch_of(s), issue: issue_of(s) } }

// A session that went away without a SessionEnd (its process gone, or stale).
pub fn ended(mem: &mut Mem, s: &Session, now: u64) -> Vec<Value> {
    events(mem, Some(&before_of(s)), &json!({ "session_id": s.id, "hook_event_name": "SessionEnd" }), None, now)
}

// The permission request a session waits on: (tool, the rule that would allow it), until it ends; and the tracker
// project of each session's repository, once known ("p" on its lines: the morning suggestion finds the issue by it).
#[derive(Default)]
pub struct Mem { asked: HashMap<String, (String, String)>, pub projects: HashMap<String, String> }

fn with_project(mem: &Mem, sid: &str, mut lines: Vec<Value>) -> Vec<Value> {
    if let Some(p) = mem.projects.get(sid) { for l in &mut lines { l["p"] = json!(p); } }
    lines
}

fn dir_of(s: &Session) -> String { s.project.clone().filter(|p| !p.is_empty()).unwrap_or_else(|| s.cwd.clone()) }
fn branch_of(s: &Session) -> Option<String> { s.git.as_ref().and_then(|g| g["branch"].as_str()).map(String::from) }
fn issue_of(s: &Session) -> Option<i64> {
    s.started.as_ref().and_then(|v| v["iid"].as_i64()).or_else(|| branch_of(s).as_deref().and_then(crate::model::issue_of_branch))
}

// The rule in Claude's own permission settings that would allow this request: for Bash, the command's first word and
// its sub-command ("Bash(npm test:*)"), never its arguments; for any other tool, the tool. None when there is nothing
// short and safe to say.
pub fn rule_of(tool: &str, input: &Value) -> Option<String> {
    if tool.is_empty() || !tool.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') { return None; }
    if tool != "Bash" { return Some(tool.to_string()); }
    let words: Vec<&str> = input["command"].as_str()?.split_whitespace().collect();
    let plain = |w: &str| !w.is_empty() && w.len() <= 30 && w.chars().all(|c| c.is_ascii_alphanumeric() || "-_:".contains(c));
    let first = words.first()?.rsplit('/').next()?;
    if !plain(first) || first.contains('=') { return None; }
    let sub = words.get(1).filter(|w| plain(w) && !w.starts_with('-'));
    Some(match sub { Some(w) => format!("Bash({first} {w}:*)"), None => format!("Bash({first}:*)") })
}

// Claude Code's own commands: a prompt starting with one of them used no skill.
const BUILTIN: &[&str] = &["add-dir", "agents", "bug", "clear", "compact", "config", "context", "cost", "doctor", "exit", "export", "help", "hooks", "init",
    "login", "logout", "mcp", "memory", "model", "permissions", "release-notes", "rename", "resume", "status", "terminal-setup", "theme", "upgrade", "usage", "vim"];

// The skill a prompt starts with ("/fix …"), or the one the Skill tool runs. Only its name; never one of Claude Code's
// own commands (/clear, /compact, /model…).
pub fn skill_of(e: &Value) -> Option<String> {
    let name = match e["hook_event_name"].as_str()? {
        "UserPromptSubmit" => e["prompt"].as_str()?.trim_start().strip_prefix('/')?.split_whitespace().next()?.to_string(),
        "PreToolUse" if e["tool_name"] == "Skill" => e["tool_input"]["skill"].as_str().or(e["tool_input"]["command"].as_str())?.trim_start_matches('/').to_string(),
        _ => return None,
    };
    (!name.is_empty() && name.len() <= 60 && name.chars().all(|c| c.is_ascii_alphanumeric() || "-_:.".contains(c)) && !BUILTIN.contains(&name.as_str())).then_some(name)
}

fn line(k: &str, sid: &str, now: u64, dir: &str, branch: &Option<String>, issue: Option<i64>, more: Value) -> Value {
    let mut v = json!({ "t": now, "k": k, "s": sid, "d": dir });
    if let Some(b) = branch { v["b"] = json!(b); }
    if let Some(i) = issue { v["i"] = json!(i); }
    if let Value::Object(m) = more { for (k, x) in m { v[k] = x; } }
    v
}

// The lines one hook event adds. `s`: the session after it (None once it ended).
pub fn events(mem: &mut Mem, before: Option<&Before>, e: &Value, s: Option<&Session>, now: u64) -> Vec<Value> {
    let Some(sid) = e["session_id"].as_str() else { return vec![] };
    let ev = e["hook_event_name"].as_str().unwrap_or("");
    let (dir, branch, issue) = match s { Some(s) => (dir_of(s), branch_of(s), issue_of(s)), None => match before {
        Some(b) => (b.dir.clone(), b.branch.clone(), b.issue), None => return vec![] } };
    let mut out = vec![];
    if ev == "SessionStart" { out.push(line("start", sid, now, &dir, &branch, issue, json!({}))); }
    let (was, now_state) = (before.map(|b| b.state.as_str()), s.map(|s| s.state.as_str()));
    if was == Some("waiting") && now_state != Some("waiting") {
        out.extend(wait_end(mem, sid, before.unwrap().since, now, &dir, &branch, issue, ev == "PostToolUse"));
    }
    if ev == "PermissionRequest" {
        if let Some(r) = rule_of(e["tool_name"].as_str().unwrap_or(""), &e["tool_input"]) { mem.asked.insert(sid.into(), (e["tool_name"].as_str().unwrap_or("").into(), r)); }
    }
    if let Some(st) = now_state.filter(|st| Some(*st) != was) { out.push(line("st", sid, now, &dir, &branch, issue, json!({ "v": st }))); }
    if let Some(k) = skill_of(e) { out.push(line("skill", sid, now, &dir, &branch, issue, json!({ "v": k }))); }
    if ev == "SessionEnd" { mem.asked.remove(sid); out.push(line("end", sid, now, &dir, &branch, issue, json!({}))); }
    let out = with_project(mem, sid, out);
    if ev == "SessionEnd" { mem.projects.remove(sid); }
    out
}

// A permission request stopped waiting: how long, and whether the tool then ran (ok: you allowed it).
#[allow(clippy::too_many_arguments)]
fn wait_end(mem: &mut Mem, sid: &str, since: u64, now: u64, dir: &str, branch: &Option<String>, issue: Option<i64>, ok: bool) -> Option<Value> {
    let (tool, rule) = mem.asked.remove(sid).unwrap_or_default();
    Some(line("ask", sid, now, dir, branch, issue, json!({ "ms": now.saturating_sub(since), "tool": tool, "r": rule, "ok": ok })))
}

// A turn cut short while it waited (Esc, a denied request): Claude runs no hook, the 5 s check notices.
pub fn interrupted(mem: &mut Mem, s: &Session, since: u64, now: u64) -> Vec<Value> {
    let (dir, branch, issue) = (dir_of(s), branch_of(s), issue_of(s));
    let mut out: Vec<Value> = wait_end(mem, &s.id, since, now, &dir, &branch, issue, false).into_iter().collect();
    out.push(line("st", &s.id, now, &dir, &branch, issue, json!({ "v": s.state })));
    with_project(mem, &s.id, out)
}

// Something you did in the panel that habits look for (the stand-up, the report).
pub fn ui(what: &str, now: u64) -> Option<Value> {
    ["standup", "report"].contains(&what).then(|| json!({ "t": now, "k": "ui", "v": what }))
}

// ---------- the files ----------

// "YYYY-MM-DD" of a time (ms since 1970, UTC).
pub fn day_of(ms: u64) -> String {
    let z = (ms / DAY_MS) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", yoe + era * 400 + i64::from(m <= 2), m, d)
}
fn is_day_file(name: &str) -> bool {
    name.len() == 16 && name.ends_with(".jsonl") && name[..10].bytes().enumerate().all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() })
}
fn files(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = fs::read_dir(dir).map(|r| r.flatten().filter_map(|e| {
        let n = e.file_name().to_string_lossy().into_owned();
        is_day_file(&n).then(|| (n[..10].to_string(), e.path()))
    }).collect()).unwrap_or_default();
    out.sort();
    out
}

// Adds the lines to today's file, and forgets the days older than KEEP_DAYS.
pub fn write(dir: &Path, lines: &[Value], now: u64) -> std::io::Result<()> {
    if lines.is_empty() { return Ok(()); }
    fs::create_dir_all(dir)?;
    let mut f = fs::OpenOptions::new().create(true).append(true).open(dir.join(format!("{}.jsonl", day_of(now))))?;
    let text: String = lines.iter().map(|l| l.to_string() + "\n").collect();
    f.write_all(text.as_bytes())?;
    prune(dir, now);
    Ok(())
}
pub fn prune(dir: &Path, now: u64) {
    let oldest = day_of(now.saturating_sub((KEEP_DAYS - 1) * DAY_MS));
    for (d, p) in files(dir) { if d < oldest { let _ = fs::remove_file(p); } }
}

// Everything kept, for the panel: the events (oldest first), how many days, how big, where.
pub fn read(dir: &Path, now: u64) -> Value {
    prune(dir, now);
    let (mut events, mut bytes, mut days) = (vec![], 0u64, 0);
    for (_, p) in files(dir) {
        let Ok(text) = fs::read_to_string(&p) else { continue };
        bytes += text.len() as u64;
        days += 1;
        events.extend(text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()));
    }
    json!({ "events": events, "bytes": bytes, "days": days, "dir": dir.to_string_lossy() })
}

// "Xoá hết": every day file, then the folder when nothing else is in it.
pub fn clear(dir: &Path) -> usize {
    let n = files(dir).into_iter().filter(|(_, p)| fs::remove_file(p).is_ok()).count();
    let _ = fs::remove_dir(dir); // only when empty
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply, Applied};

    fn tmp(name: &str) -> PathBuf { std::env::temp_dir().join(format!("bobix-habits-{name}-{}", std::process::id())) }

    #[test]
    fn rules_name_the_command_never_its_arguments() {
        let r = |c: &str| rule_of("Bash", &json!({ "command": c }));
        assert_eq!(r("npm test -- --watch=false").as_deref(), Some("Bash(npm test:*)"));
        assert_eq!(r("cargo test --release").as_deref(), Some("Bash(cargo test:*)"));
        assert_eq!(r("/usr/local/bin/git status").as_deref(), Some("Bash(git status:*)"));
        assert_eq!(r("ls /Users/me/secret").as_deref(), Some("Bash(ls:*)"), "a path is an argument");
        assert_eq!(r("curl https://x.example/?token=abc").as_deref(), Some("Bash(curl:*)"));
        assert_eq!(r("TOKEN=abc npm publish"), None, "an assignment first: nothing");
        assert_eq!(r(""), None);
        assert_eq!(rule_of("WebFetch", &json!({ "url": "https://docs.example.com/x" })).as_deref(), Some("WebFetch"));
        assert_eq!(rule_of("mcp__x; rm", &json!({})), None);
        assert_eq!(skill_of(&json!({ "hook_event_name": "UserPromptSubmit", "prompt": "/fix the login bug please" })).as_deref(), Some("fix"));
        assert_eq!(skill_of(&json!({ "hook_event_name": "UserPromptSubmit", "prompt": "fix it /now" })), None);
        for c in ["/clear", "/compact", "/help", "/model sonnet", "/resume", "/config"] {
            assert_eq!(skill_of(&json!({ "hook_event_name": "UserPromptSubmit", "prompt": c })), None, "{c}: Claude Code's own command, not a skill");
        }
        assert_eq!(skill_of(&json!({ "hook_event_name": "PreToolUse", "tool_name": "Skill", "tool_input": { "skill": "acme:build" } })).as_deref(), Some("acme:build"));
    }

    #[test]
    fn a_session_leaves_activity_only() {
        let mut m = Sessions::new();
        let mut mem = Mem::default();
        let mut log = vec![];
        mem.projects.insert("s1".into(), "team/shop".into());
        let mut ev = |m: &mut Sessions, e: Value, t: u64| {
            let b = before(m, &e);
            let r = apply(m, &e, t);
            let s = match &r { Some(Applied::Updated { id, .. }) => m.get_mut(id).map(|s| { s.project = Some("/src/shop".into()); s.git = Some(json!({ "branch": "128-cart" })); &*s }), _ => None };
            log.extend(events(&mut mem, b.as_ref(), &e, s, t));
        };
        let e = |ev: &str, more: Value| { let mut v = json!({ "session_id": "s1", "cwd": "/src/shop/app", "hook_event_name": ev }); for (k, x) in more.as_object().unwrap() { v[k] = x.clone(); } v };
        ev(&mut m, e("SessionStart", json!({ "source": "startup" })), 1_000);
        ev(&mut m, e("UserPromptSubmit", json!({ "prompt": "/fix the cart total, secret plan inside" })), 2_000);
        ev(&mut m, e("PermissionRequest", json!({ "tool_name": "Bash", "tool_input": { "command": "npm test -- cart.spec.js" } })), 3_000);
        ev(&mut m, e("PostToolUse", json!({ "tool_name": "Bash", "tool_response": { "stdout": "code output" } })), 63_000);
        ev(&mut m, e("PermissionRequest", json!({ "tool_name": "Bash", "tool_input": { "command": "rm -rf build" } })), 70_000);
        ev(&mut m, e("Stop", json!({})), 80_000);
        ev(&mut m, e("SessionEnd", json!({})), 90_000);
        let kinds: Vec<&str> = log.iter().map(|l| l["k"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["start", "st", "st", "skill", "st", "ask", "st", "st", "ask", "st", "end"]);
        let asks: Vec<(u64, &str, bool)> = log.iter().filter(|l| l["k"] == "ask").map(|l| (l["ms"].as_u64().unwrap(), l["r"].as_str().unwrap(), l["ok"] == true)).collect();
        assert_eq!(asks, [(60_000, "Bash(npm test:*)", true), (10_000, "Bash(rm:*)", false)], "how long, what, allowed or not");
        assert!(log.iter().all(|l| l["d"] == "/src/shop" && l["p"] == "team/shop" && l["i"] == 128 && l["b"] == "128-cart"));
        assert!(mem.projects.is_empty() && mem.asked.is_empty(), "nothing kept for a session that ended");
        let text = serde_json::to_string(&log).unwrap();
        for never in ["secret", "cart.spec", "code output", "the cart total", "-rf build"] { assert!(!text.contains(never), "{never} kept"); }
    }

    #[test]
    fn days_are_kept_30_days_and_cleared_on_request() {
        let dir = tmp("files");
        let _ = fs::remove_dir_all(&dir);
        let day = DAY_MS;
        let now = 1_762_650_000_000; // 2025-11-09
        assert_eq!(day_of(now), "2025-11-09");
        assert_eq!((day_of(0), day_of(951_782_400_000)), ("1970-01-01".into(), "2000-02-29".into()));
        for back in [40, 31, 30, 29, 1, 0] { write(&dir, &[json!({ "t": now - back * day, "k": "st" })], now - back * day).unwrap(); }
        fs::write(dir.join("notes.txt"), "not ours").unwrap();
        let r = read(&dir, now);
        let ts: Vec<u64> = r["events"].as_array().unwrap().iter().map(|e| (now - e["t"].as_u64().unwrap()) / day).collect();
        assert_eq!(ts, [29, 1, 0], "the last 30 days (today and 29 before)");
        assert_eq!(r["days"], 3);
        assert!(r["bytes"].as_u64().unwrap() > 0);
        assert_eq!(clear(&dir), 3);
        assert!(dir.join("notes.txt").exists(), "only Bobix's day files go");
        assert_eq!(read(&dir, now)["events"], json!([]));
        fs::remove_file(dir.join("notes.txt")).unwrap();
        assert_eq!(clear(&dir), 0);
        assert!(!dir.exists(), "an empty folder goes too");
        assert!(ui("standup", 5).is_some() && ui("rm -rf", 5).is_none());
    }
}
