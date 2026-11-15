// The work model: how projects, repositories, issues, branches, pull / merge requests, sessions and their pets link
// up, and why each link is there. Built from what Bobix already holds (hook events, git status, the issue trackers'
// last read, Claude desktop's session files) whenever a snapshot goes to the pages; nothing here reads the network or
// the disk. The board, the pets and the report read it; docs/work-model.md draws it.
use crate::{links::issue_session, state::{Session, Sessions}, work::{key_for, key_of, Forge, Work, FORGES}};
use fancy_regex::Regex;
use percent_encoding::percent_decode_str;
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::LazyLock};

fn re(s: &str) -> Regex { Regex::new(s).unwrap() }

// Why a link exists.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Why {
    Remote,      // a git remote of the folder points at the project on a host the tracker CLI is signed in to
    Folder,      // the session runs in the repository's folder
    Started,     // New on the issue in the panel opened this session (and named it "[KEY-iid] …")
    Branch,      // a branch named after the issue: "<iid>-…" or "<prefix>/<iid>-…"
    SessionName, // a Claude desktop session named "[KEY-iid] …" or "<project name> #<iid> …"
    Parent,      // spawnedFrom in Claude desktop's session file: a side session of that session
    ForgeLink,   // the issues GitHub itself links to a pull request (closing references)
    Closes,      // "Closes #n", "Fixes #n", "Resolves #n", "Implements #n" in the pull / merge request's text
}

// ---------- link rules ----------

// A git remote URL → project path ("group/sub/project" on GitLab, "owner/repo" on GitHub, lower case) when it points
// at `host`, else None. Forms: git@host:g/p.git, ssh://git@host[:port]/g/p.git, https://[user@]host[:port]/g/p(.git).
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

// A repository's host and project path: the first signed-in host one of its remotes points at.
// ponytail: the same path on two hosts (two GitLabs, or GitLab and GitHub) counts as one project; key by host too
// if anyone works like that.
pub fn repo_of(remotes: &[String], hosts: &[String]) -> Option<(String, String)> {
    hosts.iter().find_map(|h| remotes.iter().find_map(|u| project_path(u, h)).map(|p| (h.clone(), p)))
}
pub fn path_in(remotes: &[String], hosts: &[String]) -> Option<String> { repo_of(remotes, hosts).map(|x| x.1) }

// GitLab's "create branch" from an issue names it "<iid>-<title>", also under a prefix ("feat/<iid>-…"). A number
// followed by a part that is only digits is a date or a version ("release/2026-11", "hotfix/2026-11-09"), not an issue.
pub fn issue_of_branch(branch: &str) -> Option<i64> {
    static B: LazyLock<Regex> = LazyLock::new(|| re(r"(?:^|/)(\d+)-(?!\d+(?:[-/.]|$))"));
    B.captures(branch).ok().flatten().and_then(|m| m[1].parse().ok())
}

// Is `name` a session name of that issue? "[KEY-iid]" or "<project name> #<iid>", alone or followed by a space.
pub fn names_issue(name: &str, key: &str, path: &str, iid: i64) -> bool {
    [format!("[{key}-{iid}]"), format!("{} #{iid}", path.rsplit('/').next().unwrap_or(""))].iter()
        .any(|p| name == p || name.starts_with(&format!("{p} ")))
}

// The issue numbers a description closes: "Closes #52", "fixes: #7", "Resolves #3" (GitHub's and GitLab's words).
pub fn closes_in(text: &str) -> Vec<i64> {
    static C: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?|implement(?:s|ed)?)\s*:?\s+#(\d+)\b"));
    C.captures_iter(text).flatten().filter_map(|m| m[1].parse().ok()).collect()
}

// The issue a session works on (its project and number), and why: the session's own start in Work, else the newest of
// its branches that names one, else its parent's (a side session on a branch Claude names). `path`: its repository's
// project. A start names its issue's project, which is another one when the issue's code is in this repository: the
// session is listed under the issue it was started for.
pub fn issue_of_session(path: Option<&str>, started: Option<&Value>, branches: &[String], parent: Option<(String, i64)>) -> Option<(String, i64, Why)> {
    let path = path?;
    if let Some((p, i)) = started.and_then(|s| Some((s["path"].as_str()?, s["iid"].as_i64()?))) { return Some((p.to_string(), i, Why::Started)); }
    branches.iter().rev().find_map(|b| issue_of_branch(b)).map(|i| (path.to_string(), i, Why::Branch)).or(parent.map(|(p, i)| (p, i, Why::Parent)))
}

// ---------- pull requests (GitHub) and merge requests (GitLab) ----------

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Link { pub project: String, pub iid: i64, pub why: Why }

// One of your open pull / merge requests and the issues it closes. Only open ones are read today (state "open");
// review: GitHub's review decision (approved, changes_requested, review_required), none on GitLab yet. created / updated:
// when it was opened and last changed (ISO time); reviewed: someone reviewed it (GitHub: a review; GitLab: a comment on it).
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Review { pub kind: &'static str, pub url: String, pub project: String, pub branch: String, pub state: &'static str, pub review: Option<String>,
    pub created: Option<String>, pub updated: Option<String>, pub reviewed: bool, pub issues: Vec<Link> }

// The issues it closes: the links the forge gives, "Closes #n" in its text, and a branch named after the issue, in
// the same project. Each issue once, for its first reason.
#[allow(clippy::too_many_arguments)]
fn review(kind: &'static str, project: String, url: &Value, linked: Vec<(String, i64)>, text: &str, branch: &str, review: Option<String>, (created, updated): (&Value, &Value), reviewed: bool) -> Option<Review> {
    let url = url.as_str()?.to_string();
    let mut issues: Vec<Link> = vec![];
    let all = linked.into_iter().map(|(p, n)| (p, n, Why::ForgeLink))
        .chain(closes_in(text).into_iter().map(|n| (project.clone(), n, Why::Closes)))
        .chain(issue_of_branch(branch).map(|n| (project.clone(), n, Why::Branch)));
    for (p, n, why) in all {
        if !p.is_empty() && !issues.iter().any(|l| l.project == p && l.iid == n) { issues.push(Link { project: p, iid: n, why }); }
    }
    Some(Review { kind, url, project, branch: branch.into(), state: "open", review, created: created.as_str().map(String::from), updated: updated.as_str().map(String::from), reviewed, issues })
}
pub fn gh_pr(pr: &Value) -> Option<Review> {
    let repo = |v: &Value| v["repository"]["nameWithOwner"].as_str().unwrap_or("").to_lowercase();
    let linked = pr["closingIssuesReferences"]["nodes"].as_array().into_iter().flatten().filter_map(|n| Some((repo(n), n["number"].as_i64()?))).collect();
    let decision = pr["reviewDecision"].as_str().map(str::to_lowercase);
    let reviewed = pr["reviews"]["totalCount"].as_u64().is_some_and(|n| n > 0) || matches!(decision.as_deref(), Some("approved" | "changes_requested"));
    review("pr", repo(pr), &pr["url"], linked, pr["body"].as_str().unwrap_or(""), pr["headRefName"].as_str().unwrap_or(""), decision, (&pr["createdAt"], &pr["updatedAt"]), reviewed)
}
pub fn gl_mr(mr: &Value) -> Option<Review> {
    static REF: LazyLock<Regex> = LazyLock::new(|| re(r"!\d+$"));
    let project = REF.replace(mr["references"]["full"].as_str().unwrap_or(""), "").to_lowercase();
    let reviewed = mr["user_notes_count"].as_u64().is_some_and(|n| n > 0);
    review("mr", project, &mr["web_url"], vec![], mr["description"].as_str().unwrap_or(""), mr["source_branch"].as_str().unwrap_or(""), None, (&mr["created_at"], &mr["updated_at"]), reviewed)
}

