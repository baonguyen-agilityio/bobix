// Work items: a session's local git status (no network), and the GitLab issues assigned to you, read through your own
// `glab` (Bobix keeps no token, writes nothing to GitLab). Commands get argument lists, never a shell.
// Port of the 0.1 (Electron) app's core/work.js, plus the Work tab's state from src/main.js (workView, takeIssue, startIssue...).
use crate::{js::{head16, len16}, links::ClaudeSession, procs, state::Sessions};
use fancy_regex::Regex;
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::LazyLock};

fn re(s: &str) -> Regex { Regex::new(s).unwrap() }

// encodeURIComponent
const COMPONENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.').remove(b'!').remove(b'~').remove(b'*').remove(b'\'').remove(b'(').remove(b')');
pub fn enc(s: &str) -> String { utf8_percent_encode(s, COMPONENT).to_string() }

// `git status --porcelain=v2 --branch` → branch, upstream, ahead / behind, changed files (staged, unstaged, untracked, conflicts).
pub fn parse_status(text: &str) -> Value {
    let (mut branch, mut upstream, mut ahead, mut behind, mut changed) = (Value::Null, Value::Null, 0u64, 0u64, 0u64);
    static AB: LazyLock<Regex> = LazyLock::new(|| re(r"\+(\d+) -(\d+)"));
    for l in text.split('\n') {
        if let Some(b) = l.strip_prefix("# branch.head ") { branch = if b == "(detached)" { Value::Null } else { json!(b) }; }
        else if let Some(u) = l.strip_prefix("# branch.upstream ") { upstream = json!(u); }
        else if let Some(ab) = l.strip_prefix("# branch.ab ") {
            if let Ok(Some(m)) = AB.captures(ab) { ahead = m[1].parse().unwrap_or(0); behind = m[2].parse().unwrap_or(0); }
        } else if l.len() > 1 && b"12u?".contains(&l.as_bytes()[0]) && l.as_bytes()[1] == b' ' { changed += 1; }
    }
    json!({ "branch": branch, "upstream": upstream, "ahead": ahead, "behind": behind, "changed": changed })
}

fn git(dir: &str, args: &[&str]) -> procs::Ran {
    let mut a = vec!["-C", dir];
    a.extend_from_slice(args);
    procs::exec("git", &a, 15, &[("GIT_OPTIONAL_LOCKS", "0"), ("LC_ALL", "C")]) // never takes the index lock from a running git
}

// The git status of the repository at `dir` and its remote URLs (kept in the app: they may hold credentials), or None
// outside git.
pub fn git_status(dir: &str) -> Option<(Value, Vec<String>)> {
    let st = git(dir, &["status", "--porcelain=v2", "--branch"]);
    if !st.ok { return None; }
    let top = git(dir, &["rev-parse", "--show-toplevel"]);
    let log = git(dir, &["log", "-1", "--format=%ct%x00%s"]);
    let unpushed = git(dir, &["rev-list", "--count", "HEAD", "--not", "--remotes"]); // commits on no remote at all, upstream or not
    let wt = git(dir, &["worktree", "list", "--porcelain"]);
    let remotes = git(dir, &["remote", "-v"]);
    let mut out = parse_status(&st.out);
    let log_out = log.out.trim();
    let (t, subject) = log_out.split_once('\0').unwrap_or((log_out, ""));
    let top = top.out.trim();
    out["top"] = json!(if top.is_empty() { dir } else { top });
    out["unpushed"] = json!(if unpushed.ok { unpushed.out.trim().parse::<u64>().unwrap_or(0) } else { 0 });
    out["last"] = match t.parse::<u64>() { Ok(t) => json!({ "t": t * 1000, "subject": head16(subject, 200) }), _ => Value::Null };
    out["worktrees"] = json!(wt.out.lines().filter(|l| l.starts_with("worktree ")).count());
    let mut urls: Vec<String> = Vec::new();
    for u in remotes.out.lines().filter_map(|l| l.split_whitespace().nth(1)) { if !urls.iter().any(|x| x == u) { urls.push(u.into()); } }
    Some((out, urls))
}

