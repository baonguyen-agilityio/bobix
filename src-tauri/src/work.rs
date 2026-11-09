// Work items: a session's local git status (no network), and the GitLab / GitHub issues assigned to you, read through
// your own `glab` / `gh` (Bobix keeps no token, writes nothing to either). Commands get argument lists, never a shell.
// Port of the 0.1 (Electron) app's core/work.js, plus the Work tab's state from src/main.js (workView, takeIssue, startIssue...).
use crate::{js::{head16, len16}, links::ClaudeSession, model::{self, Model, Review}, procs};
use fancy_regex::Regex;
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
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

// The GitHub hosts gh reads with, from `gh auth status`: each host whose active account is signed in ("Logged in to
// <host> account <name>", then "- Active account: true"; gh api uses that account). Several accounts on one host list
// one block each. "Failed to log in" is a bad sign-in; "Timeout trying to log in" counts as signed in, so the read
// itself says what is wrong with the network. gh before 2.40 has one account per host: "Logged in to <host> as <name>".
// gh masks the token there; only these lines are read.
pub fn gh_logged_in_hosts(text: &str) -> Vec<String> {
    static ACCOUNT: LazyLock<Regex> = LazyLock::new(|| re(r"(?:Logged|log) in to (\S+) (account|as) "));
    let (mut out, mut last): (Vec<String>, Option<String>) = (vec![], None);
    let mut add = |h: String| if !out.contains(&h) { out.push(h) };
    for l in text.lines() {
        if let Ok(Some(m)) = ACCOUNT.captures(l) {
            let h = m[1].to_lowercase();
            last = (!l.contains("Failed to log in")).then(|| h.clone());
            if &m[2] == "as" && l.contains("Logged in to") { add(h); }
        } else if l.trim_start().starts_with("- Active account: true") {
            if let Some(h) = last.take() { add(h); }
        }
    }
    out
}

// Where an issue tracker's issues come from: GitLab through glab, GitHub through gh, each behind its own Settings switch.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Forge { GitLab, GitHub }
pub const FORGES: [Forge; 2] = [Forge::GitLab, Forge::GitHub];

impl Forge {
    pub fn key(self) -> &'static str { match self { Forge::GitLab => "gitlab", Forge::GitHub => "github" } } // its Settings switch
    pub fn bin(self) -> &'static str { match self { Forge::GitLab => "glab", Forge::GitHub => "gh" } }
    // Its statuses for "the CLI is missing" and "the CLI is signed in nowhere"; the panel says which command to run.
    pub fn missing(self) -> &'static str { match self { Forge::GitLab => "noglab", Forge::GitHub => "nogh" } }
    pub fn signed_out(self) -> &'static str { match self { Forge::GitLab => "auth", Forge::GitHub => "ghauth" } }
    pub fn on(self, settings: &Value) -> bool { settings[self.key()] == true }
    // None when the CLI is missing, else the hosts it is signed in to (none when nowhere). Both exit non-zero when any
    // host fails, and older gh writes this to stderr.
    pub fn hosts(self, bin: &str) -> Option<Vec<String>> {
        if self == Forge::GitLab { return glab_hosts(bin); }
        let r = procs::exec(bin, &["auth", "status"], 30, GH_ENV);
        if r.missing { return None; }
        Some(gh_logged_in_hosts(&format!("{}\n{}", r.out, r.err)))
    }
    pub fn fetch(self, bin: &str, host: &str) -> Value {
        match self {
            Forge::GitLab => fetch_issues(bin, host),
            Forge::GitHub => read_issues(self, bin, &["api", "--hostname", host, "issues?filter=assigned&state=open&per_page=100"], GH_ENV, slim_gh_issue, "GitHub"),
        }
    }
    fn env(self) -> &'static [(&'static str, &'static str)] { if self == Forge::GitHub { GH_ENV } else { &[] } }
    // Your open pull requests (GitHub) / merge requests (GitLab), with the issues they close: those wait for review.
    // Read-only (GitHub's is a GraphQL query, no mutation); a failed read gives none, the issue list still shows.
    pub fn reviews(self, bin: &str, host: &str) -> Vec<Review> {
        let r = match self {
            Forge::GitHub => procs::exec(bin, &["api", "--hostname", host, "graphql", "-f", &format!("query={GH_PRS}")], 30, GH_ENV),
            Forge::GitLab => procs::exec(bin, &["api", "--hostname", host, "merge_requests?scope=created_by_me&state=opened&per_page=100"], 30, &[]),
        };
        let Ok(v) = serde_json::from_str::<Value>(&r.out) else { return vec![] };
        let list = if self == Forge::GitHub { &v["data"]["viewer"]["pullRequests"]["nodes"] } else { &v };
        list.as_array().into_iter().flatten().filter_map(|x| if self == Forge::GitHub { model::gh_pr(x) } else { model::gl_mr(x) }).collect()
    }
    // Issues assigned to you that were closed since `since` (ISO time, UTC); the panel keeps the ones closed today.
    pub fn closed(self, bin: &str, host: &str, since: &str) -> Vec<Value> {
        let (path, slim): (String, fn(&Value) -> Option<Value>) = match self {
            Forge::GitHub => (format!("issues?filter=assigned&state=closed&since={since}&per_page=100"), slim_gh_issue),
            Forge::GitLab => (format!("issues?scope=assigned_to_me&state=closed&updated_after={since}&per_page=100"), |i| Some(slim_issue(i))),
        };
        let r = procs::exec(bin, &["api", "--hostname", host, &path], 30, self.env());
        let Ok(Value::Array(a)) = serde_json::from_str::<Value>(&r.out) else { return vec![] };
        a.iter().filter_map(|i| {
            let at = i["closed_at"].as_str().filter(|t| *t >= since)?;
            let mut x = slim(i)?;
            x["closedAt"] = json!(at);
            Some(x)
        }).collect()
    }
}