// The stage of an open issue: ask (a live session on it waits for your approval), doing (one works on it), both even
// with an open pull / merge request; else verify (an open PR / MR closes it), else doing (a live session is on it),
// else todo. Done (closed today) is the panel's: it has the clock.
pub fn stage_of(review: bool, sessions: &[&str]) -> &'static str {
    if sessions.contains(&"waiting") { "ask" } else if sessions.contains(&"working") { "doing" } else if review { "verify" } else if !sessions.is_empty() { "doing" } else { "todo" }
}

// ---------- the model ----------

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Via<T> { #[serde(flatten)] pub to: T, pub why: Why }

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Id { pub id: String }
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Path { pub path: String }
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct IssueRef { pub project: String, pub iid: i64 }
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Url { pub url: String }

// A repository of a project: where its issues are tracked (forge, host), and the folders here that hold a clone.
#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Repo { pub path: String, pub key: String, pub auto_key: String, pub forge: &'static str, pub host: String, pub dirs: Vec<Via<Path>> }

// An open issue assigned to you, or one a session here works on (read on its own, see Forge::issue). stage: see stage_of. resume: a Claude desktop session named after it, in its repository's folder.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Issue { pub project: String, pub iid: i64, pub title: Value, pub url: Value, pub stage: &'static str,
    pub sessions: Vec<Via<Id>>, pub reviews: Vec<Via<Url>>, pub resume: Option<Via<Id>> }

// A branch Bobix saw: a session's (now or earlier in its transcript), or a pull / merge request's source branch.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Branch { pub project: String, pub name: String, pub issue: Option<Via<IssueRef>>, pub sessions: Vec<String>, pub reviews: Vec<String> }

// A permission request the session waits on, since when (ms). held: Bobix holds it, so it can be answered from Bobix.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Approval { pub tool: String, pub ask: String, pub since: u64, pub held: bool }

// A Claude Code session, and its pet (one per session; "auto" | "hidden" | "shown").
#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SessionLinks {
    pub id: String, pub status: String, pub since: u64, pub last_activity: u64, pub approvals: Vec<Approval>, pub pet: String,
    pub dir: Option<Via<Path>>, pub repo: Option<Via<Path>>, pub issue: Option<Via<IssueRef>>, pub parent: Option<Via<Id>>,
}

#[derive(Serialize, Default, Clone, PartialEq, Debug)]
pub struct Model { pub repos: Vec<Repo>, pub issues: Vec<Issue>, pub reviews: Vec<Review>, pub branches: Vec<Branch>, pub sessions: Vec<SessionLinks> }

fn host_of(url: &Value) -> String { url.as_str().and_then(|u| tauri::Url::parse(u).ok()).and_then(|u| u.host_str().map(String::from)).unwrap_or_default() }

// A session's branches, oldest first: the ones in its transcript, then the one it is on now.
fn branches_of(s: &Session) -> Vec<String> {
    s.branches.iter().cloned().chain(s.git.as_ref().and_then(|g| g["branch"].as_str()).map(String::from)).collect()
}