// The GitLab hosts glab is signed in to, from `glab auth status` (it names each host it can reach with "Logged in to
// <host> as …"; a host whose sign-in fails has no such line). glab masks the token there; only these lines are read.
pub fn logged_in_hosts(text: &str) -> Vec<String> {
    static LOGGED: LazyLock<Regex> = LazyLock::new(|| re(r"Logged in to (\S+) as "));
    let mut out: Vec<String> = Vec::new();
    for m in LOGGED.captures_iter(text).flatten() { let h = m[1].to_lowercase(); if !out.contains(&h) { out.push(h); } }
    out
}
// None when glab is missing, else the hosts (none when it is signed in nowhere). glab exits non-zero when any host fails.
pub fn glab_hosts(glab: &str) -> Option<Vec<String>> {
    let r = procs::exec(glab, &["auth", "status"], 30, &[]);
    if r.missing { return None; }
    Some(logged_in_hosts(&format!("{}\n{}", r.out, r.err)))
}

// A git remote URL → GitLab project path ("group/sub/project", lower case) when it points at `host`, else None.
// Forms: git@host:g/p.git, ssh://git@host[:port]/g/p.git, https://[user@]host[:port]/g/p(.git).
pub fn project_path(remote: &str, host: &str) -> Option<String> {
    static SCP: LazyLock<Regex> = LazyLock::new(|| re(r"^(?:[A-Za-z0-9_.-]+@)?([^:/\s]+):(?!/)(.+)$"));
    static SCHEME: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^[a-z][A-Za-z0-9_+.-]*://"));
    static PORT: LazyLock<Regex> = LazyLock::new(|| re(r":\d+$"));
    if host.is_empty() { return None; }
    let h = PORT.replace(&host.to_lowercase(), "").into_owned();
    let (rhost, p) = match SCP.captures(remote).ok().flatten() {
        Some(m) if !SCHEME.is_match(remote).unwrap_or(false) => (m[1].to_string(), m[2].to_string()),
        _ => {
            let u = tauri::Url::parse(remote).ok()?;
            if !["http", "https", "ssh", "git+ssh"].contains(&u.scheme()) { return None; }
            (u.host_str()?.to_string(), percent_decode_str(u.path()).decode_utf8().ok()?.into_owned())
        }
    };
    if rhost.to_lowercase() != h { return None; }
    let p = p.trim_matches('/');
    let p = if p.len() >= 4 && p[p.len() - 4..].eq_ignore_ascii_case(".git") { &p[..p.len() - 4] } else { p };
    p.contains('/').then(|| p.to_lowercase())
}

// GitLab's "create branch" from an issue names it "<iid>-<title>", also under a prefix ("feat/<iid>-…").
pub fn issue_of_branch(branch: &str) -> Option<i64> {
    static B: LazyLock<Regex> = LazyLock::new(|| re(r"(?:^|/)(\d+)-"));
    B.captures(branch).ok().flatten().and_then(|m| m[1].parse().ok())
}

// A project's short code, from its name (the last part of its path): the first two letters of the first word and the
// first letter of the last word ("ba-qa-kit" → BAK); one word: its first three letters ("bobix" → BOB).
pub fn key_for(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or("").to_uppercase();
    let w: Vec<&str> = name.split(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit())).filter(|x| !x.is_empty()).collect();
    match w.as_slice() {
        [] => "ISSUE".into(),
        [one] => one.chars().take(3).collect(),
        [first, .., last] => format!("{}{}", first.chars().take(2).collect::<String>(), &last[..1]),
    }
}

