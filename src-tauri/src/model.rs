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

// GitLab's "create branch" from an issue names it "<iid>-<title>", also under a prefix ("feat/<iid>-…").
pub fn issue_of_branch(branch: &str) -> Option<i64> {
    static B: LazyLock<Regex> = LazyLock::new(|| re(r"(?:^|/)(\d+)-"));
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

// The issue a session works on, and why: the session's own start (New on that issue), else the newest of its branches
// that names one, else its parent's (a side session on a branch Claude names). `path`: its repository's project.
pub fn issue_of_session(path: Option<&str>, started: Option<&Value>, branches: &[String], parent: Option<i64>) -> Option<(i64, Why)> {
    let path = path?;
    if let Some(s) = started.filter(|s| s["path"] == path) { return s["iid"].as_i64().map(|i| (i, Why::Started)); }
    branches.iter().rev().find_map(|b| issue_of_branch(b)).map(|i| (i, Why::Branch)).or(parent.map(|i| (i, Why::Parent)))
}

// ---------- pull requests (GitHub) and merge requests (GitLab) ----------

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Link { pub project: String, pub iid: i64, pub why: Why }

// One of your open pull / merge requests and the issues it closes. Only open ones are read today (state "open");
// review: GitHub's review decision (approved, changes_requested, review_required), none on GitLab yet.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Review { pub kind: &'static str, pub url: String, pub project: String, pub branch: String, pub state: &'static str, pub review: Option<String>, pub issues: Vec<Link> }

// The issues it closes: the links the forge gives, "Closes #n" in its text, and a branch named after the issue, in
// the same project. Each issue once, for its first reason.
fn review(kind: &'static str, project: String, url: &Value, linked: Vec<(String, i64)>, text: &str, branch: &str, review: Option<String>) -> Option<Review> {
    let url = url.as_str()?.to_string();
    let mut issues: Vec<Link> = vec![];
    let all = linked.into_iter().map(|(p, n)| (p, n, Why::ForgeLink))
        .chain(closes_in(text).into_iter().map(|n| (project.clone(), n, Why::Closes)))
        .chain(issue_of_branch(branch).map(|n| (project.clone(), n, Why::Branch)));
    for (p, n, why) in all {
        if !p.is_empty() && !issues.iter().any(|l| l.project == p && l.iid == n) { issues.push(Link { project: p, iid: n, why }); }
    }
    Some(Review { kind, url, project, branch: branch.into(), state: "open", review, issues })
}
pub fn gh_pr(pr: &Value) -> Option<Review> {
    let repo = |v: &Value| v["repository"]["nameWithOwner"].as_str().unwrap_or("").to_lowercase();
    let linked = pr["closingIssuesReferences"]["nodes"].as_array().into_iter().flatten().filter_map(|n| Some((repo(n), n["number"].as_i64()?))).collect();
    let decision = pr["reviewDecision"].as_str().map(str::to_lowercase);
    review("pr", repo(pr), &pr["url"], linked, pr["body"].as_str().unwrap_or(""), pr["headRefName"].as_str().unwrap_or(""), decision)
}
pub fn gl_mr(mr: &Value) -> Option<Review> {
    static REF: LazyLock<Regex> = LazyLock::new(|| re(r"!\d+$"));
    let project = REF.replace(mr["references"]["full"].as_str().unwrap_or(""), "").to_lowercase();
    review("mr", project, &mr["web_url"], vec![], mr["description"].as_str().unwrap_or(""), mr["source_branch"].as_str().unwrap_or(""), None)
}

// The stage of an open issue: verify (an open pull / merge request closes it), ask (a live session on it waits for
// your approval), doing (a live session works on it), else todo. Done (closed today) is the panel's: it has the clock.
pub fn stage_of(review: bool, sessions: &[&str]) -> &'static str {
    if review { "verify" } else if sessions.contains(&"waiting") { "ask" } else if !sessions.is_empty() { "doing" } else { "todo" }
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

// An open issue assigned to you. stage: see stage_of. resume: a Claude desktop session named after it, in its repository's folder.
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
            let parent_iid = s.parent_host.as_deref().and_then(|h| by_host_git.get(h))
                .filter(|p| path.is_some() && path_of(p) == path)
                .and_then(|p| issue_of_session(path.as_deref(), p.started.as_ref(), &branches_of(p), None)).map(|x| x.0);
            let issue = git.and_then(|_| issue_of_session(path.as_deref(), s.started.as_ref(), &branches_of(s), parent_iid));
            let dir = git.map(|g| s.project.clone().filter(|p| !p.is_empty()).unwrap_or_else(|| g["top"].as_str().unwrap_or("").to_string()));
            m.sessions.push(SessionLinks {
                id: s.id.clone(), status: s.state.clone(), since: s.since, last_activity: s.updated, pet: s.pet.clone(),
                approvals: if s.state == "waiting" { vec![Approval { tool: s.ask_tool.clone(), ask: s.ask.clone(), since: s.since, held: s.answer.is_some() }] } else { vec![] },
                dir: dir.map(|d| Via { to: Path { path: d }, why: Why::Folder }),
                repo: path.clone().map(|p| Via { to: Path { path: p }, why: Why::Remote }),
                issue: path.zip(issue).map(|(p, (iid, why))| Via { to: IssueRef { project: p, iid }, why }),
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
    pub fn issue(&self, project: &str, iid: i64) -> Option<&Issue> { self.issues.iter().find(|i| i.project == project && i.iid == iid) }

    // The read-only view the pages get (snapshot "model").
    pub fn json(&self) -> Value { serde_json::to_value(self).unwrap_or(json!({})) }
}

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
        let b = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let s = |br: &[&str], parent: Option<i64>, path: Option<&str>| issue_of_session(path, None, &b(br), parent);
        assert_eq!(s(&["29-old", "main", "feat/31-new", "main"], None, Some("g/a")), Some((31, Why::Branch)), "the newest branch that names one");
        assert_eq!(s(&["main", "HEAD"], None, Some("g/a")), None);
        assert_eq!(s(&["claude/brave-otter"], Some(26), Some("g/a")), Some((26, Why::Parent)));
        assert_eq!(s(&["29-x"], Some(26), Some("g/a")), Some((29, Why::Branch)), "its own branch first");
        assert_eq!(s(&["29-x"], None, None), None, "no project: no issue");
        assert_eq!(issue_of_session(Some("g/a"), Some(&json!({ "path": "g/a", "iid": 7 })), &b(&["29-x"]), None), Some((7, Why::Started)), "started from an issue");
        assert_eq!(issue_of_session(Some("g/a"), Some(&json!({ "path": "g/b", "iid": 7 })), &b(&["29-x"]), None), Some((29, Why::Branch)), "started in another project");
    }

    #[test]
    fn stages_from_reviews_and_sessions() {
        assert_eq!(stage_of(true, &["waiting"]), "verify");
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
        let mr = json!({ "iid": 12, "web_url": "https://gl.example.com/g/sub/app/-/merge_requests/12", "references": { "full": "G/Sub/App!12" },
            "source_branch": "29-login-error", "description": null });
        let r = gl_mr(&mr).unwrap();
        assert_eq!(r.issues, [Link { project: "g/sub/app".into(), iid: 29, why: Why::Branch }]);
        assert_eq!((r.kind, r.review.as_ref()), ("mr", None));
        assert!(gl_mr(&json!({ "web_url": "u", "references": {}, "source_branch": "main" })).unwrap().issues.is_empty());
        assert!(gh_pr(&json!({})).is_none(), "no link to open: nothing to show");
    }
}