impl Model {
    pub fn build(settings: &Value, sessions: &Sessions, work: &Work) -> Model {
        let on: Vec<Forge> = FORGES.into_iter().filter(|f| f.on(settings)).collect();
        let hosts = work.hosts(settings);
        let repo_at = |top: &Value| repo_of(work.remotes_of.get(top.as_str().unwrap_or("")).map(Vec::as_slice).unwrap_or(&[]), &hosts);
        // Each session's repository (host, project), read once.
        let repo_of_session: HashMap<&str, (String, String)> = sessions.values().filter_map(|s| Some((s.id.as_str(), repo_at(&s.git.as_ref()?["top"])?))).collect();
        let path_of = |s: &Session| repo_of_session.get(s.id.as_str()).map(|r| r.1.clone());
        // Parents by Claude desktop session id. The issue a side session inherits comes from a parent that has a git
        // status; the parent the pages show may be any session (the same id twice: the later one).
        let by_host: HashMap<&str, &Session> = sessions.values().filter_map(|s| Some((s.host_session.as_deref()?, s))).collect();
        let by_host_git: HashMap<&str, &Session> = sessions.values().filter(|s| s.git.is_some()).filter_map(|s| Some((s.host_session.as_deref()?, s))).collect();

        let mut m = Model::default();
        for s in sessions.values() {
            let git = s.git.as_ref();
            let path = path_of(s);
            let parent_issue = s.parent_host.as_deref().and_then(|h| by_host_git.get(h))
                .filter(|p| path.is_some() && path_of(p) == path)
                .and_then(|p| issue_of_session(path.as_deref(), p.started.as_ref(), &branches_of(p), None)).map(|x| (x.0, x.1));
            let issue = git.and_then(|_| issue_of_session(path.as_deref(), s.started.as_ref(), &branches_of(s), parent_issue));
            let dir = git.map(|g| s.project.clone().filter(|p| !p.is_empty()).unwrap_or_else(|| g["top"].as_str().unwrap_or("").to_string()));
            m.sessions.push(SessionLinks {
                id: s.id.clone(), status: s.state.clone(), since: s.since, last_activity: s.updated, pet: s.pet.clone(),
                approvals: if s.state == "waiting" { vec![Approval { tool: s.ask_tool.clone(), ask: s.ask.clone(), since: s.since, held: s.answer.is_some() }] } else { vec![] },
                dir: dir.map(|d| Via { to: Path { path: d }, why: Why::Folder }),
                repo: path.clone().map(|p| Via { to: Path { path: p }, why: Why::Remote }),
                issue: issue.map(|(p, iid, why)| Via { to: IssueRef { project: p, iid }, why }),
                parent: s.parent_host.as_deref().and_then(|h| by_host.get(h)).map(|p| Via { to: Id { id: p.id.clone() }, why: Why::Parent }),
            });
        }

        for f in &on { m.reviews.extend(work.feeds[*f as usize].reviews.iter().cloned()); }
        let live = |p: &str, iid: i64| m.sessions.iter().filter(|s| s.issue.as_ref().is_some_and(|i| i.to.project == p && i.to.iid == iid))
            .map(|s| Via { to: Id { id: s.id.clone() }, why: s.issue.as_ref().unwrap().why }).collect::<Vec<_>>();
        // Projects: the ones a session's folder belongs to, and the ones of your issues.
        let mut repos: indexmap::IndexMap<String, Repo> = Default::default();
        let mut add_repo = |path: &str, forge: Forge, host: String, dir: Option<&str>| {
            let r = repos.entry(path.to_string()).or_insert_with(|| Repo { path: path.into(), key: key_of(settings, path), auto_key: key_for(path), forge: forge.key(), host, dirs: vec![] });
            if let Some(d) = dir.filter(|d| !r.dirs.iter().any(|x| x.to.path == *d)) { r.dirs.push(Via { to: Path { path: d.into() }, why: Why::Folder }); }
        };
        for (s, x) in sessions.values().zip(&m.sessions) {
            let Some((host, path)) = repo_of_session.get(s.id.as_str()).cloned() else { continue };
            let forge = on.iter().copied().find(|f| work.feeds[*f as usize].issues["hosts"].as_array().is_some_and(|h| h.iter().any(|y| y == host.as_str()))).unwrap_or(Forge::GitLab);
            add_repo(&path, forge, host, x.dir.as_ref().map(|d| d.to.path.as_str()));
        }
        for f in &on {
            for i in work.feeds[*f as usize].issues["list"].as_array().into_iter().flatten() {
                let (p, iid) = (i["project"].as_str().unwrap_or("").to_string(), i["iid"].as_i64().unwrap_or(-1));
                add_repo(&p, *f, host_of(&i["url"]), None);
                let reviews: Vec<Via<Url>> = m.reviews.iter().filter_map(|r| r.issues.iter().find(|l| l.project == p && l.iid == iid).map(|l| Via { to: Url { url: r.url.clone() }, why: l.why })).collect();
                let sessions = live(&p, iid);
                let statuses: Vec<&str> = sessions.iter().filter_map(|v| m.sessions.iter().find(|s| s.id == v.to.id)).map(|s| s.status.as_str()).collect();
                let stage = stage_of(!reviews.is_empty(), &statuses);
                m.issues.push(Issue { project: p, iid, title: i["title"].clone(), url: i["url"].clone(), stage, sessions, reviews, resume: None });
            }
        }
        // A Claude desktop session named after an issue nobody works on here, in the folder of a live session there.
        for i in m.issues.iter_mut().filter(|i| i.sessions.is_empty()) {
            let Some(dir) = m.sessions.iter().rev().find(|s| s.repo.as_ref().is_some_and(|r| r.to.path == i.project)).and_then(|s| s.dir.as_ref()) else { continue };
            let key = key_of(settings, &i.project);
            i.resume = issue_session(&work.claude_list, &dir.to.path, |t| names_issue(t, &key, &i.project, i.iid)).map(|id| Via { to: Id { id }, why: Why::SessionName });
        }
        repos.sort_keys();
        m.repos = repos.into_values().collect();

        // Branches, per project: sessions' (now and before) and the reviews' source branches.
        let mut branches: indexmap::IndexMap<(String, String), Branch> = Default::default();
        fn add<'a>(all: &'a mut indexmap::IndexMap<(String, String), Branch>, project: &str, name: &str) -> &'a mut Branch {
            all.entry((project.into(), name.into())).or_insert_with(|| Branch { project: project.into(), name: name.into(),
                issue: issue_of_branch(name).map(|iid| Via { to: IssueRef { project: project.into(), iid }, why: Why::Branch }), sessions: vec![], reviews: vec![] })
        }
        for (s, x) in sessions.values().zip(&m.sessions) {
            let Some(p) = x.repo.as_ref().map(|r| r.to.path.as_str()) else { continue };
            let now = s.git.as_ref().and_then(|g| g["branch"].as_str());
            for b in branches_of(s) {
                let br = add(&mut branches, p, &b);
                if Some(b.as_str()) == now && !br.sessions.contains(&s.id) { br.sessions.push(s.id.clone()); }
            }
        }
        for r in &m.reviews { if !r.branch.is_empty() { add(&mut branches, &r.project, &r.branch).reviews.push(r.url.clone()); } }
        m.branches = branches.into_values().collect();
        m
    }

    pub fn session(&self, id: &str) -> Option<&SessionLinks> { self.sessions.iter().find(|s| s.id == id) }
    // The issues sessions here work on, with their repository's host: (host, project, iid), each once.
    pub fn linked(&self) -> Vec<(String, String, i64)> {
        let mut out = vec![];
        for i in self.sessions.iter().filter_map(|s| s.issue.as_ref()) {
            let Some(r) = self.repos.iter().find(|r| r.path == i.to.project && !r.host.is_empty()) else { continue };
            let x = (r.host.clone(), i.to.project.clone(), i.to.iid);
            if !out.contains(&x) { out.push(x); }
        }
        out
    }
    pub fn issue(&self, project: &str, iid: i64) -> Option<&Issue> { self.issues.iter().find(|i| i.project == project && i.iid == iid) }

    // The read-only view the pages get (snapshot "model").
    pub fn json(&self) -> Value { serde_json::to_value(self).unwrap_or(json!({})) }
}


// ---------- stuck work and the stages of the day, in Rust (the page's board.js computes the same; tests hold both) ----------