// The name of a session started from an issue: "[<KEY>-<iid>] <short title>". Leading tags such as "[Bug]" go; the
// title is cut at a word near 40 characters.
pub fn session_title_for(key: &str, iid: i64, title: &str) -> String {
    static TAGS: LazyLock<Regex> = LazyLock::new(|| re(r"^(\s*\[[^\]]*\])+"));
    static WS: LazyLock<Regex> = LazyLock::new(|| re(r"\s+"));
    static LAST: LazyLock<Regex> = LazyLock::new(|| re(r"\s+\S*$"));
    static TRAIL: LazyLock<Regex> = LazyLock::new(|| re(r"[\s,.;:-]+$"));
    let d = TAGS.replace(title, "");
    let mut d = WS.replace_all(&d, " ").trim().to_string();
    if len16(&d) > 40 {
        let cut = LAST.replace(head16(&d, 41), "").into_owned();
        d = TRAIL.replace(&cut, "").into_owned() + "…";
    }
    if d.is_empty() { format!("[{key}-{iid}]") } else { format!("[{key}-{iid}] {d}") }
}

// Is `name` a session name of that issue? "[KEY-iid]" or "<project name> #<iid>", alone or followed by a space.
pub fn names_issue(name: &str, key: &str, path: &str, iid: i64) -> bool {
    [format!("[{key}-{iid}]"), format!("{} #{iid}", path.rsplit('/').next().unwrap_or(""))].iter()
        .any(|p| name == p || name.starts_with(&format!("{p} ")))
}

// The issue a session works on: the session's own start (New on that issue), else the newest of its branches that
// names one, else its parent's (a side session on a branch Claude names).
pub fn issue_of_session(path: Option<&str>, started: Option<&Value>, branches: &[String], parent_iid: Option<i64>) -> Option<i64> {
    let path = path?;
    if let Some(s) = started.filter(|s| s["path"] == path) { return s["iid"].as_i64(); }
    branches.iter().rev().find_map(|b| issue_of_branch(b)).or(parent_iid)
}

// The fields Bobix shows of one issue from GitLab's API.
pub fn slim_issue(i: &Value) -> Value {
    static REF: LazyLock<Regex> = LazyLock::new(|| re(r"#\d+$"));
    let labels: Vec<Value> = i["labels"].as_array().map(|a| a.iter().take(10).map(|l| json!(l.as_str().map(String::from).unwrap_or_else(|| l.to_string()))).collect()).unwrap_or_default();
    let due = i["due_date"].as_str().filter(|d| !d.is_empty()).map(|d| json!(d)).unwrap_or(Value::Null);
    json!({ "iid": i["iid"], "title": head16(i["title"].as_str().unwrap_or(""), 300), "url": i["web_url"],
        "project": REF.replace(i["references"]["full"].as_str().unwrap_or(""), "").to_lowercase(), "labels": labels, "due": due })
}

// One session as the Work tab sees it: { id, path (GitLab project), dir, iid }.
pub struct Live { pub id: String, pub path: Option<String>, pub dir: String, pub iid: Option<i64> }

// Issues by project, local repositories first (a session of theirs runs here), the rest "not on this machine". Each
// issue gets `sessions`: the ids working on it. In progress first, then by due date, then newest.
pub fn group_issues(issues: &[Value], live: &[Live]) -> (Vec<Value>, Vec<Value>) {
    let dir_of: HashMap<&str, &str> = live.iter().filter_map(|r| Some((r.path.as_deref()?, r.dir.as_str()))).collect();
    let mut groups: indexmap::IndexMap<String, Vec<Value>> = Default::default();
    for i in issues {
        let p = i["project"].as_str().unwrap_or("").to_string();
        let working: Vec<&str> = live.iter().filter(|s| s.path.as_deref() == Some(&p) && s.iid.is_some() && s.iid == i["iid"].as_i64()).map(|s| s.id.as_str()).collect();
        let mut x = i.clone();
        x["sessions"] = json!(working);
        groups.entry(p).or_default().push(x);
    }
    groups.sort_keys(); // ponytail: byte order, not localeCompare; same for lower-case GitLab paths
    let (mut local, mut away) = (vec![], vec![]);
    for (path, mut list) in groups {
        let due = |v: &Value| v["due"].as_str().unwrap_or("9").to_string();
        list.sort_by(|a, b| {
            let n = |v: &Value| v["sessions"].as_array().map_or(0, |a| a.len());
            n(b).cmp(&n(a)).then_with(|| due(a).cmp(&due(b))).then_with(|| b["iid"].as_i64().cmp(&a["iid"].as_i64()))
        });
        match dir_of.get(path.as_str()) {
            Some(d) => local.push(json!({ "path": path, "dir": d, "issues": list })),
            None => away.push(json!({ "path": path, "issues": list })),
        }
    }
    (local, away)
}