// Your open pull requests on a GitHub host, with the issues GitHub links to each (closing references) and its review decision.
const GH_PRS: &str = "query{viewer{pullRequests(states:OPEN,first:100,orderBy:{field:UPDATED_AT,direction:DESC}){nodes{number url headRefName body reviewDecision repository{nameWithOwner} closingIssuesReferences(first:20){nodes{number repository{nameWithOwner}}}}}}}";

// gh never asks anything (no terminal) and does not look for a new version of itself.
const GH_ENV: &[(&str, &str)] = &[("GH_PROMPT_DISABLED", "1"), ("GH_NO_UPDATE_NOTIFIER", "1"), ("NO_COLOR", "1")];

pub fn any_on(settings: &Value) -> bool { FORGES.iter().any(|f| f.on(settings)) }

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

// The fields Bobix shows of one issue from GitLab's API.
pub fn slim_issue(i: &Value) -> Value {
    static REF: LazyLock<Regex> = LazyLock::new(|| re(r"#\d+$"));
    let labels: Vec<Value> = i["labels"].as_array().map(|a| a.iter().take(10).map(|l| json!(l.as_str().map(String::from).unwrap_or_else(|| l.to_string()))).collect()).unwrap_or_default();
    let due = i["due_date"].as_str().filter(|d| !d.is_empty()).map(|d| json!(d)).unwrap_or(Value::Null);
    json!({ "iid": i["iid"], "title": head16(i["title"].as_str().unwrap_or(""), 300), "url": i["web_url"],
        "project": REF.replace(i["references"]["full"].as_str().unwrap_or(""), "").to_lowercase(), "labels": labels, "due": due })
}