// Milliseconds since 1970 of an ISO 8601 text ("2026-11-09T10:00:00Z", with or without fractions or an offset); None when it
// is not one (the page's Date.parse gives NaN, and an item with no date is left out the same way).
pub fn parse_iso(s: &str) -> Option<f64> {
    let b = s.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' { return None; }
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let (mut h, mut mi, mut sec, mut ms, mut off) = (0, 0, 0, 0.0, 0i64);
    if b.len() > 10 {
        if b[10] != b'T' && b[10] != b' ' || b.len() < 16 { return None; }
        h = n(11..13)?; mi = n(14..16)?;
        let mut i = 16;
        if b.get(i) == Some(&b':') { sec = n(i + 1..i + 3)?; i += 3; }
        if b.get(i) == Some(&b'.') { let j = (i + 1..b.len()).find(|&k| !b[k].is_ascii_digit()).unwrap_or(b.len()); ms = format!("0.{}", &s[i + 1..j]).parse::<f64>().ok()? * 1000.0; i = j; }
        match b.get(i) {
            Some(b'Z') | None => {}
            Some(&c @ (b'+' | b'-')) => { let (oh, om) = (n(i + 1..i + 3)?, n((i + 4).min(s.len())..(i + 6).min(s.len())).unwrap_or(0)); off = (oh * 60 + om) * if c == b'-' { -1 } else { 1 }; }
            _ => return None,
        }
    }
    // days from civil (Howard Hinnant's algorithm)
    let (yy, mm) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * mm + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(((days * 86400 + h * 3600 + (mi - off) * 60 + sec) * 1000) as f64 + ms)
}

const STUCK_DEFAULTS: [(&str, f64); 5] = [("stuckNoPr", 2.0), ("stuckNoReview", 24.0), ("stuckBranch", 4.0), ("stuckIdle", 4.0), ("stuckMaxAge", 14.0)];

// Work that stands still, one item each (see board.js `stuck`): nopr, noreview, branch, idle; most stuck first within each
// kind; `at` its last activity and `old` when quiet longer than stuckMaxAge days; skipped items (cfg.stuckSkipped: id → its
// `at` then) are left out until they move. `model`: the work model as the pages get it; `sessions`: their JSON.
pub fn stuck(model: &Value, sessions: &[Value], cfg: &Value, now: f64) -> Vec<Value> {
    if !model.is_object() || cfg["stuckOn"] == false { return vec![]; }
    const H: f64 = 3_600_000.0;
    let by_id: HashMap<&str, &Value> = sessions.iter().filter_map(|s| Some((s["id"].as_str()?, s))).collect();
    let hours = |k: &str| cfg[k].as_f64().unwrap_or_else(|| STUCK_DEFAULTS.iter().find(|d| d.0 == k).map_or(0.0, |d| d.1));
    let over = |ms: f64, k: &str| ms >= hours(k) * H;
    let busy = |s: &Value| matches!(s["state"].as_str(), Some("working" | "waiting"));
    let num = |s: &Value, k: &str| s[k].as_f64().unwrap_or(0.0);
    // the first session with the greatest `k` (reduce keeps the earlier one on a tie)
    let newest = |ss: &[&Value], k: &str| -> Value { ss.iter().fold(ss[0], |a, b| if num(b, k) > num(a, k) { b } else { a }).clone() };
    let reviewed: std::collections::HashSet<String> = model["reviews"].as_array().into_iter().flatten().map(|r| format!("{}\n{}", r["project"].as_str().unwrap_or(""), r["branch"].as_str().unwrap_or(""))).collect();
    let mut items: [Vec<Value>; 4] = Default::default(); // nopr, noreview, branch, idle
    for i in model["issues"].as_array().into_iter().flatten() {
        let ss: Vec<&Value> = i["sessions"].as_array().into_iter().flatten().filter_map(|x| by_id.get(x["id"].as_str()?).copied()).collect();
        if i["reviews"].as_array().is_some_and(|r| !r.is_empty()) || ss.is_empty() || ss.iter().any(|s| busy(s)) { continue; }
        let done: Vec<&Value> = ss.iter().copied().filter(|s| s["state"] == "done").collect();
        let (kind, k, s, at_key) = if done.is_empty() { ("idle", "stuckIdle", newest(&ss, "updated"), "updated") } else { ("nopr", "stuckNoPr", newest(&done, "since"), "since") };
        let ms = now - num(&s, at_key);
        if over(ms, k) {
            let (project, iid) = (i["project"].as_str().unwrap_or(""), i["iid"].as_i64().unwrap_or(0));
            items[if kind == "nopr" { 0 } else { 3 }].push(json!({ "id": format!("{kind}:{project}#{iid}"), "kind": kind, "project": project, "iid": iid, "title": i["title"], "ms": ms, "limit": hours(k), "session": s["id"], "branch": s["git"]["branch"], "at": num(&newest(&ss, "updated"), "updated") }));
        }
    }
    for r in model["reviews"].as_array().into_iter().flatten() {
        let Some(created) = r["created"].as_str().and_then(parse_iso) else { continue };
        let ms = now - created;
        if r["reviewed"] == true || !(ms >= 0.0) || !over(ms, "stuckNoReview") { continue; }
        let l = &r["issues"][0];
        let i = model["issues"].as_array().into_iter().flatten().find(|x| x["project"] == l["project"] && x["iid"] == l["iid"] && !l.is_null());
        let at = created.max(r["updated"].as_str().and_then(parse_iso).unwrap_or(0.0));
        items[1].push(json!({ "id": format!("noreview:{}", r["url"].as_str().unwrap_or("")), "kind": "noreview", "project": r["project"], "iid": i.map(|i| i["iid"].clone()).unwrap_or(Value::Null),
            "title": i.map(|i| i["title"].clone()).filter(|t| t.as_str().is_some_and(|t| !t.is_empty())).unwrap_or_else(|| r["branch"].clone()), "ms": ms, "limit": hours("stuckNoReview"), "url": r["url"], "review": r["kind"], "at": at }));
    }
    for b in model["branches"].as_array().into_iter().flatten() {
        let ss: Vec<&Value> = b["sessions"].as_array().into_iter().flatten().filter_map(|id| by_id.get(id.as_str()?).copied()).collect();
        let Some(g) = ss.iter().find(|s| s["git"]["branch"] == b["name"]).map(|s| &s["git"]) else { continue };
        let (own, last) = (g["own"].as_f64().unwrap_or(0.0), g["last"]["t"].as_f64().unwrap_or(0.0));
        let key = format!("{}\n{}", b["project"].as_str().unwrap_or(""), b["name"].as_str().unwrap_or(""));
        if !(own > 0.0) || last == 0.0 || ss.iter().any(|s| busy(s)) || reviewed.contains(&key) { continue; }
        if items[0].iter().chain(&items[3]).any(|x| x["project"] == b["project"] && x["branch"] == b["name"]) { continue; }
        let ms = now - last;
        if over(ms, "stuckBranch") {
            let n = newest(&ss, "updated");
            items[2].push(json!({ "id": format!("branch:{}:{}", b["project"].as_str().unwrap_or(""), b["name"].as_str().unwrap_or("")), "kind": "branch", "project": b["project"], "iid": b["issue"]["iid"].clone(), "title": b["name"], "ms": ms, "limit": hours("stuckBranch"), "commits": own, "branch": b["name"], "session": n["id"], "at": last.max(num(&n, "updated")) }));
        }
    }
    let skipped = &cfg["stuckSkipped"];
    let max_age = hours("stuckMaxAge") * 24.0 * H;
    let mut out = vec![];
    for k in [0, 1, 2, 3] {
        let mut v: Vec<Value> = items[k].iter().filter(|x| !(skipped[x["id"].as_str().unwrap_or("")].as_f64().is_some_and(|s| s >= x["at"].as_f64().unwrap_or(0.0)))).cloned().collect();
        for x in &mut v { x["old"] = json!(now - x["at"].as_f64().unwrap_or(0.0) > max_age); }
        v.sort_by(|a, b| b["ms"].as_f64().partial_cmp(&a["ms"].as_f64()).unwrap_or(std::cmp::Ordering::Equal));
        out.extend(v);
    }
    out
}