// Open issues assigned to you on `host`, through `glab api` (GET). { ok, issues } or { ok: false, reason, detail }.
// ponytail: first 100 only (one page), add paging when someone has more open issues than that.
pub fn fetch_issues(glab: &str, host: &str) -> Value {
    static AUTH: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b401\b|unauthori[sz]ed|not logged|no token|authenticat"));
    static WS: LazyLock<Regex> = LazyLock::new(|| re(r"\s+"));
    let r = procs::exec(glab, &["api", "--hostname", host, "issues?scope=assigned_to_me&state=opened&per_page=100"], 30, &[]);
    if r.missing { return json!({ "ok": false, "reason": "noglab" }); }
    if !r.ok {
        let text = format!("{} {}", r.err, r.out);
        if AUTH.is_match(&text).unwrap_or(false) { return json!({ "ok": false, "reason": "auth" }); }
        return json!({ "ok": false, "reason": "error", "detail": head16(WS.replace_all(&text, " ").trim(), 200) });
    }
    match serde_json::from_str::<Value>(&r.out) {
        Ok(Value::Array(a)) => json!({ "ok": true, "issues": a.iter().map(slim_issue).collect::<Vec<_>>() }),
        _ => json!({ "ok": false, "reason": "error", "detail": "unexpected answer from GitLab" }),
    }
}

// ---------- the Work tab's state (the 0.1 Electron app's main.js) ----------
#[derive(Default)]
pub struct GitCache { pub at: u64, pub busy: bool, pub git: Option<Value> }

pub struct Work {
    pub git_cache: HashMap<String, GitCache>, // folder -> its last status
    pub remotes_of: HashMap<String, Vec<String>>, // repository top folder -> remote URLs (never sent to the pages)
    pub glab_path: Option<String>,
    pub issues: Value, // { status: off | loading | noglab | auth | error | ok, hosts, at, failed, list }
    pub issues_busy: bool,
    pub claude_list: Vec<ClaudeSession>, // Claude desktop's own sessions, to find an issue's session by its name
    // New on an issue opens a new session in the project's folder and waits here: the first session created in exactly
    // that folder afterwards takes the issue and its name (see take_issue).
    pub new_for: indexmap::IndexMap<String, Value>, // project folder -> { path, iid, title }
}

impl Default for Work {
    fn default() -> Self {
        Work { git_cache: HashMap::new(), remotes_of: HashMap::new(), glab_path: None, issues: json!({ "status": "off" }), issues_busy: false, claude_list: vec![], new_for: Default::default() }
    }
}

// A project's code in session names: the one set in Settings, else the one made from its name.
pub fn key_of(settings: &Value, p: &str) -> String {
    settings["workKeys"][p].as_str().map(String::from).unwrap_or_else(|| key_for(p))
}

impl Work {
    pub fn hosts(&self, settings: &Value) -> Vec<String> {
        if settings["gitlab"] != true { return vec![]; }
        self.issues["hosts"].as_array().map(|a| a.iter().filter_map(|h| h.as_str().map(String::from)).collect()).unwrap_or_default()
    }

    // A repository's GitLab project path: the first signed-in host one of its remotes points at.
    // ponytail: the same path on two GitLabs counts as one project; key by host too if anyone works like that.
    pub fn path_in(remotes: &[String], hosts: &[String]) -> Option<String> {
        hosts.iter().find_map(|h| remotes.iter().find_map(|u| project_path(u, h)))
    }

    fn list(&self) -> &[Value] { self.issues["list"].as_array().map(Vec::as_slice).unwrap_or(&[]) }

    pub fn find_issue(&self, path: &str, iid: i64) -> Option<&Value> {
        self.list().iter().find(|i| i["project"] == path && i["iid"].as_i64() == Some(iid))
    }