// The same fields of one issue from GitHub's API (GET /issues); None for a pull request, which that list also holds.
// The project is "owner/repo" from repository_url (…/repos/<owner>/<repo>), lower case like a remote's path.
pub fn slim_gh_issue(i: &Value) -> Option<Value> {
    if i.get("pull_request").is_some() { return None; }
    let project = i["repository_url"].as_str()?.split_once("/repos/")?.1.trim_matches('/').to_lowercase();
    let labels: Vec<Value> = i["labels"].as_array().map(|a| a.iter().take(10).filter_map(|l| l["name"].as_str().or(l.as_str()).map(|n| json!(n))).collect()).unwrap_or_default();
    Some(json!({ "iid": i["number"], "title": head16(i["title"].as_str().unwrap_or(""), 300), "url": i["html_url"],
        "project": project, "labels": labels, "due": Value::Null }))
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
// ponytail: first 100 only (one page), add paging when someone has more open issues than that. Same for GitHub.
pub fn fetch_issues(glab: &str, host: &str) -> Value {
    read_issues(Forge::GitLab, glab, &["api", "--hostname", host, "issues?scope=assigned_to_me&state=opened&per_page=100"], &[], |i| Some(slim_issue(i)), "GitLab")
}

fn read_issues(f: Forge, bin: &str, args: &[&str], env: &[(&str, &str)], slim: fn(&Value) -> Option<Value>, name: &str) -> Value {
    let r = procs::exec(bin, args, 30, env);
    if r.missing { return json!({ "ok": false, "reason": f.missing() }); }
    if !r.ok { return failed(&format!("{} {}", r.err, r.out)); }
    match serde_json::from_str::<Value>(&r.out) {
        Ok(Value::Array(a)) => json!({ "ok": true, "issues": a.iter().filter_map(slim).collect::<Vec<_>>() }),
        _ => json!({ "ok": false, "reason": "error", "detail": format!("unexpected answer from {name}") }),
    }
}

// A new Claude desktop session in `dir`, its prompt box filled with `prompt` (q, Claude's documented link: nothing is
// sent until you press Enter there). Claude cuts q at about 14 000 characters; Bobix stops a bit earlier.
pub fn new_session_link(dir: &str, prompt: &str) -> String {
    let prompt = head16(prompt.trim(), 12_000);
    let q = if prompt.is_empty() { String::new() } else { format!("&q={}", enc(prompt)) };
    format!("claude://code/new?folder={}{q}", enc(dir))
}

// An issue's description, read once (GET) when you press Start, for the prompt Claude gets; Bobix does not keep it.
// ponytail: cut at 3000 characters; the prompt box lets you add the rest.
pub fn fetch_body(f: Forge, bin: &str, host: &str, path: &str, iid: i64) -> Result<String, Value> {
    let (api, field) = match f {
        Forge::GitHub => (format!("repos/{path}/issues/{iid}"), "body"),
        Forge::GitLab => (format!("projects/{}/issues/{iid}", enc(path)), "description"),
    };
    let r = procs::exec(bin, &["api", "--hostname", host, &api], 30, f.env());
    if !r.ok { return Err(failed(&format!("{} {}", r.err, r.out))); }
    let v: Value = serde_json::from_str(&r.out).map_err(|_| json!({ "ok": false, "reason": "error", "detail": "unexpected answer" }))?;
    Ok(head16(v[field].as_str().unwrap_or("").trim(), 3000).to_string())
}

// A failed read: signed out (401, bad or missing token), else the CLI's own words, on one line.
fn failed(text: &str) -> Value {
    static AUTH: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b401\b|unauthori[sz]ed|not logged|no token|authenticat|bad credentials"));
    static WS: LazyLock<Regex> = LazyLock::new(|| re(r"\s+"));
    if AUTH.is_match(text).unwrap_or(false) { return json!({ "ok": false, "reason": "auth" }); }
    json!({ "ok": false, "reason": "error", "detail": head16(WS.replace_all(text, " ").trim(), 200) })
}

// ---------- the Work tab's state (the 0.1 Electron app's main.js) ----------
#[derive(Default)]
pub struct GitCache { pub at: u64, pub busy: bool, pub git: Option<Value> }

// One tracker's last read. issues: { status: off | loading | <missing> | <signed_out> | error | ok, hosts, at, failed, list,
// closed }; reviews: your open pull / merge requests, read with the issues.
pub struct Feed { pub bin: Option<String>, pub issues: Value, pub reviews: Vec<Review>, pub busy: bool }
impl Default for Feed { fn default() -> Self { Feed { bin: None, issues: json!({ "status": "off" }), reviews: vec![], busy: false } } }

pub struct Work {
    pub git_cache: HashMap<String, GitCache>, // folder -> its last status
    pub remotes_of: HashMap<String, Vec<String>>, // repository top folder -> remote URLs (never sent to the pages)
    pub feeds: [Feed; 2], // by Forge, in FORGES order
    pub claude_list: Vec<ClaudeSession>, // Claude desktop's own sessions, to find an issue's session by its name
    // New on an issue opens a new session in the project's folder and waits here: the first session created in exactly
    // that folder afterwards takes the issue and its name (see take_issue).
    pub new_for: indexmap::IndexMap<String, Value>, // project folder -> { path, iid, title }
}

impl Default for Work {
    fn default() -> Self {
        Work { git_cache: HashMap::new(), remotes_of: HashMap::new(), feeds: Default::default(), claude_list: vec![], new_for: Default::default() }
    }
}

// A project's code in session names: the one set in Settings, else the one made from its name.
pub fn key_of(settings: &Value, p: &str) -> String {
    settings["workKeys"][p].as_str().map(String::from).unwrap_or_else(|| key_for(p))
}

impl Work {
    pub fn feed(&mut self, f: Forge) -> &mut Feed { &mut self.feeds[f as usize] }
    fn on(&self, settings: &Value) -> Vec<&Feed> {
        FORGES.iter().filter(|f| f.on(settings)).map(|f| &self.feeds[*f as usize]).collect()
    }

    // The hosts of every tracker that is on: GitLab's first, then GitHub's.
    pub fn hosts(&self, settings: &Value) -> Vec<String> {
        self.on(settings).into_iter().flat_map(|f| f.issues["hosts"].as_array().into_iter().flatten().filter_map(|h| h.as_str().map(String::from))).collect()
    }

    fn list(&self, settings: &Value) -> Vec<&Value> {
        self.on(settings).into_iter().flat_map(|f| f.issues["list"].as_array().into_iter().flatten()).collect()
    }

    pub fn find_issue(&self, settings: &Value, path: &str, iid: i64) -> Option<&Value> {
        self.list(settings).into_iter().find(|i| i["project"] == path && i["iid"].as_i64() == Some(iid))
    }

    // Where to read an issue Bobix listed again: its tracker, that tracker's CLI, the signed-in host of its link.
    pub fn source_of(&self, settings: &Value, path: &str, iid: i64) -> Option<(Forge, String, String, Value)> {
        FORGES.into_iter().filter(|f| f.on(settings)).find_map(|f| {
            let feed = &self.feeds[f as usize];
            let i = feed.issues["list"].as_array()?.iter().find(|i| i["project"] == path && i["iid"].as_i64() == Some(iid))?;
            let url_host = i["url"].as_str().and_then(|u| tauri::Url::parse(u).ok())?.host_str()?.to_lowercase();
            let host = feed.issues["hosts"].as_array()?.iter().filter_map(|h| h.as_str()).find(|h| h.split(':').next() == Some(url_host.as_str()))?;
            Some((f, feed.bin.clone()?, host.to_string(), i.clone()))
        })
    }

    fn extra(&self, settings: &Value, key: &str) -> Vec<&Value> {
        self.on(settings).into_iter().flat_map(|f| f.issues[key].as_array().into_iter().flatten()).collect()
    }
    // Only links Bobix read: an issue, a review that closes one, an issue closed lately.
    pub fn open_issue_ok(&self, settings: &Value, url: &str) -> bool {
        self.list(settings).into_iter().chain(self.extra(settings, "closed")).any(|i| i["url"] == url)
            || self.on(settings).into_iter().flat_map(|f| &f.reviews).any(|r| r.url == url)
    }

    // The trackers that are on, as one: ok when one read, loading while one reads for the first time, else error.
    // problems: the missing / signed-out statuses, one per tracker; failed: the hosts whose read failed.
    fn status(&self, settings: &Value) -> Value {
        let on = self.on(settings);
        let has = |s: &str| on.iter().any(|f| f.issues["status"] == s);
        let status = if has("ok") { "ok" } else if has("loading") || has("off") { "loading" } else { "error" };
        let problems: Vec<&Value> = on.iter().map(|f| &f.issues["status"]).filter(|s| FORGES.iter().any(|x| *s == x.missing() || *s == x.signed_out())).collect();
        let failed: Vec<&Value> = on.iter().flat_map(|f| f.issues["failed"].as_array().into_iter().flatten()).collect();
        let at = on.iter().filter_map(|f| f.issues["at"].as_u64()).max();
        json!({ "status": status, "at": at, "problems": problems, "failed": failed })
    }

    // What the pages see: issues grouped by project, each with its stage and links from the work model.
    pub fn view(&self, settings: &Value, m: &Model) -> Value {
        if !any_on(settings) { return json!({ "status": "off" }); }
        let live: Vec<Live> = m.sessions.iter().filter_map(|s| Some(Live { id: s.id.clone(), path: s.repo.as_ref().map(|r| r.to.path.clone()),
            dir: s.dir.as_ref()?.to.path.clone(), iid: s.issue.as_ref().map(|i| i.to.iid) })).collect();
        let all: Vec<Value> = self.list(settings).into_iter().cloned().collect();
        let (mut local, mut away) = group_issues(&all, &live);
        for (g, here) in local.iter_mut().map(|g| (g, true)).chain(away.iter_mut().map(|g| (g, false))) {
            let p = g["path"].as_str().unwrap_or("").to_string();
            g["key"] = json!(key_of(settings, &p));
            g["autoKey"] = json!(key_for(&p));
            for i in g["issues"].as_array_mut().into_iter().flatten() {
                let mi = m.issue(&p, i["iid"].as_i64().unwrap_or(-1));
                if here && i["sessions"].as_array().is_some_and(|a| a.is_empty()) { i["resume"] = json!(mi.and_then(|x| x.resume.as_ref()).map(|r| &r.to.id)); }
                if let Some(r) = mi.and_then(|x| x.reviews.first()) { i["review"] = json!(r.to.url); } // an open PR / MR closes it
                i["stage"] = json!(mi.map_or("todo", |x| x.stage));
            }
        }
        let mut view = self.status(settings);
        view["hosts"] = json!(self.hosts(settings));
        view["local"] = json!(local);
        view["away"] = json!(away);
        view["closed"] = json!(self.extra(settings, "closed"));
        view
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Sessions;

    fn view(w: &Work, settings: &Value, sessions: &Sessions) -> Value { w.view(settings, &Model::build(settings, sessions, w)) }

    #[test]
    fn start_link_and_where_an_issue_is_read_again() {
        assert_eq!(new_session_link("/src/a b", ""), "claude://code/new?folder=%2Fsrc%2Fa%20b");
        assert_eq!(new_session_link("/src/app", " #54 Pick & go\n\nhttps://h/i/54?x=1 "), "claude://code/new?folder=%2Fsrc%2Fapp&q=%2354%20Pick%20%26%20go%0A%0Ahttps%3A%2F%2Fh%2Fi%2F54%3Fx%3D1");
        assert!(new_session_link("/d", &"é".repeat(20_000)).len() < 12_000 * 6 + 40, "cut before Claude's own limit");
        let mut w = Work::default();
        w.feeds[Forge::GitHub as usize] = Feed { bin: Some("/bin/gh".into()), busy: false, reviews: vec![], issues: json!({ "status": "ok", "hosts": ["github.com", "ghe.example.com:8443"],
            "list": [{ "iid": 54, "project": "acme/app", "title": "t", "url": "https://github.com/acme/app/issues/54" }, { "iid": 7, "project": "t/a", "title": "x", "url": "https://ghe.example.com:8443/t/a/issues/7" }] }) };
        let on = json!({ "github": true });
        let got = w.source_of(&on, "acme/app", 54).map(|(f, bin, host, i)| (f, bin, host, i["title"].clone()));
        assert_eq!(got, Some((Forge::GitHub, "/bin/gh".into(), "github.com".into(), json!("t"))));
        assert_eq!(w.source_of(&on, "t/a", 7).map(|x| x.2), Some("ghe.example.com:8443".into()), "the signed-in host, port and all");
        assert!(w.source_of(&on, "acme/app", 55).is_none(), "only issues Bobix listed");
        assert!(w.source_of(&json!({ "github": false }), "acme/app", 54).is_none(), "GitHub off");
    }

    #[test]
    fn glab_status_gives_signed_in_hosts() {
        let status = "gitlab.com\n  x gitlab.com: API call failed: GET https://gitlab.com/api/v4/user: 401\n  ✓ Git operations for gitlab.com configured to use ssh protocol.\n\
            git.example.com:8443\n  ✓ Logged in to git.example.com:8443 as someone (keyring)\n  ✓ Token: **************************\n\
            Gitlab.Example.org\n  ✓ Logged in to Gitlab.Example.org as other (config.yml)\n";
        assert_eq!(logged_in_hosts(status), ["git.example.com:8443", "gitlab.example.org"]);
        assert!(logged_in_hosts("X could not authenticate").is_empty());
    }

    #[test]
    fn gh_status_gives_hosts_whose_active_account_is_signed_in() {
        let two = "github.com\n  ✓ Logged in to github.com account first (keyring)\n  - Active account: true\n  - Git operations protocol: https\n  - Token: gho_************************************\n\n\
            \x20 ✓ Logged in to github.com account second (keyring)\n  - Active account: false\n  - Token: gho_****\n\n\
            GHE.Example.com\n  X Failed to log in to ghe.example.com account worker (default)\n  - Active account: true\n  - The token in default is invalid.\n\
            \x20 ✓ Logged in to ghe.example.com account other (keyring)\n  - Active account: false\n\
            ghe.two.org\n  X Timeout trying to log in to ghe.two.org account me (keyring)\n  - Active account: true\n";
        assert_eq!(gh_logged_in_hosts(two), ["github.com", "ghe.two.org"], "the active account counts; a failed one is signed out; a timeout is left to the read");
        let second_active = "github.com\n  ✓ Logged in to github.com account a (keyring)\n  - Active account: false\n  ✓ Logged in to github.com account b (keyring)\n  - Active account: true\n";
        assert_eq!(gh_logged_in_hosts(second_active), ["github.com"]);
        assert_eq!(gh_logged_in_hosts("github.com\n  ✓ Logged in to github.com as old (oauth_token)\n  ✓ Token: *******\n"), ["github.com"], "gh before 2.40");
        assert!(gh_logged_in_hosts("You are not logged into any GitHub hosts. To log in, run: gh auth login\n").is_empty());
    }

    #[test]
    fn gh_issues_leave_pull_requests_out() {
        let page: Value = serde_json::from_str(r#"[
            { "number": 53, "title": "feat: GitHub issues", "html_url": "https://github.com/Acme/Shop-App/issues/53",
              "repository_url": "https://api.github.com/repos/Acme/Shop-App", "labels": [{ "name": "enhancement", "color": "a2eeef" }],
              "milestone": { "due_on": "2026-12-01T08:00:00Z" }, "user": { "login": "x" } },
            { "number": 54, "title": "a PR", "html_url": "https://github.com/acme/shop-app/pull/54",
              "repository_url": "https://api.github.com/repos/acme/shop-app", "labels": [], "pull_request": { "url": "u" } },
            { "number": 7, "title": "on GHE", "html_url": "https://ghe.example.com/t/a/issues/7",
              "repository_url": "https://ghe.example.com/api/v3/repos/t/a", "labels": [] }
        ]"#).unwrap();
        let got: Vec<Value> = page.as_array().unwrap().iter().filter_map(slim_gh_issue).collect();
        assert_eq!(got, [
            json!({ "iid": 53, "title": "feat: GitHub issues", "url": "https://github.com/Acme/Shop-App/issues/53", "project": "acme/shop-app", "labels": ["enhancement"], "due": null }),
            json!({ "iid": 7, "title": "on GHE", "url": "https://ghe.example.com/t/a/issues/7", "project": "t/a", "labels": [], "due": null }),
        ]);
        assert_eq!(failed("gh: Bad credentials (HTTP 401)")["reason"], "auth");
        assert_eq!(failed("error connecting to api.github.com\ncheck your internet connection or https://githubstatus.com"),
            json!({ "ok": false, "reason": "error", "detail": "error connecting to api.github.com check your internet connection or https://githubstatus.com" }));
    }

    #[test]
    fn view_joins_gitlab_and_github() {
        let mut w = Work::default();
        w.feeds[Forge::GitLab as usize].issues = json!({ "status": "noglab", "at": 5 });
        w.feeds[Forge::GitHub as usize].issues = json!({ "status": "ok", "hosts": ["github.com"], "at": 9, "failed": [{ "host": "ghe.example.com", "detail": "x" }],
            "list": [{ "iid": 53, "project": "acme/shop-app", "title": "t", "labels": [], "due": null, "url": "https://github.com/acme/shop-app/issues/53" }] });
        let sessions = Sessions::default();
        let both = json!({ "gitlab": true, "github": true });
        let v = view(&w, &both, &sessions);
        assert_eq!((&v["status"], &v["problems"], &v["hosts"], &v["at"]), (&json!("ok"), &json!(["noglab"]), &json!(["github.com"]), &json!(9)));
        assert_eq!(v["failed"][0]["host"], "ghe.example.com");
        assert_eq!((&v["away"][0]["path"], &v["away"][0]["key"]), (&json!("acme/shop-app"), &json!("SHA")));
        assert!(w.open_issue_ok(&both, "https://github.com/acme/shop-app/issues/53"));
        let gh_only = json!({ "gitlab": false, "github": true });
        assert_eq!(view(&w, &gh_only, &sessions)["problems"], json!([]));
        let gl_only = json!({ "gitlab": true, "github": false });
        let v = view(&w, &gl_only, &sessions);
        assert_eq!((&v["status"], &v["away"]), (&json!("error"), &json!([])));
        assert!(!w.open_issue_ok(&gl_only, "https://github.com/acme/shop-app/issues/53"), "GitHub off: its links do not open");
        assert_eq!(view(&w, &json!({}), &sessions), json!({ "status": "off" }));
    }

    #[test]
    fn issues_count_the_sessions_on_them() {
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
    fn reviews_put_issues_in_review() {
        let pr: Value = serde_json::from_str(r#"{ "number": 70, "url": "https://github.com/Acme/Shop/pull/70", "headRefName": "feat/52-board-day",
            "body": "Closes #60\nCloses #52", "repository": { "nameWithOwner": "Acme/Shop" } }"#).unwrap();
        // in the view: the issue it closes carries the review's link, which may be opened; the closed list goes along
        let mut w = Work::default();
        w.feeds[Forge::GitHub as usize].issues = json!({ "status": "ok", "hosts": ["github.com"], "at": 1,
            "list": [{ "iid": 52, "project": "acme/shop", "title": "t", "labels": [], "due": null, "url": "https://github.com/acme/shop/issues/52" },
                     { "iid": 53, "project": "acme/shop", "title": "u", "labels": [], "due": null, "url": "https://github.com/acme/shop/issues/53" }],
            "closed": [{ "iid": 50, "project": "acme/shop", "title": "v", "url": "https://github.com/acme/shop/issues/50", "closedAt": "2026-11-08T03:00:00Z" }] });
        w.feeds[Forge::GitHub as usize].reviews = vec![model::gh_pr(&pr).unwrap()];
        let on = json!({ "github": true });
        let v = view(&w, &on, &Sessions::default());
        let issue = |n: i64| v["away"][0]["issues"].as_array().unwrap().iter().find(|i| i["iid"] == n).unwrap().clone();
        assert_eq!((&issue(52)["review"], issue(53).get("review")), (&json!("https://github.com/Acme/Shop/pull/70"), None));
        assert_eq!((&issue(52)["stage"], &issue(53)["stage"]), (&json!("verify"), &json!("todo")));
        assert_eq!(v["closed"][0]["iid"], 50);
        assert!(w.open_issue_ok(&on, "https://github.com/Acme/Shop/pull/70") && w.open_issue_ok(&on, "https://github.com/acme/shop/issues/50"));
        assert!(!w.open_issue_ok(&on, "https://evil.example/pull/1"));
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

    // Against the GitHub(s) the person's own gh is signed in to (read-only GET); prints counts only.
    // cargo test live_gh -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_gh() {
        let gh = crate::find_bin("gh").expect("gh on PATH");
        let hosts = Forge::GitHub.hosts(&gh).expect("gh runs");
        println!("hosts: {}", hosts.len());
        for (n, h) in hosts.iter().enumerate() {
            let r = Forge::GitHub.fetch(&gh, h);
            println!("host {n}: ok {} reason {} issues {}", r["ok"], r["reason"], r["issues"].as_array().map_or(0, |a| a.len()));
            for i in r["issues"].as_array().into_iter().flatten() { assert!(i["iid"].is_i64() && i["url"].as_str().is_some_and(|u| u.contains("/issues/")) && i["project"].as_str().is_some_and(|p| p.contains('/'))); }
        }
    }
}