// Each open issue in one stage of the day (see board.js `stages`): [issue, group path, live session ids] per list. `done`: the
// issues closed on the same day as `now`, by `day` (the local calendar day of a time), once each; most urgent first in doing.
pub fn stages(work: &Value, sessions: &[Value], now: f64, day: &dyn Fn(f64) -> String) -> Value {
    let mut out = json!({ "todo": [], "doing": [], "verify": [], "done": [] });
    if !work.is_object() || work["status"] == "off" { return out; }
    let st: HashMap<&str, &Value> = sessions.iter().filter_map(|s| Some((s["id"].as_str()?, s))).collect();
    let pri = |s: &str| match s { "waiting" => 0, "working" => 1, "done" => 2, _ => 3 };
    let groups: Vec<&Value> = work["local"].as_array().into_iter().flatten().chain(work["away"].as_array().into_iter().flatten()).collect();
    let mut doing: Vec<(Value, i32, f64)> = vec![];
    for g in &groups {
        for i in g["issues"].as_array().into_iter().flatten() {
            let live: Vec<&Value> = i["sessions"].as_array().into_iter().flatten().filter_map(|id| st.get(id.as_str()?).copied()).collect();
            let ids: Vec<Value> = live.iter().map(|s| s["id"].clone()).collect();
            let item = json!([i, g["path"], ids]);
            match i["stage"].as_str() {
                Some("ask" | "doing") => {
                    let lead = live.iter().map(|s| pri(s["st"].as_str().unwrap_or("idle"))).min().unwrap_or(i32::MAX);
                    let last = live.iter().map(|s| s["since"].as_f64().unwrap_or(0.0)).fold(0.0, f64::max);
                    doing.push((item, lead, last));
                }
                Some("verify") => out["verify"].as_array_mut().unwrap().push(item),
                _ => out["todo"].as_array_mut().unwrap().push(item),
            }
        }
    }
    doing.sort_by(|a, b| a.1.cmp(&b.1).then(b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal)));
    out["doing"] = Value::Array(doing.into_iter().map(|d| d.0).collect());
    let today = day(now);
    for i in work["closed"].as_array().into_iter().flatten() {
        let Some(t) = i["closedAt"].as_str().and_then(parse_iso) else { continue };
        if day(t) != today { continue; }
        let dup = out["done"].as_array().unwrap().iter().any(|d| d[0]["iid"] == i["iid"] && d[1] == i["project"]);
        if !dup { out["done"].as_array_mut().unwrap().push(json!([i, i["project"], []])); }
    }
    out
}