    pub fn open_issue_ok(&self, settings: &Value, url: &str) -> bool {
        settings["gitlab"] == true && self.list().iter().any(|i| i["url"] == url)
    }

    // What the pages see: issues grouped by project, and for each session the issue it works on.
    pub fn view(&self, settings: &Value, sessions: &Sessions) -> (Value, HashMap<String, Value>) {
        let mut issue_of = HashMap::new();
        if settings["gitlab"] != true { return (json!({ "status": "off" }), issue_of); }
        let hosts = self.hosts(settings);
        let path_of = |top: &Value| Self::path_in(self.remotes_of.get(top.as_str().unwrap_or("")).map(Vec::as_slice).unwrap_or(&[]), &hosts);
        let branches = |s: &crate::state::Session| -> Vec<String> {
            s.branches.iter().cloned().chain(s.git.as_ref().and_then(|g| g["branch"].as_str()).map(String::from)).collect()
        };
        let by_host: HashMap<&str, &crate::state::Session> = sessions.values().filter(|s| s.git.is_some()).filter_map(|s| Some((s.host_session.as_deref()?, s))).collect();
        let live: Vec<Live> = sessions.values().filter_map(|s| {
            let g = s.git.as_ref()?;
            let path = path_of(&g["top"]);
            let parent_iid = s.parent_host.as_deref().and_then(|h| by_host.get(h)).filter(|p| path.is_some() && path_of(&p.git.as_ref().unwrap()["top"]) == path)
                .and_then(|p| issue_of_session(path.as_deref(), p.started.as_ref(), &branches(p), None)); // a side session works on its parent's issue
            let iid = issue_of_session(path.as_deref(), s.started.as_ref(), &branches(s), parent_iid);
            let dir = s.project.clone().filter(|p| !p.is_empty()).unwrap_or_else(|| g["top"].as_str().unwrap_or("").to_string());
            Some(Live { id: s.id.clone(), path, dir, iid })
        }).collect();
        for s in &live {
            if let (Some(iid), Some(p)) = (s.iid, s.path.as_deref()) {
                let i = self.find_issue(p, iid);
                issue_of.insert(s.id.clone(), json!({ "iid": iid, "title": i.map_or(json!(""), |i| i["title"].clone()), "url": i.map_or(json!(""), |i| i["url"].clone()) }));
            }
        }
        let (mut local, mut away) = group_issues(self.list(), &live);
        for g in &mut local {
            let (path, dir) = (g["path"].as_str().unwrap_or("").to_string(), g["dir"].as_str().unwrap_or("").to_string());
            let key = key_of(settings, &path);
            for i in g["issues"].as_array_mut().into_iter().flatten() {
                if i["sessions"].as_array().is_some_and(|a| a.is_empty()) {
                    let iid = i["iid"].as_i64().unwrap_or(-1);
                    i["resume"] = json!(crate::links::issue_session(&self.claude_list, &dir, |t| names_issue(t, &key, &path, iid)));
                }
            }
        }
        for g in local.iter_mut().chain(away.iter_mut()) {
            let p = g["path"].as_str().unwrap_or("").to_string();
            g["key"] = json!(key_of(settings, &p));
            g["autoKey"] = json!(key_for(&p));
        }
        let view = json!({ "status": self.issues["status"], "hosts": hosts, "at": self.issues.get("at").cloned().unwrap_or(Value::Null),
            "failed": self.issues.get("failed").cloned().unwrap_or(json!([])), "local": local, "away": away });
        (view, issue_of)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_urls_give_the_gitlab_project_only_on_its_host() {
        let h = "gitlab.example.com";
        assert_eq!(project_path("git@gitlab.example.com:team/shop-app.git", h).as_deref(), Some("team/shop-app"));
        assert_eq!(project_path("ssh://git@gitlab.example.com:2222/Group/Sub/Proj.git", h).as_deref(), Some("group/sub/proj"));
        assert_eq!(project_path("https://gitlab.example.com/team/shop-app", h).as_deref(), Some("team/shop-app"));
        assert_eq!(project_path("https://oauth2:secret@gitlab.example.com/g/p.git/", h).as_deref(), Some("g/p"));
        assert_eq!(project_path("https://gitlab.example.com:8443/g/p.git", "gitlab.example.com:8443").as_deref(), Some("g/p"));
        assert_eq!(project_path("git@github.com:baonguyen-agilityio/bobix.git", h), None);
        assert_eq!(project_path("https://gitlab.com/g/p.git", h), None);
        assert_eq!(project_path("/Users/me/repo.git", h), None);
        assert_eq!(project_path("git@gitlab.example.com:onlyname.git", h), None);
        assert_eq!(project_path("git@gitlab.example.com:g/p.git", ""), None);
        let status = "gitlab.com\n  x gitlab.com: API call failed: GET https://gitlab.com/api/v4/user: 401\n  ✓ Git operations for gitlab.com configured to use ssh protocol.\n\
            git.example.com:8443\n  ✓ Logged in to git.example.com:8443 as someone (keyring)\n  ✓ Token: **************************\n\
            Gitlab.Example.org\n  ✓ Logged in to Gitlab.Example.org as other (config.yml)\n";
        assert_eq!(logged_in_hosts(status), ["git.example.com:8443", "gitlab.example.org"]);
        assert!(logged_in_hosts("X could not authenticate").is_empty());
    }

    #[test]
    fn branches_and_sessions_link_to_issues() {
        assert_eq!(issue_of_branch("29-buy-item-list-cannot-be-used"), Some(29));
        assert_eq!(issue_of_branch("feat/29-x"), Some(29));
        assert_eq!(issue_of_branch("v29-x"), None);
        assert_eq!(issue_of_branch("main"), None);
        let b = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let s = |br: &[&str], parent: Option<i64>, path: Option<&str>| issue_of_session(path, None, &b(br), parent);
        assert_eq!(s(&["29-old", "main", "feat/31-new", "main"], None, Some("g/a")), Some(31));
        assert_eq!(s(&["main", "HEAD"], None, Some("g/a")), None);
        assert_eq!(s(&["claude/brave-otter"], Some(26), Some("g/a")), Some(26));
        assert_eq!(s(&["29-x"], Some(26), Some("g/a")), Some(29));
        assert_eq!(s(&["29-x"], None, None), None);
        assert_eq!(issue_of_session(Some("g/a"), Some(&json!({ "path": "g/a", "iid": 7 })), &b(&["29-x"]), None), Some(7), "started from an issue");
        let issues = [json!({ "iid": 29, "project": "g/a", "title": "x", "labels": [], "due": null, "url": "u" })];
        let live = |id: &str, p: &str, iid| Live { id: id.into(), path: Some(p.into()), dir: String::new(), iid: Some(iid) };
        let (local, away) = group_issues(&issues, &[]);
        assert!(local.is_empty() && away[0]["issues"][0]["sessions"] == json!([]));
        let (local, _) = group_issues(&issues, &[live("s1", "g/a", 29), live("s2", "g/b", 29), live("s3", "g/a", 290)]);
        assert_eq!(local[0]["issues"][0]["sessions"], json!(["s1"]), "same number in another project does not count");
    }

    #[test]
    fn issues_grouped_local_first() {
        let mk = |p: &str, iid: i64, due: Value| json!({ "iid": iid, "project": p, "title": format!("t{iid}"), "labels": [], "due": due, "url": format!("u{iid}") });
        let issues = [mk("g/away", 1, Value::Null), mk("g/here", 2, json!("2026-12-01")), mk("g/here", 3, json!("2026-11-10")), mk("g/here", 4, Value::Null)];
        let live = [Live { id: "s".into(), path: Some("g/here".into()), dir: "/src/here".into(), iid: Some(4) }, Live { id: "x".into(), path: None, dir: "/tmp/x".into(), iid: None }];
        let (local, away) = group_issues(&issues, &live);
        assert_eq!(local.len(), 1);
        assert_eq!((local[0]["path"].as_str(), local[0]["dir"].as_str()), (Some("g/here"), Some("/src/here")));
        assert_eq!(local[0]["issues"].as_array().unwrap().iter().map(|i| i["iid"].as_i64().unwrap()).collect::<Vec<_>>(), [4, 3, 2]);
        assert_eq!(away[0]["path"], "g/away");
        assert!(away[0].get("dir").is_none());
        assert_eq!(slim_issue(&json!({ "iid": 7, "title": "T", "web_url": "https://h/g/p/-/issues/7", "references": { "full": "G/P#7" }, "labels": ["Bug"], "due_date": "2026-11-20" })),
            json!({ "iid": 7, "title": "T", "url": "https://h/g/p/-/issues/7", "project": "g/p", "labels": ["Bug"], "due": "2026-11-20" }));
    }

    #[test]
    fn porcelain_v2_parsing() {
        let r = parse_status("# branch.oid abc\n# branch.head 29-fix\n# branch.upstream origin/29-fix\n# branch.ab +2 -1\n1 .M N... 100644 100644 100644 a b src/x.js\n? new.txt\nu UU N... 1 2 3 4 a b c f\n");
        assert_eq!(r, json!({ "branch": "29-fix", "upstream": "origin/29-fix", "ahead": 2, "behind": 1, "changed": 3 }));
        assert_eq!(parse_status("# branch.head (detached)\n")["branch"], Value::Null);
    }

    #[test]
    fn session_names_and_project_codes() {
        assert_eq!(session_title_for("SHA", 27, "[Bug] Buy - Back leaves without \"Leave without saving?\" and typed values are lost"), "[SHA-27] Buy - Back leaves without \"Leave without…");
        assert_eq!(session_title_for("BAK", 23, "Sửa lỗi đăng nhập"), "[BAK-23] Sửa lỗi đăng nhập");
        assert_eq!(session_title_for("API", 4, ""), "[API-4]");
        assert!(len16(&session_title_for("API", 5, &"x".repeat(200))) <= 52);
        for (p, k) in [("team/ba-qa-kit", "BAK"), ("bao/ba-qa-kit-sandbox", "BAS"), ("bao/html-css-training", "HTT"), ("g/shop_app", "SHA"), ("g/bobix", "BOB"), ("g/x2", "X2"), ("g/---", "ISSUE")] {
            assert_eq!(key_for(p), k, "{p}");
        }
        let m = |n: &str| names_issue(n, "BAK", "bao/ba-qa-kit-sandbox", 2);
        assert!(m("[BAK-2] Fix") && m("[BAK-2]") && m("ba-qa-kit-sandbox #2 Fix"));
        assert!(!m("[BAK-23] Fix") && !m("[BAS-2] Fix") && !m("ba-qa-kit-sandbox #23 Fix") && !m("Fix [BAK-2]"));
        assert_eq!(enc("/Users/a b/ğ?&"), "%2FUsers%2Fa%20b%2F%C4%9F%3F%26");
    }

    #[test]
    fn git_status_of_this_repository() {
        let dir = env!("CARGO_MANIFEST_DIR");
        let (g, _) = git_status(dir).expect("the source folder is a git checkout");
        assert!(g["top"].as_str().is_some_and(|t| !t.is_empty()) && g["worktrees"].as_u64() >= Some(1), "{g}");
        assert!(git_status(&std::env::temp_dir().join("no-such-bobix-dir").to_string_lossy()).is_none());
    }

    // Against the GitLab(s) the person's own glab is signed in to (read-only GET); prints counts only.
    // cargo test live_glab -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_glab() {
        let glab = crate::find_bin("glab").expect("glab on PATH");
        let hosts = glab_hosts(&glab).expect("glab runs");
        println!("hosts: {}", hosts.len());
        for h in &hosts {
            let r = fetch_issues(&glab, h);
            println!("host {}: ok {} reason {} issues {}", hosts.iter().position(|x| x == h).unwrap(), r["ok"], r["reason"], r["issues"].as_array().map_or(0, |a| a.len()));
            for i in r["issues"].as_array().into_iter().flatten() { assert!(i["iid"].is_i64() && i["url"].is_string() && i["project"].as_str().is_some_and(|p| p.contains('/'))); }
        }
    }
}
