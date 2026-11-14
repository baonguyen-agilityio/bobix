// Work items: a session's local git status (no network), and the open GitLab / GitHub issues of the repositories you
// ticked, read through your own `glab` / `gh` (Bobix keeps no token, and writes nothing to either from here; the
// ticket flow's Open MR / PR, on your click, is in tickets.rs). Commands get argument lists, never a shell.
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
    let own = git(dir, &["rev-list", "--count", "HEAD", "--not", "refs/remotes/origin/HEAD"]); // commits not on the remote's default branch
    let remotes = git(dir, &["remote", "-v"]);
    let mut out = parse_status(&st.out);
    let log_out = log.out.trim();
    let (t, subject) = log_out.split_once('\0').unwrap_or((log_out, ""));
    let top = top.out.trim();
    out["top"] = json!(if top.is_empty() { dir } else { top });
    out["unpushed"] = json!(if unpushed.ok { unpushed.out.trim().parse::<u64>().unwrap_or(0) } else { 0 });
    out["last"] = match t.parse::<u64>() { Ok(t) => json!({ "t": t * 1000, "subject": head16(subject, 200) }), _ => Value::Null };
    out["worktrees"] = json!(wt.out.lines().filter(|l| l.starts_with("worktree ")).count());
    out["own"] = if own.ok { json!(own.out.trim().parse::<u64>().unwrap_or(0)) } else { Value::Null }; // none: no origin/HEAD here
    let mut urls: Vec<String> = Vec::new();
    for u in remotes.out.lines().filter_map(|l| l.split_whitespace().nth(1)).map(crate::clones::unalias) { if !urls.contains(&u) { urls.push(u); } }
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
// The hosts a CLI's own config lists, by name only: the keys under glab's `hosts:` (config.yml), or gh's top-level keys
// (hosts.yml). Tokens live in those files: nothing but the key names is kept.
pub fn config_hosts(yaml: &str, glab: bool) -> Vec<String> {
    let (mut out, mut inside, mut ind): (Vec<String>, bool, Option<usize>) = (vec![], !glab, (!glab).then_some(0));
    for l in yaml.lines() {
        let t = l.trim_start();
        if t.is_empty() || t.starts_with('#') { continue; }
        let n = l.len() - t.len();
        if glab && n == 0 { inside = t.trim_end() == "hosts:"; ind = None; continue; }
        let Some(k) = t.trim_end().strip_suffix(':').filter(|_| inside) else { continue }; // a host is a key with a map under it
        if *ind.get_or_insert(n) != n { continue; }
        let h = k.trim_matches(|c| c == '"' || c == '\'').to_lowercase();
        if !h.is_empty() && !out.contains(&h) { out.push(h); }
    }
    out
}
// Where glab / gh keep that file: their own variable, XDG, ~/.config, and the system's config folder (newer glab on macOS
// and Windows, gh on Windows). Hosts from every file that exists, once each.
fn listed_hosts(f: Forge) -> Vec<String> {
    use std::path::PathBuf;
    let (var, dir, file) = match f { Forge::GitLab => ("GLAB_CONFIG_DIR", "glab-cli", "config.yml"), Forge::GitHub => ("GH_CONFIG_DIR", "gh", "hosts.yml") };
    let home = crate::paths::home();
    let mut at: Vec<PathBuf> = std::env::var_os(var).map(|d| PathBuf::from(d).join(file)).into_iter().collect();
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") { at.push(PathBuf::from(x).join(dir).join(file)); }
    at.push(home.join(".config").join(dir).join(file));
    if cfg!(target_os = "macos") { at.push(home.join("Library/Application Support").join(dir).join(file)); }
    if let Some(a) = std::env::var_os("APPDATA").filter(|_| cfg!(windows)) { at.push(PathBuf::from(a).join(if f == Forge::GitHub { "GitHub CLI" } else { dir }).join(file)); }
    let mut out: Vec<String> = Vec::new();
    for p in at { for h in std::fs::read_to_string(p).map(|y| config_hosts(&y, f == Forge::GitLab)).unwrap_or_default() { if !out.contains(&h) { out.push(h); } } }
    out
}
// How long one host's sign-in check may take. `glab auth status` on its own checks every host in turn, so one host that
// does not answer (gitlab.com listed with no token: about 31 s) used to hide the others.
pub const HOST_SECS: u64 = 10;
pub const PAR_READS: usize = 8; // GitLab projects read at the same time, at most
// Each host on its own, all at once: `<cli> auth status --hostname <host>`. The hosts signed in, and the others with why
// (named in the Work tab, ignored for the read), and the account each one signed in reads with. None when the CLI is missing.
pub fn check_hosts(f: Forge, bin: &str, hosts: &[String], secs: u64) -> Option<(Vec<String>, Vec<Value>, HashMap<String, String>)> {
    let rs: Vec<(procs::Ran, bool)> = std::thread::scope(|s| {
        let js: Vec<_> = hosts.iter().map(|h| s.spawn(move || {
            let t = std::time::Instant::now();
            let r = procs::exec(bin, &["auth", "status", "--hostname", h], secs, f.env());
            let late = !r.ok && t.elapsed().as_secs() >= secs;
            (r, late)
        })).collect();
        js.into_iter().map(|j| j.join().unwrap()).collect()
    });
    if rs.iter().any(|(r, _)| r.missing) { return None; }
    let (mut ok, mut skipped, mut who) = (vec![], vec![], HashMap::new());
    for (h, (r, late)) in hosts.iter().zip(rs) {
        let text = format!("{}\n{}", r.out, r.err);
        let signed = if f == Forge::GitLab { logged_in_hosts(&text) } else { gh_logged_in_hosts(&text) };
        if signed.contains(h) { ok.push(h.clone()); if let Some(n) = logins(&text).remove(h) { who.insert(h.clone(), n); } }
        else { skipped.push(json!({ "host": h, "detail": if late { format!("no answer in {secs} s, ignored") } else { "not signed in, ignored".into() } })) }
    }
    Some((ok, skipped, who))
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
// The account each signed-in host reads with: "Logged in to <host> as <name>" (glab, gh before 2.40), or "Logged in
// to <host> account <name>" followed by "- Active account: true" (gh). To mark the issues assigned to you.
pub fn logins(text: &str) -> HashMap<String, String> {
    static L: LazyLock<Regex> = LazyLock::new(|| re(r"Logged in to (\S+) (account|as) (\S+)"));
    let (mut out, mut last) = (HashMap::new(), None);
    for l in text.lines() {
        if let Ok(Some(m)) = L.captures(l) {
            let (h, n) = (m[1].to_lowercase(), m[3].to_string());
            if &m[2] == "as" { out.entry(h).or_insert(n); last = None; } else { last = Some((h, n)); }
        } else if l.contains("Active account: true") {
            if let Some((h, n)) = last.take() { out.insert(h, n); }
        } else if l.contains("Active account: false") { last = None; }
    }
    out
}

// Issues per page of the Work list, and how far back a repository's activity pre-ticks it in the repo picker.
pub const PER_PAGE: usize = 20;
pub const RECENT_DAYS: u64 = 90;
pub const MAX_PAGE: usize = 50; // the list pages that can be shown: a read never asks for more than 50 × 20 + 1 issues
// How many of the newest issues a read takes for page `page`: up to it, and one more to know whether a next exists.
pub fn need_for(page: usize) -> usize { page.clamp(1, MAX_PAGE) * PER_PAGE + 1 }
pub const MANY_GITLAB: usize = 20; // GitLab projects ticked past this: each is one more read every 5 minutes, so Work warns

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
    // None when the CLI is missing, else the hosts it is signed in to (none when nowhere), the listed hosts it is not
    // (with why), and the account each host reads with (`logins`, to mark the issues assigned to you). Each host its config
    // lists is checked on its own; with none listed (a token from the environment, say), one `auth status` for all. Both
    // exit non-zero when any host fails, and older gh writes this to stderr.
    pub fn auth(self, bin: &str) -> Option<(Vec<String>, Vec<Value>, HashMap<String, String>)> {
        let listed = listed_hosts(self);
        if !listed.is_empty() { return check_hosts(self, bin, &listed, HOST_SECS); }
        let r = procs::exec(bin, &["auth", "status"], 30, self.env());
        if r.missing { return None; }
        let text = format!("{}\n{}", r.out, r.err);
        Some((if self == Forge::GitLab { logged_in_hosts(&text) } else { gh_logged_in_hosts(&text) }, vec![], logins(&text)))
    }
    pub fn hosts(self, bin: &str) -> Option<(Vec<String>, Vec<Value>)> { self.auth(bin).map(|(h, s, _)| (h, s)) }
    // Every repository (GitHub) / project (GitLab) you can reach on `host`, most recent activity first, for the repo
    // picker: [{ path, at }] (GET; ponytail: 5 pages of 100, add more when someone reaches over 500).
    pub fn repos(self, bin: &str, host: &str) -> Result<Vec<Value>, Value> {
        let mut out = vec![];
        for page in 1..=5 {
            let api = match self {
                Forge::GitHub => format!("user/repos?per_page=100&sort=pushed&page={page}"),
                Forge::GitLab => format!("projects?membership=true&simple=true&order_by=last_activity_at&per_page=100&page={page}"),
            };
            let r = procs::exec(bin, &["api", "--hostname", host, &api], 30, self.env());
            let a = match serde_json::from_str::<Value>(&r.out) { Ok(Value::Array(a)) if r.ok => a, _ if page > 1 => break, _ if !r.ok => return Err(failed(&format!("{} {}", r.err, r.out))), _ => return Err(json!({ "ok": false, "reason": "error", "detail": "unexpected answer" })) };
            out.extend(a.iter().filter_map(|x| {
                let (p, at) = match self { Forge::GitHub => (x["full_name"].as_str()?, x["pushed_at"].as_str().or(x["updated_at"].as_str())), Forge::GitLab => (x["path_with_namespace"].as_str()?, x["last_activity_at"].as_str()) };
                Some(json!({ "path": p.to_lowercase(), "at": at }))
            }));
            if a.len() < 100 { break; }
        }
        Ok(out)
    }
    // The newest `need` open issues of the ticked `repos` on `host`, most recently updated first (GET): GitHub, one
    // search per 30 repositories (ponytail: GitHub's query length limit; the chunks are merged like GitLab's projects);
    // GitLab, each project's own list. `filter`: all | mine | unassigned. Each issue gets `mine` (assigned to `me`).
    // { ok, issues, failed: [{ host, detail }] } (a GitLab project that fails is left out and named) or a failed read.
    // GitLab projects are read at the same time (PAR_READS at most), each for HOST_SECS at most: the first one that
    // does not answer in time marks the host as hanging, its other projects are not asked, and the host is named once
    // (`hung`: its reviews and closed issues are not asked either). A project that fails otherwise (404, 403) is named
    // on its own and the others still show. GitHub: one search per 30 repositories, as before, each in HOST_SECS.
    pub fn page(self, bin: &str, host: &str, repos: &[String], filter: &str, need: usize, me: Option<&str>) -> Value {
        let per = need.clamp(1, 100);
        let late = || json!({ "ok": false, "reason": "timeout", "detail": format!("no answer in {HOST_SECS} s, ignored") });
        // the pages a list needs, `args(page)` each, until one comes back short
        let get = |args: &dyn Fn(usize) -> Vec<String>, items: fn(&Value) -> Option<&Vec<Value>>| -> Result<Vec<Value>, Value> {
            let mut out = vec![];
            for page in 1..=need.div_ceil(100) {
                let a = args(page);
                let t = std::time::Instant::now();
                let r = procs::exec(bin, &a.iter().map(String::as_str).collect::<Vec<_>>(), HOST_SECS, self.env());
                if !r.ok && t.elapsed().as_secs() >= HOST_SECS { return Err(late()); }
                if !r.ok { return Err(failed(&format!("{} {}", r.err, r.out))); }
                let v: Value = serde_json::from_str(&r.out).map_err(|_| json!({ "ok": false, "reason": "error", "detail": "unexpected answer" }))?;
                let list = items(&v).cloned().unwrap_or_default();
                let n = list.len();
                out.extend(list);
                if n < per { break; }
            }
            Ok(out)
        };
        let (mut issues, mut bad, mut hung) = (vec![], vec![], false);
        match self {
            Forge::GitHub => {
                let f = match filter { "mine" => " assignee:@me", "unassigned" => " no:assignee", _ => "" };
                for chunk in repos.chunks(30) {
                    let q = format!("is:issue is:open{f}{}", chunk.iter().map(|r| format!(" repo:{r}")).collect::<String>());
                    let args = |page: usize| ["api", "--hostname", host, "-X", "GET", "search/issues", "-f", &format!("q={q}"), "-f", "sort=updated", "-f", "order=desc",
                        "-f", &format!("per_page={per}"), "-f", &format!("page={page}")].iter().map(|x| x.to_string()).collect();
                    match get(&args, |v| v["items"].as_array()) {
                        Ok(a) => issues.extend(a.iter().filter_map(slim_gh_issue)),
                        Err(mut e) => { if e["reason"] == "timeout" { e["hung"] = json!(true); } return e; }
                    }
                }
            }
            Forge::GitLab => {
                let f = match filter { "mine" => "&scope=assigned_to_me", "unassigned" => "&assignee_id=None", _ => "" };
                let stuck = std::sync::atomic::AtomicBool::new(false);
                let next = std::sync::atomic::AtomicUsize::new(0);
                let done: std::sync::Mutex<Vec<Option<Result<Vec<Value>, Value>>>> = std::sync::Mutex::new(vec![None; repos.len()]);
                std::thread::scope(|s| for _ in 0..PAR_READS.min(repos.len()) {
                    s.spawn(|| loop {
                        let n = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        let Some(p) = repos.get(n) else { break };
                        if stuck.load(std::sync::atomic::Ordering::SeqCst) { continue; } // the host hangs: not asked
                        let args = |page: usize| vec!["api".into(), "--hostname".into(), host.into(),
                            format!("projects/{}/issues?state=opened&order_by=updated_at&sort=desc&per_page={per}&page={page}{f}", enc(p))];
                        let r = get(&args, |v| v.as_array());
                        if r.as_ref().is_err_and(|e| e["reason"] == "timeout") { stuck.store(true, std::sync::atomic::Ordering::SeqCst); }
                        done.lock().unwrap()[n] = Some(r);
                    });
                });
                hung = stuck.into_inner();
                let mut answered = false;
                for (p, r) in repos.iter().zip(done.into_inner().unwrap()) {
                    match r {
                        Some(Ok(a)) => { answered = true; issues.extend(a.iter().map(slim_issue)); }
                        Some(Err(e)) if e["reason"] == "auth" => return e,
                        Some(Err(e)) if e["reason"] == "timeout" => {}
                        Some(Err(e)) => bad.push(json!({ "host": host, "detail": format!("{p}: {}", e["detail"].as_str().unwrap_or("")) })),
                        None => {} // not asked: the host hangs
                    }
                }
                if hung {
                    if !answered && bad.is_empty() { let mut e = late(); e["hung"] = json!(true); return e; }
                    bad.push(json!({ "host": host, "detail": late()["detail"] }));
                }
            }
        }
        for i in issues.iter_mut() { i["mine"] = json!(me.is_some_and(|me| i["assignees"].as_array().is_some_and(|a| a.iter().any(|x| x.as_str().is_some_and(|x| x.eq_ignore_ascii_case(me)))))); }
        newest_first(&mut issues);
        issues.truncate(need);
        json!({ "ok": true, "issues": issues, "failed": bad, "hung": hung })
    }
    fn env(self) -> &'static [(&'static str, &'static str)] { if self == Forge::GitHub { GH_ENV } else { &[] } }
    // One open issue, read on its own (GET), marked `linked`: one a session here works on (its branch or its start
    // names it) that is not assigned to you, so the session shows under its ticket. None when closed, a pull request,
    // or the read fails.
    pub fn issue(self, bin: &str, host: &str, path: &str, iid: i64) -> Option<Value> {
        let api = match self { Forge::GitHub => format!("repos/{path}/issues/{iid}"), Forge::GitLab => format!("projects/{}/issues/{iid}", enc(path)) };
        let r = procs::exec(bin, &["api", "--hostname", host, &api], 30, self.env());
        linked_issue(self, &serde_json::from_str(&r.out).ok()?)
    }
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

// Your open pull requests on a GitHub host, with the issues GitHub links to each (closing references), when each was opened and last changed, and its reviews.
const GH_PRS: &str = "query{viewer{pullRequests(states:OPEN,first:100,orderBy:{field:UPDATED_AT,direction:DESC}){nodes{number url headRefName body createdAt updatedAt reviewDecision reviews{totalCount} repository{nameWithOwner} closingIssuesReferences(first:20){nodes{number repository{nameWithOwner}}}}}}}";

// gh never asks anything (no terminal) and does not look for a new version of itself.
const GH_ENV: &[(&str, &str)] = &[("GH_PROMPT_DISABLED", "1"), ("GH_NO_UPDATE_NOTIFIER", "1"), ("NO_COLOR", "1")];

// The forge's page for a new pull request (GitHub) / merge request (GitLab) from `branch`, for a project and a branch
// the work model knows; the person fills it in and opens it there. Bobix writes nothing.
pub fn new_review_url(m: &Model, project: &str, branch: &str) -> Option<String> {
    let r = m.repos.iter().find(|r| r.path == project && !r.host.is_empty())?;
    if !m.branches.iter().any(|b| b.project == project && b.name == branch) { return None; }
    Some(if r.forge == Forge::GitHub.key() { format!("https://{}/{}/compare/{}?expand=1", r.host, r.path, enc(branch).replace("%2F", "/")) }
        else { format!("https://{}/{}/-/merge_requests/new?merge_request%5Bsource_branch%5D={}", r.host, r.path, enc(branch)) })
}

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

// Most recently updated first (ISO times compare as text; ponytail: GitLab's milliseconds sort a hair before GitHub's
// whole seconds).
pub fn newest_first(list: &mut [Value]) {
    list.sort_by(|a, b| b["updated"].as_str().unwrap_or("").cmp(a["updated"].as_str().unwrap_or("")));
}

// The first ticks of the repo picker: the FIRST_TICKS most recently active repositories since `since` (ISO time), and
// how many more were active since then (a big organisation: hundreds; each ticked one is read every 5 minutes).
pub const FIRST_TICKS: usize = 10;
pub fn recent(repos: &[Value], since: &str) -> (Vec<String>, usize) {
    let mut r: Vec<(&str, &str)> = repos.iter().filter_map(|r| Some((r["path"].as_str()?, r["at"].as_str().filter(|t| *t >= since)?))).collect();
    r.sort_by(|a, b| b.1.cmp(a.1));
    let more = r.len().saturating_sub(FIRST_TICKS);
    (r.into_iter().take(FIRST_TICKS).map(|(p, _)| p.to_string()).collect(), more)
}

// The fields Bobix shows of one issue from GitLab's API.
pub fn slim_issue(i: &Value) -> Value {
    static REF: LazyLock<Regex> = LazyLock::new(|| re(r"#\d+$"));
    let labels: Vec<Value> = i["labels"].as_array().map(|a| a.iter().take(10).map(|l| json!(l.as_str().map(String::from).unwrap_or_else(|| l.to_string()))).collect()).unwrap_or_default();
    let due = i["due_date"].as_str().filter(|d| !d.is_empty()).map(|d| json!(d)).unwrap_or(Value::Null);
    let who: Vec<Value> = i["assignees"].as_array().into_iter().flatten().filter_map(|a| a["username"].as_str()).take(3).map(|x| json!(x)).collect();
    json!({ "iid": i["iid"], "title": head16(i["title"].as_str().unwrap_or(""), 300), "url": i["web_url"],
        "project": REF.replace(i["references"]["full"].as_str().unwrap_or(""), "").to_lowercase(), "labels": labels, "due": due, "updated": i["updated_at"], "assignees": who })
}

// The same fields of one issue from GitHub's API (GET /issues); None for a pull request, which that list also holds.
// The project is "owner/repo" from repository_url (…/repos/<owner>/<repo>), lower case like a remote's path.
pub fn slim_gh_issue(i: &Value) -> Option<Value> {
    if i.get("pull_request").is_some() { return None; }
    let project = i["repository_url"].as_str()?.split_once("/repos/")?.1.trim_matches('/').to_lowercase();
    let labels: Vec<Value> = i["labels"].as_array().map(|a| a.iter().take(10).filter_map(|l| l["name"].as_str().or(l.as_str()).map(|n| json!(n))).collect()).unwrap_or_default();
    let who: Vec<Value> = i["assignees"].as_array().into_iter().flatten().filter_map(|a| a["login"].as_str()).take(3).map(|x| json!(x)).collect();
    Some(json!({ "iid": i["number"], "title": head16(i["title"].as_str().unwrap_or(""), 300), "url": i["html_url"],
        "project": project, "labels": labels, "due": Value::Null, "updated": i["updated_at"], "assignees": who }))
}

// An issue read on its own (Forge::issue): kept while open, marked `linked`.
fn linked_issue(f: Forge, v: &Value) -> Option<Value> {
    if !matches!(v["state"].as_str(), Some("open" | "opened")) { return None; }
    let mut i = if f == Forge::GitHub { slim_gh_issue(v)? } else { slim_issue(v) };
    i["linked"] = json!(true);
    Some(i)
}

// One session as the Work tab sees it: { id, path (its repository's project), dir, iid and of (the issue it works on:
// its number, and its project, which is another one than `path` when the issue's code is in this repository) }.
pub struct Live { pub id: String, pub path: Option<String>, pub dir: String, pub iid: Option<i64>, pub of: Option<String> }

// Issues by project, local repositories first (a session of theirs runs here), the rest "not on this machine". Each
// issue gets `sessions`: the ids working on it. In progress first, then by due date, then newest.
// `known`: the local clones Bobix remembers (clones.rs), for projects no session works in here.
pub fn group_issues(issues: &[Value], live: &[Live], known: &HashMap<String, String>) -> (Vec<Value>, Vec<Value>) {
    let mut dir_of: HashMap<&str, &str> = known.iter().map(|(p, d)| (p.as_str(), d.as_str())).collect();
    dir_of.extend(live.iter().filter_map(|r| Some((r.path.as_deref()?, r.dir.as_str()))));
    let mut groups: indexmap::IndexMap<String, Vec<Value>> = Default::default();
    for i in issues {
        let p = i["project"].as_str().unwrap_or("").to_string();
        let working: Vec<&str> = live.iter().filter(|s| s.of.as_deref() == Some(&p) && s.iid.is_some() && s.iid == i["iid"].as_i64()).map(|s| s.id.as_str()).collect();
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

// A folder you chose or cloned (Choose folder…, Change folder…, Clone…) wins over the folder a session works in: Start
// works there. A found one does not (a session's folder is the surer guess).
fn pin_chosen(local: &mut [Value], settings: &Value, known: &HashMap<String, String>) {
    for g in local {
        let p = g["path"].as_str().unwrap_or("").to_string();
        let how = &settings["workDirs"][&p]["how"];
        if let (Some("chosen" | "cloned"), Some(d)) = (how.as_str(), known.get(&p)) { g["dir"] = json!(d); g["via"] = how.clone(); }
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
// closed }, list holding the newest issues of the ticked repositories up to the page shown (and one more, to know
// whether a next page exists); reviews: your open pull / merge requests, read with the issues; repos: per host, the
// repositories you can reach (the repo picker's list, read when needed).
pub struct Feed { pub bin: Option<String>, pub issues: Value, pub reviews: Vec<Review>, pub busy: bool, pub repos: HashMap<String, Vec<Value>> }
impl Default for Feed { fn default() -> Self { Feed { bin: None, issues: json!({ "status": "off" }), reviews: vec![], busy: false, repos: HashMap::new() } } }

pub struct Work {
    pub git_cache: HashMap<String, GitCache>, // folder -> its last status
    pub remotes_of: HashMap<String, Vec<String>>, // repository top folder -> remote URLs (never sent to the pages)
    pub feeds: [Feed; 2], // by Forge, in FORGES order
    pub claude_list: Vec<ClaudeSession>, // Claude desktop's own sessions, to find an issue's session by its name
    // A start (Quick, Full in Claude, the terminal command) opens a new session and waits here; the session that takes
    // it gets the issue and its name (see Work::claim). One wait per issue and folder: several can wait at once.
    pub new_for: indexmap::IndexMap<String, Value>, // "<path>#<iid>@<folder>" -> { dir, repo, path, iid, title, url }
    // Sessions that started where a start waits and have not sent their first prompt yet: only that prompt may claim it.
    pub fresh: std::collections::HashSet<String>,
    // "project#iid" of the issues sessions worked on that were read on their own (Forge::issue), so each is asked once
    pub asked: std::collections::HashSet<String>,
    pub page: usize, // the page of the issue list shown, from 1
    // A read you asked for (filter, page, ticked repos, Refresh) bumps `gen`; a tracker's read keeps its answer only when
    // no newer one was asked meanwhile (`read_gen`: the ask its kept answer is for). Older answers are dropped.
    pub gen: u64,
    pub read_gen: [u64; 2],
    // The clone search ran for this tracker (by Forge): once per app start, and again after Refresh.
    pub scanned: [bool; 2],
    // A host's first read ticked only its FIRST_TICKS most recent repositories: how many more were active lately (memory
    // only; the repo picker says so until you save your ticks).
    pub more_repos: HashMap<String, usize>,
}

impl Default for Work {
    fn default() -> Self {
        Work { git_cache: HashMap::new(), remotes_of: HashMap::new(), feeds: Default::default(), claude_list: vec![], new_for: Default::default(), fresh: Default::default(), asked: Default::default(), page: 1, gen: 0, read_gen: [0; 2], scanned: [false; 2], more_repos: HashMap::new() }
    }
}

// Where a waiting start's session may come up. Claude desktop does not open a session in a folder inside a repository's
// worktree as given: it starts in the repository's main folder, or in a worktree of its own under .claude/worktrees.
// So: the folder the start opened (or one inside it), the main folder of its repository, or a Claude worktree there.
fn waits_here(w: &Value, cwd: &str) -> bool {
    use crate::paths::{dir_key, same_dir};
    let under = |base: &str, d: &str| !base.is_empty() && dir_key(d).starts_with(&(dir_key(base) + "/"));
    let (dir, repo) = (w["dir"].as_str().unwrap_or(""), w["repo"].as_str().unwrap_or(""));
    same_dir(cwd, dir) || under(dir, cwd) || (!repo.is_empty() && (same_dir(cwd, repo) || under(&format!("{repo}/.claude/worktrees"), cwd)))
}
// The prompt Bobix gave the session names this issue: its link, or "#<iid> <title>" (both lead the prompt it builds).
fn names_it(w: &Value, prompt: &str) -> bool {
    let url = w["url"].as_str().unwrap_or("");
    let head: String = w["title"].as_str().unwrap_or("").trim().chars().take(40).collect();
    (!url.is_empty() && prompt.contains(url)) || (!head.is_empty() && prompt.contains(&format!("#{} {head}", w["iid"].as_i64().unwrap_or(0))))
}

impl Work {
    // A start waits for its session: in `dir` (the folder Claude was asked to open), whose repository's main folder is
    // `repo`. The same issue started again in the same folder replaces its wait.
    pub fn wait_for(&mut self, dir: &str, repo: &str, path: &str, iid: i64, title: &Value, url: &Value) {
        self.new_for.insert(format!("{path}#{iid}@{}", crate::paths::dir_key(dir)), json!({ "dir": dir, "repo": repo, "path": path, "iid": iid, "title": title, "url": url }));
    }
    // A start waits where a session in `cwd` may be its own (the session then waits for its first prompt).
    pub fn waits_at(&self, cwd: &str) -> bool { self.new_for.values().any(|w| waits_here(w, cwd)) }
    // The wait a new session takes: one where it came up, whose issue its first prompt names. A session nothing names
    // (one you opened yourself there, with your own prompt) takes none. The oldest wait first.
    pub fn claim(&mut self, cwd: &str, prompt: &str) -> Option<Value> {
        let k = self.new_for.iter().find(|(_, w)| waits_here(w, cwd) && names_it(w, prompt)).map(|(k, _)| k.clone())?;
        self.new_for.shift_remove(&k)
    }
    // Stop waiting in `dir`: for one issue, or for every start there.
    pub fn cancel(&mut self, dir: &str, iid: Option<i64>) -> bool {
        let n = self.new_for.len();
        self.new_for.retain(|_, w| !(crate::paths::same_dir(w["dir"].as_str().unwrap_or(""), dir) && iid.is_none_or(|i| w["iid"].as_i64() == Some(i))));
        self.new_for.len() != n
    }
}

// A project's code in session names: the one set in Settings, else the one made from its name.
pub fn key_of(settings: &Value, p: &str) -> String {
    settings["workKeys"][p].as_str().map(String::from).unwrap_or_else(|| key_for(p))
}

impl Work {
    pub fn feed(&mut self, f: Forge) -> &mut Feed { &mut self.feeds[f as usize] }
    // A read you asked for has not answered yet (the panel shows "Reading your issues…" in place of the list).
    pub fn reading(&self, settings: &Value) -> bool { FORGES.into_iter().any(|f| f.on(settings) && self.read_gen[f as usize] < self.gen) }
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

    // A session works on an issue Bobix has not read and has not asked for yet (a new session on an issue not assigned to you).
    pub fn unread_link(&self, settings: &Value, m: &Model) -> bool {
        m.linked().iter().any(|(_, p, iid)| self.find_issue(settings, p, *iid).is_none() && !self.asked.contains(&format!("{p}#{iid}")))
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
        // both trackers are on by default: a CLI that is not installed is a problem only while nothing reads
        let problems: Vec<&Value> = on.iter().map(|f| &f.issues["status"]).filter(|s| FORGES.iter().any(|x| *s == x.signed_out() || (*s == x.missing() && !has("ok")))).collect();
        let failed: Vec<&Value> = on.iter().flat_map(|f| f.issues["failed"].as_array().into_iter().flatten()).collect();
        let at = on.iter().filter_map(|f| f.issues["at"].as_u64()).max();
        json!({ "status": status, "at": at, "problems": problems, "failed": failed })
    }

    // What the pages see: issues grouped by project, each with its stage and links from the work model.
    pub fn view(&self, settings: &Value, m: &Model) -> Value {
        if !any_on(settings) { return json!({ "status": "off" }); }
        let live: Vec<Live> = m.sessions.iter().filter_map(|s| Some(Live { id: s.id.clone(), path: s.repo.as_ref().map(|r| r.to.path.clone()),
            dir: s.dir.as_ref()?.to.path.clone(), iid: s.issue.as_ref().map(|i| i.to.iid), of: s.issue.as_ref().map(|i| i.to.project.clone()) })).collect();
        let works_on = |i: &Value| live.iter().any(|s| s.of.as_deref() == i["project"].as_str() && s.iid.is_some() && s.iid == i["iid"].as_i64());
        // one page of the ticked repositories' issues, newest first; an issue a session here works on stays on any page;
        // one read only because a session worked on it (linked) leaves with its last session
        let mut listed: Vec<&Value> = self.list(settings).into_iter().filter(|i| i["linked"] != true).collect();
        listed.sort_by(|a, b| b["updated"].as_str().unwrap_or("").cmp(a["updated"].as_str().unwrap_or("")));
        let page = self.page.max(1);
        let more = listed.len() > page * PER_PAGE;
        let shown: Vec<&Value> = listed.into_iter().skip((page - 1) * PER_PAGE).take(PER_PAGE).collect();
        let all: Vec<Value> = self.list(settings).into_iter().filter(|i| if i["linked"] == true { works_on(i) } else { shown.contains(i) || works_on(i) }).cloned().collect();
        let known = crate::clones::dirs(settings);
        let (mut local, mut away) = group_issues(&all, &live, &known);
        pin_chosen(&mut local, settings, &known);
        for (g, here) in local.iter_mut().map(|g| (g, true)).chain(away.iter_mut().map(|g| (g, false))) {
            let p = g["path"].as_str().unwrap_or("").to_string();
            g["key"] = json!(key_of(settings, &p));
            if here && !live.iter().any(|s| s.path.as_deref() == Some(p.as_str())) { g["via"] = settings["workDirs"][&p]["how"].clone(); } // no session here yet
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
        view["page"] = json!(page);
        view["more"] = json!(more);
        // a read made for an earlier page: this page shows once the read for it is in
        view["filling"] = json!(self.on(settings).into_iter().any(|f| f.issues["need"].as_u64().is_some_and(|n| (n as usize) < need_for(page))));
        view["filter"] = settings["workFilter"].clone();
        view["reading"] = json!(self.reading(settings));
        view["gen"] = json!(self.gen); // reads asked for so far: the panel waits for it to move after its ask
        view["dirs"] = json!(known); // the remembered folders that still exist (Start's folder for a code project)
        // the ones you chose or cloned: they win over a session's folder, also for a code project with no issue listed
        view["pinned"] = json!(known.keys().filter_map(|p| crate::clones::pinned(settings, p).map(|d| (p.clone(), d))).collect::<HashMap<_, _>>());
        // the repositories ticked on the hosts read (none: the list asks you to pick some), and a warning for many GitLab
        // projects, each read on its own
        let ticked = |f: &Feed| f.issues["hosts"].as_array().into_iter().flatten().filter_map(|h| h.as_str()).map(|h| settings["workRepos"][h].as_array().map_or(0, |a| a.len())).sum::<usize>();
        view["ticked"] = json!(self.on(settings).into_iter().map(ticked).sum::<usize>());
        view["many"] = json!(Forge::GitLab.on(settings) && ticked(&self.feeds[Forge::GitLab as usize]) > MANY_GITLAB);
        view["moreRepos"] = json!(self.more_repos.values().sum::<usize>());
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
        w.feeds[Forge::GitHub as usize] = Feed { bin: Some("/bin/gh".into()), busy: false, reviews: vec![], repos: HashMap::new(), issues: json!({ "status": "ok", "hosts": ["github.com", "ghe.example.com:8443"],
            "list": [{ "iid": 54, "project": "acme/app", "title": "t", "url": "https://github.com/acme/app/issues/54" }, { "iid": 7, "project": "t/a", "title": "x", "url": "https://ghe.example.com:8443/t/a/issues/7" }] }) };
        let on = json!({ "github": true });
        let got = w.source_of(&on, "acme/app", 54).map(|(f, bin, host, i)| (f, bin, host, i["title"].clone()));
        assert_eq!(got, Some((Forge::GitHub, "/bin/gh".into(), "github.com".into(), json!("t"))));
        assert_eq!(w.source_of(&on, "t/a", 7).map(|x| x.2), Some("ghe.example.com:8443".into()), "the signed-in host, port and all");
        assert!(w.source_of(&on, "acme/app", 55).is_none(), "only issues Bobix listed");
        assert!(w.source_of(&json!({ "github": false }), "acme/app", 54).is_none(), "GitHub off");
    }

    // GitLab projects are read at the same time (8 at most), each for 10 s at most: a project that hangs makes the host
    // named once and its other projects not asked, and the read still ends in about 10 s with the others' issues; a
    // project that is only slow is waited for; a 404 leaves out that project only; nothing is left running.
    #[cfg(unix)]
    #[test]
    fn gitlab_projects_read_at_once_and_a_hanging_host_is_cut_off() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("bobix-par-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("runs")).unwrap();
        let bin = root.join("glab-stand-in");
        // the project is in the 4th argument; each run leaves runs/<pid> while it runs and logs how many runs there are
        std::fs::write(&bin, format!(r#"#!/bin/sh
d="{dir}"; touch "$d/runs/$$"; ls "$d/runs" | wc -l >> "$d/at-once"
p=$(echo "$4" | sed 's#projects/\([^/]*\)/issues.*#\1#; s#%2F#/#')
case "$p" in g/slow) sleep 30;; g/snail*) sleep 2;; g/tick*) sleep 1;; g/gone) rm -f "$d/runs/$$"; echo '{{"message":"404 Project Not Found"}}'; echo 'glab: 404 Not Found' >&2; exit 1;; esac
n=$(echo "$p" | tr -dc 0-9); n=${{n:-0}}
echo "[{{\"iid\": 1$n, \"title\": \"t\", \"web_url\": \"https://h/$p/-/issues/1\", \"references\": {{\"full\": \"$p#1$n\"}}, \"labels\": [], \"updated_at\": \"2026-11-10T08:00:$(printf %02d $n)Z\", \"assignees\": []}}]"
rm -f "$d/runs/$$"
"#, dir = root.display())).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let b = bin.to_string_lossy().into_owned();
        let read = |repos: &[&str]| { let t = std::time::Instant::now(); let r = Forge::GitLab.page(&b, "h", &repos.iter().map(|x| x.to_string()).collect::<Vec<_>>(), "all", 21, None); (r, t.elapsed().as_secs_f64()) };
        let projects = |r: &Value| r["issues"].as_array().unwrap().iter().map(|i| i["project"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        // a) one project hangs: about 10 s, the others shown newest first, the host named once
        let (r, secs) = read(&["g/a1", "g/slow", "g/b2"]);
        assert!(secs < 12.0, "the read ends in about 10 s: {secs}");
        assert_eq!((r["ok"].clone(), r["hung"].clone(), projects(&r)), (json!(true), json!(true), vec!["g/b2".to_string(), "g/a1".into()]));
        assert_eq!(r["failed"], json!([{ "host": "h", "detail": "no answer in 10 s, ignored" }]));
        assert!(!procs::exec("pgrep", &["-f", &b], 5, &[]).ok, "nothing of it is left running");
        assert!(read(&["g/slow"]).0["ok"] == false, "a host whose every project hangs failed: the read names it");
        // b) all slow but in time: all shown, read at the same time
        let (r, secs) = read(&["g/snail1", "g/snail2", "g/snail3"]);
        assert!(secs < 5.0 && r["hung"] == false && r["failed"] == json!([]) && projects(&r).len() == 3, "{secs} {r}");
        // c) a 404: that project only, named; the host is not cut off
        let (r, _) = read(&["g/a1", "g/gone"]);
        assert_eq!((projects(&r), r["hung"].clone()), (vec!["g/a1".to_string()], json!(false)));
        assert!(r["failed"][0]["detail"].as_str().unwrap().starts_with("g/gone: "), "{r}");
        // e) 12 projects of 1 s: 8 at a time at most, two rounds
        let _ = std::fs::remove_file(root.join("at-once"));
        let _ = std::fs::remove_dir_all(root.join("runs")); // the killed runs never took their mark away
        std::fs::create_dir_all(root.join("runs")).unwrap();
        let many: Vec<String> = (10..22).map(|n| format!("g/tick{n}")).collect();
        let (r, secs) = read(&many.iter().map(String::as_str).collect::<Vec<_>>());
        let most = std::fs::read_to_string(root.join("at-once")).unwrap().split_whitespace().filter_map(|x| x.parse::<usize>().ok()).max().unwrap();
        assert!(most <= PAR_READS && projects(&r).len() == 12 && secs < 4.0, "{most} at once, {secs} s");
        let _ = std::fs::remove_dir_all(&root);
    }

    // A start waits for its session; the session that takes it is new where the start waits, and its first prompt
    // names the issue (#184): Claude desktop opens a Full start in the repository's main folder (or its own worktree
    // there), not in the flow's worktree, and the code may be in another repository than the issue.
    #[test]
    fn a_waiting_start_is_taken_by_its_own_session_only() {
        let (title, url) = (json!("Checkout rounds totals wrong"), json!("https://gl.example.test/acme/web-shop/-/issues/401"));
        let prompt = "#401 Checkout rounds totals wrong\n\nSteps…\n\nhttps://gl.example.test/acme/web-shop/-/issues/401";
        let iid = |w: Option<Value>| w.and_then(|w| w["iid"].as_i64());
        // issue in acme/web-shop, code in acme/service: Quick in the service's folder; Full in a worktree of it
        let mut w = Work::default();
        w.wait_for("/code/service", "/code/service", "acme/web-shop", 401, &title, &url);
        assert_eq!(w.claim("/code/service", "fix the build please"), None, "a session you opened there yourself: its prompt names nothing");
        assert_eq!(w.claim("/code/other", prompt), None, "another folder");
        assert_eq!(w.claim("/code/service", prompt).map(|x| x["path"].clone()), Some(json!("acme/web-shop")), "the issue's project names it, not the code's");
        assert!(w.new_for.is_empty() && w.claim("/code/service", prompt).is_none(), "taken once");
        // Full: waits in the flow's worktree; Claude starts in that worktree, the main folder, or a Claude worktree there
        // (and a start from before, in a worktree where Claude keeps its own)
        for (dir, cwd) in [("/code/service/.bobix/worktrees/service-401", "/code/service/.bobix/worktrees/service-401"), ("/code/service/.bobix/worktrees/service-401", "/code/service/.bobix/worktrees/service-401/src"),
            ("/code/service/.bobix/worktrees/service-401", "/code/service"), ("/code/service/.bobix/worktrees/service-401", "/code/service/.claude/worktrees/brave-otter"),
            ("/code/service/.claude/worktrees/bobix-401", "/code/service"), ("/code/service/.claude/worktrees/bobix-401", "/code/service/.claude/worktrees/bobix-401/src"),
            // the same folders as Windows spells them: git with "/", Claude with "\\", a trailing separator
            ("C:/code/service\\.bobix\\worktrees\\service-401", "C:\\code\\service\\.bobix\\worktrees\\service-401\\"), ("C:/code/service\\.bobix\\worktrees\\service-401", "C:\\code\\service"),
            ("C:/code/service\\.bobix\\worktrees\\service-401", "C:\\code\\service\\.claude\\worktrees\\brave-otter")] {
            w.wait_for(dir, if dir.starts_with("C:") { "C:/code/service" } else { "/code/service" }, "acme/web-shop", 401, &title, &url);
            assert_eq!(iid(w.claim(cwd, prompt)), Some(401), "{dir} <- {cwd}");
            assert!(w.new_for.is_empty());
        }
        w.wait_for("/code/service/.bobix/worktrees/service-401", "/code/service", "acme/web-shop", 401, &title, &url);
        assert_eq!(w.claim("/code/service-old", prompt), None, "a folder whose name only begins the same");
        assert_eq!(w.claim("/code/service/.bobix/worktrees/service-402", prompt), None, "another flow's worktree");
        assert_eq!(w.claim("/code/service/src", prompt), None, "a folder inside the main one");
        // the prompt edited: its link kept, or "#401 <title>" kept, is enough; "#401" alone is not
        assert_eq!(w.claim("/code/service", "#401 please"), None);
        assert_eq!(iid(w.claim("/code/service", "Look at https://gl.example.test/acme/web-shop/-/issues/401 first")), Some(401));
        // the same issue started in two code repositories: each session takes the start where it came up
        w.wait_for("/code/service", "/code/service", "acme/web-shop", 401, &title, &url);
        w.wait_for("/code/api", "/code/api", "acme/web-shop", 401, &title, &url);
        assert_eq!(w.claim("/code/api", prompt).map(|x| x["dir"].clone()), Some(json!("/code/api")));
        assert_eq!(w.new_for.values().map(|x| x["dir"].clone()).collect::<Vec<_>>(), [json!("/code/service")], "the other still waits");
        // two starts waiting in one folder: each session takes the one its prompt names
        w.wait_for("/code/service", "/code/service", "acme/web-shop", 402, &json!("Export orders as CSV"), &json!("https://gl.example.test/acme/web-shop/-/issues/402"));
        assert_eq!(iid(w.claim("/code/service", "#402 Export orders as CSV\n\nhttps://gl.example.test/acme/web-shop/-/issues/402")), Some(402));
        assert_eq!(iid(w.claim("/code/service", prompt)), Some(401));
        // cancelled: nothing takes it any more; for one issue, or every start in that folder
        w.wait_for("/code/service", "/code/service", "acme/web-shop", 401, &title, &url);
        w.wait_for("/code/service", "/code/service", "acme/web-shop", 402, &json!("Export orders as CSV"), &json!("u402"));
        assert!(w.cancel("/code/service", Some(401)) && w.claim("/code/service", prompt).is_none() && w.new_for.len() == 1);
        assert!(w.cancel("/code/service/", None) && w.new_for.is_empty(), "the folder as Windows / a trailing slash spells it");
        assert!(!w.waits_at("/code/service"));
    }

    // Reading: from the ask (filter, page, Refresh) until every tracker that is on kept an answer to it; an older ask's
    // answer does not end it, and a tracker that is off never holds it up.
    #[test]
    fn reading_lasts_until_the_newest_ask_is_answered() {
        let mut w = Work::default();
        let both = json!({ "gitlab": true, "github": true });
        assert!(!w.reading(&both), "nothing asked yet");
        w.gen = 2;
        w.read_gen = [1, 2];
        assert!(w.reading(&both), "GitLab answered an older ask only");
        assert!(!w.reading(&json!({ "gitlab": false, "github": true })), "GitLab off: not waited for");
        w.read_gen = [2, 2];
        assert!(!w.reading(&both));
        assert!(!w.reading(&json!({ "gitlab": false, "github": false })));
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
              "milestone": { "due_on": "2026-12-01T08:00:00Z" }, "user": { "login": "x" }, "updated_at": "2026-11-10T08:00:00Z", "assignees": [{ "login": "Me" }, { "login": "ann" }] },
            { "number": 54, "title": "a PR", "html_url": "https://github.com/acme/shop-app/pull/54",
              "repository_url": "https://api.github.com/repos/acme/shop-app", "labels": [], "pull_request": { "url": "u" } },
            { "number": 7, "title": "on GHE", "html_url": "https://ghe.example.com/t/a/issues/7",
              "repository_url": "https://ghe.example.com/api/v3/repos/t/a", "labels": [] }
        ]"#).unwrap();
        let got: Vec<Value> = page.as_array().unwrap().iter().filter_map(slim_gh_issue).collect();
        assert_eq!(got, [
            json!({ "iid": 53, "title": "feat: GitHub issues", "url": "https://github.com/Acme/Shop-App/issues/53", "project": "acme/shop-app", "labels": ["enhancement"], "due": null, "updated": "2026-11-10T08:00:00Z", "assignees": ["Me", "ann"] }),
            json!({ "iid": 7, "title": "on GHE", "url": "https://ghe.example.com/t/a/issues/7", "project": "t/a", "labels": [], "due": null, "updated": null, "assignees": [] }),
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
        assert_eq!((&v["status"], &v["problems"], &v["hosts"], &v["at"]), (&json!("ok"), &json!([]), &json!(["github.com"]), &json!(9)), "glab not installed: quiet while GitHub reads");
        assert_eq!(v["failed"][0]["host"], "ghe.example.com");
        assert_eq!((&v["away"][0]["path"], &v["away"][0]["key"]), (&json!("acme/shop-app"), &json!("SHA")));
        assert!(w.open_issue_ok(&both, "https://github.com/acme/shop-app/issues/53"));
        let gh_only = json!({ "gitlab": false, "github": true });
        assert_eq!(view(&w, &gh_only, &sessions)["problems"], json!([]));
        let gl_only = json!({ "gitlab": true, "github": false });
        let v = view(&w, &gl_only, &sessions);
        assert_eq!((&v["status"], &v["away"], &v["problems"]), (&json!("error"), &json!([]), &json!(["noglab"])), "nothing reads: it says why");
        assert!(!w.open_issue_ok(&gl_only, "https://github.com/acme/shop-app/issues/53"), "GitHub off: its links do not open");
        assert_eq!(view(&w, &json!({}), &sessions), json!({ "status": "off" }));
    }

    #[test]
    fn one_page_of_the_ticked_repos_newest_first() {
        // 45 issues over two trackers: 20 a page, newest first, the next page known to exist until the last
        let mk = |p: &str, n: i64, t: &str| json!({ "iid": n, "project": p, "title": "t", "labels": [], "due": null, "url": format!("https://h/{p}/issues/{n}"), "updated": t });
        let mut w = Work::default();
        w.feeds[Forge::GitHub as usize].issues = json!({ "status": "ok", "hosts": ["github.com"], "list": (0..25).map(|n| mk("a/gh", n, &format!("2026-11-{:02}T00:00:00Z", n + 1))).collect::<Vec<_>>() });
        w.feeds[Forge::GitLab as usize].issues = json!({ "status": "ok", "hosts": ["gl.example.test"], "list": (0..20).map(|n| mk("b/gl", n, &format!("2026-10-{:02}T12:00:00.000Z", n + 1))).collect::<Vec<_>>() });
        let on = json!({ "github": true, "gitlab": true, "workFilter": "mine", "workRepos": { "github.com": ["a/gh"], "gl.example.test": ["b/gl"] } });
        let ids = |v: &Value| { let mut x: Vec<(String, i64)> = v["away"].as_array().unwrap().iter().flat_map(|g| g["issues"].as_array().unwrap().iter().map(|i| (i["project"].as_str().unwrap().to_string(), i["iid"].as_i64().unwrap()))).collect(); x.sort(); x };
        let v = view(&w, &on, &Sessions::default());
        assert_eq!((&v["page"], &v["more"], &v["filter"], &v["ticked"], &v["many"]), (&json!(1), &json!(true), &json!("mine"), &json!(2), &json!(false)));
        assert_eq!(ids(&v), (5..25).map(|n| ("a/gh".to_string(), n)).collect::<Vec<_>>(), "the 20 newest: all GitHub's November ones");
        w.page = 2;
        let v = view(&w, &on, &Sessions::default());
        assert_eq!(v["more"], true);
        assert_eq!(ids(&v), (0..5).map(|n| ("a/gh".to_string(), n)).chain((5..20).map(|n| ("b/gl".to_string(), n))).collect::<Vec<_>>());
        w.page = 3;
        let v = view(&w, &on, &Sessions::default());
        assert_eq!((v["more"].clone(), ids(&v).len()), (json!(false), 5));
        // page 2 asked for while the read made for page 1 is what is in: filling until the read for page 2 is
        w.feeds[Forge::GitHub as usize].issues["need"] = json!(need_for(1));
        w.page = 2;
        assert_eq!(view(&w, &on, &Sessions::default())["filling"], true);
        w.feeds[Forge::GitHub as usize].issues["need"] = json!(need_for(2));
        assert_eq!(view(&w, &on, &Sessions::default())["filling"], false);
        let many = json!({ "gitlab": true, "workRepos": { "gl.example.test": (0..21).map(|n| format!("g/p{n}")).collect::<Vec<_>>() } });
        assert_eq!(view(&w, &many, &Sessions::default())["many"], true, "21 GitLab projects: each is a read of its own");
        assert_eq!((need_for(0), need_for(1), need_for(3), need_for(10_000)), (21, 21, 61, MAX_PAGE * PER_PAGE + 1), "a read is capped, whatever the page");
        // the repo picker's first ticks: active in the last 90 days
        let repos = [json!({ "path": "a/new", "at": "2026-11-01T00:00:00Z" }), json!({ "path": "a/old", "at": "2026-01-01T00:00:00Z" }), json!({ "path": "a/never", "at": null })];
        assert_eq!(recent(&repos, "2026-08-13T00:00:00Z"), (vec!["a/new".to_string()], 0));
        // a big organisation: only the 10 most recently active are ticked, and the rest are counted
        let big: Vec<Value> = (0..300).map(|n| json!({ "path": format!("org/p{n:03}"), "at": format!("2026-10-{:02}T{:02}:00:00Z", 1 + n % 28, n % 24) })).collect();
        let (t, more) = recent(&big, "2026-08-13T00:00:00Z");
        assert_eq!((t.len(), more), (10, 290));
        let newest = |p: &String| big.iter().find(|r| r["path"] == p.as_str()).unwrap()["at"].as_str().unwrap().to_string();
        let cut = t.iter().map(newest).min().unwrap();
        assert!(big.iter().filter(|r| !t.contains(&r["path"].as_str().unwrap().to_string())).all(|r| r["at"].as_str().unwrap() <= cut.as_str()), "the newest ones");
        // whose issues are yours: the account each host reads with
        let gh = "github.com\n  ✓ Logged in to github.com account first (keyring)\n  - Active account: false\n  ✓ Logged in to github.com account Second (keyring)\n  - Active account: true\n\
            ghe.example.com\n  ✓ Logged in to ghe.example.com as old (oauth_token)\n";
        assert_eq!(logins(gh), HashMap::from([("github.com".to_string(), "Second".to_string()), ("ghe.example.com".to_string(), "old".to_string())]));
        assert_eq!(logins("gitlab.example.test\n  ✓ Logged in to gitlab.example.test as demo (config.yml)\n")["gitlab.example.test"], "demo");
    }

    #[test]
    fn issues_count_the_sessions_on_them() {
        let issues = [json!({ "iid": 29, "project": "g/a", "title": "x", "labels": [], "due": null, "url": "u" })];
        let live = |id: &str, p: &str, iid| Live { id: id.into(), path: Some(p.into()), dir: String::new(), iid: Some(iid), of: Some(p.into()) };
        let (local, away) = group_issues(&issues, &[], &HashMap::new());
        assert!(local.is_empty() && away[0]["issues"][0]["sessions"] == json!([]));
        let (local, _) = group_issues(&issues, &[live("s1", "g/a", 29), live("s2", "g/b", 29), live("s3", "g/a", 290)], &HashMap::new());
        assert_eq!(local[0]["issues"][0]["sessions"], json!(["s1"]), "same number in another project does not count");
        // a session in g/b started for g/a's issue 29 (its code is in g/b): listed under that issue, and g/b's folder is its own
        let cross = Live { id: "s4".into(), path: Some("g/b".into()), dir: "/src/b".into(), iid: Some(29), of: Some("g/a".into()) };
        let (local, away) = group_issues(&issues, &[cross], &HashMap::new());
        assert!(local.is_empty() && away[0]["issues"][0]["sessions"] == json!(["s4"]), "g/a has no folder here: the session's folder is g/b's");
    }

    #[test]
    fn issues_grouped_local_first() {
        let mk = |p: &str, iid: i64, due: Value| json!({ "iid": iid, "project": p, "title": format!("t{iid}"), "labels": [], "due": due, "url": format!("u{iid}") });
        let issues = [mk("g/away", 1, Value::Null), mk("g/here", 2, json!("2026-12-01")), mk("g/here", 3, json!("2026-11-10")), mk("g/here", 4, Value::Null)];
        let live = [Live { id: "s".into(), path: Some("g/here".into()), dir: "/src/here".into(), iid: Some(4), of: Some("g/here".into()) }, Live { id: "x".into(), path: None, dir: "/tmp/x".into(), iid: None, of: None }];
        let (local, away) = group_issues(&issues, &live, &HashMap::new());
        assert_eq!(local.len(), 1);
        assert_eq!((local[0]["path"].as_str(), local[0]["dir"].as_str()), (Some("g/here"), Some("/src/here")));
        assert_eq!(local[0]["issues"].as_array().unwrap().iter().map(|i| i["iid"].as_i64().unwrap()).collect::<Vec<_>>(), [4, 3, 2]);
        assert_eq!(away[0]["path"], "g/away");
        assert!(away[0].get("dir").is_none());
        // a clone Bobix knows (found, chosen or cloned) makes its project local too; a session's folder wins
        let known: HashMap<String, String> = [("g/away".to_string(), "/src/away".to_string()), ("g/here".to_string(), "/elsewhere".to_string())].into();
        let (local, away) = group_issues(&issues, &live, &known);
        assert!(away.is_empty());
        let dir = |p: &str| local.iter().find(|g| g["path"] == p).unwrap()["dir"].clone();
        assert_eq!((dir("g/away"), dir("g/here")), (json!("/src/away"), json!("/src/here")));
        // unless you chose (or cloned) it: then yours wins, a found one does not
        let mut pinned = local.clone();
        pin_chosen(&mut pinned, &json!({ "workDirs": { "g/here": { "dir": "/elsewhere", "how": "chosen" }, "g/away": { "dir": "/src/away", "how": "found" } } }), &known);
        let dir = |p: &str| pinned.iter().find(|g| g["path"] == p).unwrap()["dir"].clone();
        assert_eq!((dir("g/away"), dir("g/here")), (json!("/src/away"), json!("/elsewhere")));
        assert_eq!(pinned.iter().find(|g| g["path"] == "g/here").unwrap()["via"], "chosen");
        assert_eq!(slim_issue(&json!({ "iid": 7, "title": "T", "web_url": "https://h/g/p/-/issues/7", "references": { "full": "G/P#7" }, "labels": ["Bug"], "due_date": "2026-11-20" })),
            json!({ "iid": 7, "title": "T", "url": "https://h/g/p/-/issues/7", "project": "g/p", "labels": ["Bug"], "due": "2026-11-20", "updated": null, "assignees": [] }));
        assert_eq!(slim_issue(&json!({ "updated_at": "2026-11-10T08:00:00.000Z", "assignees": [{ "username": "me" }] }))["assignees"], json!(["me"]));
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
    fn an_issue_a_session_works_on_is_read_while_open_and_leaves_with_its_last_session() {
        let gh = json!({ "number": 91, "title": "Pet costumes", "html_url": "https://github.com/o/bobix/issues/91", "repository_url": "https://api.github.com/repos/O/Bobix", "labels": [], "state": "open" });
        assert_eq!(linked_issue(Forge::GitHub, &gh), Some(json!({ "iid": 91, "title": "Pet costumes", "url": "https://github.com/o/bobix/issues/91", "project": "o/bobix", "labels": [], "due": null, "updated": null, "assignees": [], "linked": true })));
        assert_eq!(linked_issue(Forge::GitHub, &json!({ "state": "closed", "number": 91 })), None);
        assert_eq!(linked_issue(Forge::GitHub, &json!({ "state": "open", "number": 9, "pull_request": {}, "repository_url": "https://api.github.com/repos/o/b" })), None, "a pull request");
        assert_eq!(linked_issue(Forge::GitLab, &json!({ "iid": 4, "state": "opened", "title": "T", "web_url": "u", "references": { "full": "g/p#4" } }))
            .map(|i| (i["project"].clone(), i["linked"].clone())), Some((json!("g/p"), json!(true))));
        // in the view: shown only while a session works on it
        let mut w = Work::default();
        w.feeds[Forge::GitHub as usize].issues = json!({ "status": "ok", "hosts": ["github.com"], "at": 1,
            "list": [{ "iid": 5, "project": "o/bobix", "title": "mine", "labels": [], "due": null, "url": "u5" }, linked_issue(Forge::GitHub, &gh).unwrap()] });
        let v = view(&w, &json!({ "github": true }), &Sessions::default());
        assert_eq!(v["away"][0]["issues"].as_array().unwrap().iter().map(|i| i["iid"].as_i64().unwrap()).collect::<Vec<_>>(), [5]);
        let m = Model { repos: vec![model::Repo { path: "o/bobix".into(), key: "BOB".into(), auto_key: "BOB".into(), forge: "github", host: "github.com".into(), dirs: vec![] }],
            sessions: vec![model::SessionLinks { id: "s".into(), status: "working".into(), since: 0, last_activity: 0, approvals: vec![], pet: "auto".into(), dir: None, repo: None, parent: None,
                issue: Some(model::Via { to: model::IssueRef { project: "o/bobix".into(), iid: 91 }, why: model::Why::Branch }) }], ..Default::default() };
        assert_eq!(m.linked(), [("github.com".to_string(), "o/bobix".to_string(), 91)]);
        let mut w = Work::default();
        w.feeds[Forge::GitHub as usize].issues = json!({ "status": "ok", "hosts": ["github.com"], "list": [] });
        let on = json!({ "github": true });
        assert!(w.unread_link(&on, &m), "a session on an issue the board has not read");
        w.asked.insert("o/bobix#91".into());
        assert!(!w.unread_link(&on, &m), "asked once only (closed, or not readable)");
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
    fn host_names_from_the_cli_config_only() {
        let glab = "git_protocol: ssh\nhosts:\n    gitlab.com:\n        api_protocol: https\n        token: secret-a\n    \"Git.Acme.Example\":\n        token: secret-b\n        nested:\n            deep: x\nhost: gitlab.com\nother:\n    not-a-host:\n";
        assert_eq!(config_hosts(glab, true), ["gitlab.com", "git.acme.example"]);
        let gh = "github.com:\n    users:\n        me:\n    git_protocol: https\n    oauth_token: secret-c\nghe.example.com:\n    user: me\n";
        assert_eq!(config_hosts(gh, false), ["github.com", "ghe.example.com"]);
        assert!(config_hosts("", true).is_empty() && config_hosts("hosts: {}\n", true).is_empty());
    }

    // A stand-in glab: one host answers at once, one never does (as gitlab.com with no token did, for about 31 s).
    // The signed-in host is found in a few seconds, and the other is named, not a reason to say "signed in nowhere".
    #[cfg(unix)]
    #[test]
    fn one_host_that_hangs_does_not_hide_the_one_signed_in() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("bobix-glab-stub-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("glab");
        std::fs::write(&bin, "#!/bin/sh\ncase \"$4\" in\n  git.acme.example) echo \"  ✓ Logged in to git.acme.example as sam (keyring)\" >&2; exit 0;;\n  gitlab.com) exec sleep 30;;\n  *) echo \"x: not logged in\" >&2; exit 1;;\nesac\n").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let hosts: Vec<String> = ["gitlab.com", "git.acme.example", "old.example"].map(String::from).into();
        let t = std::time::Instant::now();
        let (ok, skipped, who) = check_hosts(Forge::GitLab, bin.to_str().unwrap(), &hosts, 2).expect("the stub runs");
        assert_eq!(who.get("git.acme.example").map(String::as_str), Some("sam"), "the account it reads with");
        assert!(t.elapsed().as_secs() < 6, "the hosts are checked at once, each with its own limit: {:?}", t.elapsed());
        assert_eq!(ok, ["git.acme.example"]);
        assert_eq!(skipped, [json!({ "host": "gitlab.com", "detail": "no answer in 2 s, ignored" }), json!({ "host": "old.example", "detail": "not signed in, ignored" })]);
        assert!(check_hosts(Forge::GitLab, dir.join("no-such-glab").to_str().unwrap(), &hosts, 2).is_none(), "missing CLI");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn git_status_of_this_repository() {
        let dir = env!("CARGO_MANIFEST_DIR");
        let (g, _) = git_status(dir).expect("the source folder is a git checkout");
        assert!(g["top"].as_str().is_some_and(|t| !t.is_empty()) && g["worktrees"].as_u64() >= Some(1), "{g}");
        assert!(git_status(&std::env::temp_dir().join("no-such-bobix-dir").to_string_lossy()).is_none());
    }

    // Against the GitLab(s) / GitHub(s) the person's own glab / gh is signed in to (read-only GET); prints counts only.
    // cargo test live_ -- --ignored --nocapture
    fn live(f: Forge) {
        let bin = crate::find_bin(f.bin()).expect("the CLI on PATH");
        let (hosts, skipped, logins) = f.auth(&bin).expect("it runs");
        println!("hosts: {} skipped: {} with an account: {}", hosts.len(), skipped.len(), logins.len());
        for (n, h) in hosts.iter().enumerate() {
            let repos = f.repos(&bin, h).expect("repos read");
            let paths: Vec<String> = repos.iter().take(5).filter_map(|r| r["path"].as_str().map(String::from)).collect();
            let r = f.page(&bin, h, &paths, "all", 21, logins.get(h).map(String::as_str));
            println!("host {n}: repos {} ok {} reason {} issues {}", repos.len(), r["ok"], r["reason"], r["issues"].as_array().map_or(0, |a| a.len()));
            for i in r["issues"].as_array().into_iter().flatten() { assert!(i["iid"].is_i64() && i["url"].is_string() && i["updated"].is_string() && i["project"].as_str().is_some_and(|p| paths.iter().any(|x| x == p))); }
        }
    }
    #[test]
    #[ignore]
    fn live_glab() { live(Forge::GitLab) }
    #[test]
    #[ignore]
    fn live_gh() { live(Forge::GitHub) }
}