// The local calendar day of a time, "YYYY-MM-DD" (the clock's offset where the OS tells it; UTC elsewhere).
pub fn local_day(ms: f64) -> String {
    let secs = (ms / 1000.0).floor() as i64 + local_offset_secs(ms);
    let days = secs.div_euclid(86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let (y, doy) = (yoe + era * 400, doe - (365 * yoe + yoe / 4 - yoe / 100));
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}
#[cfg(unix)]
fn local_offset_secs(ms: f64) -> i64 {
    let t = (ms / 1000.0).floor() as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: localtime_r fills `tm` from `t`; both are valid for the call.
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() { 0 } else { tm.tm_gmtoff as i64 }
}
// shortcut: Windows reads the day in UTC; the local offset through its time-zone API when it matters.
#[cfg(not(unix))]
fn local_offset_secs(_ms: f64) -> i64 { 0 }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_urls_give_the_project_only_on_its_host() {
        let h = "gitlab.example.com";
        assert_eq!(project_path("git@gitlab.example.com:team/shop-app.git", h).as_deref(), Some("team/shop-app"));
        assert_eq!(project_path("ssh://git@gitlab.example.com:2222/Group/Sub/Proj.git", h).as_deref(), Some("group/sub/proj"));
        assert_eq!(project_path("https://gitlab.example.com/team/shop-app", h).as_deref(), Some("team/shop-app"));
        assert_eq!(project_path("https://oauth2:secret@gitlab.example.com/g/p.git/", h).as_deref(), Some("g/p"));
        assert_eq!(project_path("https://gitlab.example.com:8443/g/p.git", "gitlab.example.com:8443").as_deref(), Some("g/p"));
        assert_eq!(project_path("git@github.com:acme/app.git", h), None);
        assert_eq!(project_path("https://gitlab.com/g/p.git", h), None);
        assert_eq!(project_path("/Users/me/repo.git", h), None);
        assert_eq!(project_path("git@gitlab.example.com:onlyname.git", h), None);
        assert_eq!(project_path("git@gitlab.example.com:g/p.git", ""), None);
        assert_eq!(project_path("git@github.com:Acme/App.git", "github.com").as_deref(), Some("acme/app"));
        assert_eq!(project_path("https://ghe.example.com/team/app.git", "ghe.example.com").as_deref(), Some("team/app"));
        let hosts = ["gitlab.example.com".to_string(), "github.com".to_string()];
        let remotes = ["git@elsewhere.org:x/y.git".to_string(), "https://github.com/Acme/App.git".to_string()];
        assert_eq!(repo_of(&remotes, &hosts), Some(("github.com".into(), "acme/app".into())), "the first remote on a signed-in host");
        assert_eq!(repo_of(&remotes[..1], &hosts), None);
    }

    #[test]
    fn branches_and_sessions_link_to_issues() {
        assert_eq!(issue_of_branch("29-buy-item-list-cannot-be-used"), Some(29));
        assert_eq!(issue_of_branch("feat/29-x"), Some(29));
        assert_eq!(issue_of_branch("v29-x"), None);
        assert_eq!(issue_of_branch("main"), None);
        for b in ["release/2026-11", "hotfix/2026-11-09", "2026-11-09-notes", "v1/2-3"] { assert_eq!(issue_of_branch(b), None, "{b}: a date or a version"); }
        assert_eq!(issue_of_branch("feat/7-2fa-login"), Some(7), "a title that starts with a digit (2FA)");
        let b = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let s = |br: &[&str], parent: Option<i64>, path: Option<&str>| issue_of_session(path, None, &b(br), parent.map(|i| ("g/a".to_string(), i)));
        let a = |iid: i64, why: Why| Some(("g/a".to_string(), iid, why));
        assert_eq!(s(&["29-old", "main", "feat/31-new", "main"], None, Some("g/a")), a(31, Why::Branch), "the newest branch that names one");
        assert_eq!(s(&["main", "HEAD"], None, Some("g/a")), None);
        assert_eq!(s(&["claude/brave-otter"], Some(26), Some("g/a")), a(26, Why::Parent));
        assert_eq!(s(&["29-x"], Some(26), Some("g/a")), a(29, Why::Branch), "its own branch first");
        assert_eq!(s(&["29-x"], None, None), None, "no project: no issue");
        assert_eq!(issue_of_session(Some("g/a"), Some(&json!({ "path": "g/a", "iid": 7 })), &b(&["29-x"]), None), a(7, Why::Started), "started from an issue");
        // started for an issue of another project, whose code is in this repository: that issue, in its own project
        assert_eq!(issue_of_session(Some("g/a"), Some(&json!({ "path": "g/b", "iid": 7 })), &b(&["tracker-7-x"]), None), Some(("g/b".to_string(), 7, Why::Started)));
        assert_eq!(issue_of_session(None, Some(&json!({ "path": "g/b", "iid": 7 })), &[], None), None, "a session in no project Bobix knows");
    }

    #[test]
    fn stages_from_reviews_and_sessions() {
        assert_eq!(stage_of(true, &["waiting"]), "ask", "a session still waiting on it: not yet in review");
        assert_eq!(stage_of(true, &["working", "done"]), "doing", "a session still working on it: not yet in review");
        assert_eq!(stage_of(true, &["done", "idle"]), "verify");
        assert_eq!(stage_of(false, &["working", "waiting"]), "ask");
        assert_eq!(stage_of(false, &["working", "idle"]), "doing");
        assert_eq!(stage_of(false, &[]), "todo");
    }

    #[test]
    fn session_names_name_issues() {
        let m = |n: &str| names_issue(n, "BAK", "bao/ba-qa-kit-sandbox", 2);
        assert!(m("[BAK-2] Fix") && m("[BAK-2]") && m("ba-qa-kit-sandbox #2 Fix"));
        assert!(!m("[BAK-23] Fix") && !m("[BAS-2] Fix") && !m("ba-qa-kit-sandbox #23 Fix") && !m("Fix [BAK-2]"));
    }

    #[test]
    fn reviews_name_the_issues_they_close_and_why() {
        assert_eq!(closes_in("Closes #52\nCloses #60 and fixes: #61. Resolves #7, see #8; closed#9 prefix#10"), [52, 60, 61, 7]);
        assert!(closes_in("Refs #4, mentions #5").is_empty());
        let pr: Value = serde_json::from_str(r#"{ "number": 70, "url": "https://github.com/Acme/Shop/pull/70", "headRefName": "feat/52-board-day",
            "body": "Closes #60\nCloses #52", "repository": { "nameWithOwner": "Acme/Shop" }, "reviewDecision": "CHANGES_REQUESTED",
            "closingIssuesReferences": { "nodes": [{ "number": 61, "repository": { "nameWithOwner": "Acme/Shop" } }, { "number": 3, "repository": { "nameWithOwner": "Acme/Other" } }] } }"#).unwrap();
        let r = gh_pr(&pr).unwrap();
        let got: Vec<(&str, i64, Why)> = r.issues.iter().map(|l| (l.project.as_str(), l.iid, l.why)).collect();
        assert_eq!(got, [("acme/shop", 61, Why::ForgeLink), ("acme/other", 3, Why::ForgeLink), ("acme/shop", 60, Why::Closes), ("acme/shop", 52, Why::Closes)], "#52 once, for its first reason");
        assert_eq!((r.kind, r.state, r.review.as_deref(), r.branch.as_str()), ("pr", "open", Some("changes_requested"), "feat/52-board-day"));
        assert!(r.reviewed && r.created.is_none(), "changes requested: someone reviewed it");
        let fresh = gh_pr(&json!({ "url": "u", "createdAt": "2026-11-08T01:00:00Z", "updatedAt": "2026-11-09T01:00:00Z", "reviews": { "totalCount": 0 }, "reviewDecision": "REVIEW_REQUIRED" })).unwrap();
        assert_eq!((fresh.created.as_deref(), fresh.updated.as_deref(), fresh.reviewed), (Some("2026-11-08T01:00:00Z"), Some("2026-11-09T01:00:00Z"), false));
        assert!(gh_pr(&json!({ "url": "u", "reviews": { "totalCount": 2 } })).unwrap().reviewed);
        let mr = json!({ "iid": 12, "web_url": "https://gl.example.com/g/sub/app/-/merge_requests/12", "references": { "full": "G/Sub/App!12" },
            "source_branch": "29-login-error", "description": null });
        let r = gl_mr(&mr).unwrap();
        assert_eq!(r.issues, [Link { project: "g/sub/app".into(), iid: 29, why: Why::Branch }]);
        assert_eq!((r.kind, r.review.as_ref()), ("mr", None));
        assert!(!r.reviewed);
        let noted = gl_mr(&json!({ "web_url": "u", "created_at": "2026-11-08T01:00:00Z", "updated_at": "2026-11-08T02:00:00Z", "user_notes_count": 3 })).unwrap();
        assert_eq!((noted.created.as_deref(), noted.updated.as_deref(), noted.reviewed), (Some("2026-11-08T01:00:00Z"), Some("2026-11-08T02:00:00Z"), true), "a comment on a merge request counts as a review");
        assert!(gl_mr(&json!({ "web_url": "u", "references": {}, "source_branch": "main" })).unwrap().issues.is_empty());
        assert!(gh_pr(&json!({})).is_none(), "no link to open: nothing to show");
    }

    // The same cases as the page's board tests (test/core.test.js): the two must agree.
    fn sess(id: &str, state: &str, since: f64, updated: f64, git: Value) -> Value { json!({ "id": id, "state": state, "since": since, "updated": updated, "git": git }) }

    #[test]
    fn stuck_work_one_item_each_past_the_limits() {
        const H: f64 = 3_600_000.0;
        let now = parse_iso("2026-11-09T10:00:00Z").unwrap();
        let iso = |t: f64| { let s = (t / 1000.0) as i64; let d = s.div_euclid(86400); let r = s.rem_euclid(86400); let z = d + 719468; let era = z.div_euclid(146097); let doe = z - era * 146097; let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); let mp = (5 * doy + 2) / 153; let dd = doy - (153 * mp + 2) / 5 + 1; let mm = if mp < 10 { mp + 3 } else { mp - 9 }; let yy = yoe + era * 400 + if mm <= 2 { 1 } else { 0 }; format!("{yy:04}-{mm:02}-{dd:02}T{:02}:{:02}:{:02}Z", r / 3600, r % 3600 / 60, r % 60) };
        let git = |b: &str, own: i64, ago: f64| json!({ "branch": b, "own": own, "last": { "t": now - ago * H } });
        let sessions = vec![
            sess("s1", "done", now - 3.0 * H, now - 3.0 * H, git("1-cart", 2, 5.0)), sess("s2", "done", now - H, now - H, Value::Null), sess("s3", "idle", now - 5.0 * H, now - 5.0 * H, git("3-login", 0, 9.0)),
            sess("s4", "working", now - 9.0 * H, now, Value::Null), sess("s5", "done", now - 9.0 * H, now - 9.0 * H, git("r-branch", 2, 40.0)), sess("s6", "done", now - 7.0 * H, now - 7.0 * H, git("feat/cache", 4, 6.0)),
            sess("s7", "idle", 0.0, 0.0, git("main", 0, 50.0)), sess("s8", "waiting", now, now, git("feat/busy", 3, 9.0)), sess("s9", "idle", 0.0, 0.0, git("feat/new", 3, 1.0)),
        ];
        let on = |ids: &[&str]| ids.iter().map(|id| json!({ "id": id, "why": "branch" })).collect::<Vec<_>>();
        let branches: Vec<Value> = ["1-cart:s1", "3-login:s3", "feat/cache:s6", "main:s7", "feat/busy:s8", "feat/new:s9", "r-branch:s5"].iter().map(|x| { let (n, s) = x.split_once(':').unwrap(); json!({ "project": "g/app", "name": n, "issue": null, "sessions": [s], "reviews": [] }) }).collect();
        let model = json!({
            "issues": [
                { "project": "g/app", "iid": 1, "title": "Cart", "sessions": on(&["s1"]), "reviews": [] }, { "project": "g/app", "iid": 2, "title": "Two", "sessions": on(&["s2"]), "reviews": [] },
                { "project": "g/app", "iid": 3, "title": "Login", "sessions": on(&["s3"]), "reviews": [] }, { "project": "g/app", "iid": 4, "title": "Four", "sessions": on(&["s4", "s3"]), "reviews": [] },
                { "project": "g/app", "iid": 5, "title": "Five", "sessions": on(&["s5"]), "reviews": [{ "url": "https://h/g/app/pull/9", "why": "closes" }] }, { "project": "g/app", "iid": 6, "title": "Six", "sessions": [], "reviews": [] }],
            "reviews": [
                { "kind": "pr", "url": "https://h/g/app/pull/9", "project": "g/app", "branch": "r-branch", "created": iso(now - 26.0 * H), "reviewed": false, "issues": [{ "project": "g/app", "iid": 5, "why": "closes" }] },
                { "kind": "mr", "url": "https://h/g/web/-/merge_requests/3", "project": "g/web", "branch": "x", "created": iso(now - 30.0 * H), "reviewed": true, "issues": [] },
                { "kind": "pr", "url": "https://h/g/app/pull/10", "project": "g/app", "branch": "y", "created": iso(now - H), "reviewed": false, "issues": [] },
                { "kind": "pr", "url": "https://h/g/app/pull/11", "project": "g/app", "branch": "z", "created": null, "reviewed": false, "issues": [] }],
            "branches": branches,
        });
        let got = stuck(&model, &sessions, &json!({}), now);
        let kinds: Vec<(String, String)> = got.iter().map(|x| (x["kind"].as_str().unwrap().into(), if x["iid"].is_null() { x["title"].as_str().unwrap().into() } else { x["iid"].to_string() })).collect();
        assert_eq!(kinds, [("nopr", "1"), ("noreview", "5"), ("branch", "feat/cache"), ("idle", "3")].map(|(a, b)| (a.to_string(), b.to_string())).to_vec(), "{got:?}");
        assert_eq!((got[0]["branch"].as_str(), got[0]["session"].as_str(), got[0]["limit"].as_f64(), got[0]["ms"].as_f64()), (Some("1-cart"), Some("s1"), Some(2.0), Some(3.0 * H)));
        assert_eq!((got[1]["url"].as_str(), got[1]["review"].as_str(), got[2]["commits"].as_f64(), got[3]["session"].as_str()), (Some("https://h/g/app/pull/9"), Some("pr"), Some(4.0), Some("s3")));
        assert!(stuck(&model, &sessions, &json!({ "stuckOn": false }), now).is_empty());
        let longer = stuck(&model, &sessions, &json!({ "stuckNoPr": 4, "stuckNoReview": 48, "stuckBranch": 5, "stuckIdle": 6 }), now);
        assert_eq!(longer.iter().map(|x| (x["kind"].as_str().unwrap(), x["title"].as_str().unwrap())).collect::<Vec<_>>(), [("branch", "feat/cache"), ("branch", "1-cart")]);
        assert!(stuck(&Value::Null, &sessions, &json!({}), now).is_empty());
        assert!(got.iter().all(|x| x["old"] == false) && got[0]["at"].as_f64() == Some(now - 3.0 * H));
    }

    #[test]
    fn quiet_work_is_old_and_a_skip_lasts_until_it_moves() {
        const H: f64 = 3_600_000.0;
        const D: f64 = 24.0 * H;
        let now = parse_iso("2026-11-09T10:00:00Z").unwrap();
        let fmt = |t: f64| { let s = (t / 1000.0).floor() as i64; let d = s.div_euclid(86400); let r = s.rem_euclid(86400); let z = d + 719468; let era = z.div_euclid(146097); let doe = z - era * 146097; let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); let mp = (5 * doy + 2) / 153; let dd = doy - (153 * mp + 2) / 5 + 1; let mm = if mp < 10 { mp + 3 } else { mp - 9 }; let yy = yoe + era * 400 + if mm <= 2 { 1 } else { 0 }; format!("{yy:04}-{mm:02}-{dd:02}T{:02}:{:02}:{:02}Z", r / 3600, r % 3600 / 60, r % 60) };
        let mut sessions = vec![sess("s1", "done", now - 3.0 * H, now - 3.0 * H, Value::Null), sess("o1", "idle", now - 30.0 * D, now - 30.0 * D, json!({ "branch": "feat/old", "own": 2, "last": { "t": now - 31.0 * D } }))];
        let model = json!({
            "issues": [{ "project": "g/a", "iid": 1, "title": "New", "sessions": [{ "id": "s1" }], "reviews": [] }],
            "reviews": [{ "kind": "pr", "url": "u/pull/1", "project": "g/a", "branch": "x", "created": fmt(now - 532.0 * D), "updated": fmt(now - 470.0 * D), "reviewed": false, "issues": [] },
                { "kind": "pr", "url": "u/pull/2", "project": "g/a", "branch": "y", "created": fmt(now - 40.0 * D), "updated": fmt(now - 2.0 * D), "reviewed": false, "issues": [] }],
            "branches": [{ "project": "g/a", "name": "feat/old", "issue": null, "sessions": ["o1"], "reviews": [] }],
        });
        let pick = |v: &[Value]| v.iter().map(|x| (x["id"].as_str().unwrap().to_string(), x["old"] == true)).collect::<Vec<_>>();
        let all = stuck(&model, &sessions, &json!({}), now);
        assert_eq!(pick(&all), [("nopr:g/a#1", false), ("noreview:u/pull/1", true), ("noreview:u/pull/2", false), ("branch:g/a:feat/old", true)].map(|(a, b)| (a.to_string(), b)).to_vec(), "quiet for over 14 days (by its last change, not its age): old");
        let old60: Vec<String> = pick(&stuck(&model, &sessions, &json!({ "stuckMaxAge": 60 }), now)).into_iter().filter(|x| x.1).map(|x| x.0).collect();
        assert_eq!(old60, ["noreview:u/pull/1"], "the limit is a setting (days)");
        let at_of = |id: &str| all.iter().find(|x| x["id"] == id).unwrap()["at"].as_f64().unwrap();
        let skipped = json!({ "nopr:g/a#1": at_of("nopr:g/a#1"), "noreview:u/pull/2": at_of("noreview:u/pull/2") - 1.0 });
        let ids: Vec<String> = stuck(&model, &sessions, &json!({ "stuckSkipped": skipped }), now).iter().map(|x| x["id"].as_str().unwrap().to_string()).collect();
        assert_eq!(ids, ["noreview:u/pull/1", "noreview:u/pull/2", "branch:g/a:feat/old"], "skipped: gone while nothing moved; one that changed since comes back");
        sessions[0]["updated"] = json!(now);
        assert!(stuck(&model, &sessions, &json!({ "stuckSkipped": skipped, "stuckNoPr": 0 }), now).iter().any(|x| x["id"] == "nopr:g/a#1"));
    }

    #[test]
    fn each_issue_in_one_stage_of_the_day() {
        let now = parse_iso("2026-11-08T17:00:00Z").unwrap();
        let day = |t: f64| format!("{}", (t / 86_400_000.0).floor()); // a UTC day is enough for the test
        let mk = |iid: i64, extra: Value| { let mut v = json!({ "iid": iid, "title": format!("t{iid}"), "url": format!("u{iid}"), "sessions": [], "labels": [] }); for (k, x) in extra.as_object().unwrap() { v[k] = x.clone(); } v };
        let work = json!({ "status": "ok",
            "local": [{ "path": "g/app", "dir": "/src/app", "issues": [mk(1, json!({ "sessions": ["a", "gone"], "stage": "doing" })), mk(2, json!({ "sessions": ["b"], "stage": "verify" })), mk(3, json!({ "stage": "todo" })), mk(4, json!({ "sessions": ["c"], "stage": "doing" }))] }],
            "away": [{ "path": "g/web", "issues": [mk(5, json!({ "stage": "todo" }))] }],
            "closed": [{ "iid": 7, "project": "g/app", "closedAt": "2026-11-08T16:00:00Z" }, { "iid": 8, "project": "g/web", "closedAt": "2026-11-07T23:00:00Z" }, { "iid": 7, "project": "g/app", "closedAt": "2026-11-08T16:59:00Z" }, { "iid": 9, "project": "g/new", "closedAt": "2026-11-08T17:00:00Z" }] });
        let ss = vec![json!({ "id": "a", "st": "working", "since": 5 }), json!({ "id": "b", "st": "done", "since": 6 }), json!({ "id": "c", "st": "waiting", "since": 1 }), json!({ "id": "z", "st": "idle", "since": 1 })];
        let out = stages(&work, &ss, now, &day);
        let ids = |k: &str| out[k].as_array().unwrap().iter().map(|x| x[0]["iid"].as_i64().unwrap()).collect::<Vec<_>>();
        assert_eq!((ids("todo"), ids("doing"), ids("verify"), ids("done")), (vec![3, 5], vec![4, 1], vec![2], vec![7, 9]), "each in the stage the model gave it; most urgent first; closed today only, once");
        assert_eq!(out["doing"][1][2], json!(["a"]), "a session that is gone does not count");
        assert_eq!(out["done"][1][1], "g/new", "a project with no open issue still names its group");
        assert_eq!(stages(&json!({ "status": "off" }), &ss, now, &day), json!({ "todo": [], "doing": [], "verify": [], "done": [] }));
    }

    #[test]
    fn iso_dates_and_the_local_day() {
        assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0.0));
        assert_eq!(parse_iso("2026-11-09T10:00:00.500+02:00"), Some(parse_iso("2026-11-09T08:00:00Z").unwrap() + 500.0));
        assert_eq!(parse_iso("2026-11-09"), parse_iso("2026-11-09T00:00:00Z"));
        assert_eq!(parse_iso("not a date"), None);
        assert_eq!(parse_iso(""), None);
        let d = local_day(parse_iso("2026-11-09T12:00:00Z").unwrap());
        assert!(d == "2026-11-09" || d == "2026-11-10" || d == "2026-11-08", "{d}");
    }
}
