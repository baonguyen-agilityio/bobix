// The ticket flow (GitLab first): pick an issue, Start opens a Claude session in a worktree of its own that writes a
// plan with its assumptions first; approving that plan is the one gate (Claude's own ExitPlanMode, held through the
// PermissionRequest hook Bobix already answers); then it runs to the end in Claude's own auto mode, keeping a work log
// for you only. Bobix sets no permission rules and writes no Claude settings: the approval switches that one session's
// mode, as Claude's own "Yes, and use auto mode" does.
use crate::{js::head16, procs};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::{Path, PathBuf}};

// A worktree with a build takes about this much (the product decision's measure); a new one needs it free.
pub const WORKTREE_BYTES: u64 = 5 * 1024 * 1024 * 1024;
// The work log of a flow started before the log moved out of the worktree: `.bobix-log/work-log.md` in its folder, in a
// folder that ignores itself. Such a flow (empty `log_file`) is still read from there; nothing new is written there.
pub const LOG_DIR: &str = ".bobix-log";
pub const LOG_FILE: &str = "work-log.md";
const LOG_MAX: usize = 64 * 1024;
// Cleaned flows kept in the list (and tickets.json); their logs stay in worklogs/ either way.
pub const CLEANED_KEEP: usize = 20;

// One ticket in the flow. status: starting (worktree made, no session yet) | planning | approve (its plan waits for
// you) | running | done | blocked. cleaned: its worktree was removed (the log stays, copied into Bobix's folder).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Flow {
    pub key: String, pub path: String, pub iid: i64, pub title: String, pub url: String,
    pub repo: String, pub dir: String, pub branch: String, pub base: String,
    pub base_branch: String, // the remote branch `base` was cut from ("develop"): the merge / pull request targets it; empty on a flow from before it was asked
    pub status: String, pub session: Option<String>, pub plan: String, pub log: String,
    // The work log the session writes and Bobix reads: one file per ticket in Bobix's folder (`log_path`), outside every
    // repository and worktree. Empty on a flow from before: its log is `.bobix-log/work-log.md` in its folder.
    pub log_file: String,
    pub log_missing: bool, // the log file is gone (deleted by hand): `log` keeps the text last read
    pub seen: usize, // assumptions you have seen; more in the log put a mark on the pet
    pub summary: String, pub models: Value, pub at: u64, pub cleaned: bool,
    pub mr: String, // the merge request (GitLab) / pull request (GitHub) Bobix opened for it (its link), on your click
    pub forge: String, // its issue's tracker: gitlab (also when empty, from before GitHub) | github
    pub ci: Value,  // its head pipeline, as last read: { status: passed | running | failed | other | none, url, state, since, at }
    // The repository its code is in, when not the issue's own (an issue of a product repository whose code lives in
    // a service repository): the worktree, the MR / PR and its CI are there. Empty: the issue's repository.
    pub code: String,
    // Bobix's language when it started ("vi"; "" or "en": English): its work log, summary and review notes are in it,
    // and it keeps it when you change the language later. The MR / PR text and commits are English either way.
    pub lang: String,
    pub pid: Option<u32>, // its session's claude process, as last seen: alive after Bobix restarts means it still runs
    // Where its session came up when Claude did not open it in the worktree Bobix made (`place_of`). "claude": a worktree
    // Claude made for that session; the flow works there (`dir`, and `branch` follows that folder's), and `own` keeps the
    // worktree Bobix made, unused. "main": the repository's main folder; the flow keeps its worktree (the prompt sends
    // the session there), and `mixed` counts the files changed and not committed in the main folder at that moment
    // (yours, not the ticket's). "": the worktree, as started.
    pub place: String, pub own: String, pub mixed: usize,
    // In a worktree Claude made: commits that no remote has, already on its branch when the session came (Claude cuts
    // that worktree from the main folder's HEAD, so your own unpushed commits ride along into the ticket's MR).
    pub carried: u64,
}

pub fn key_of(path: &str, iid: i64) -> String { format!("{path}#{iid}") }
// A flow's key: the issue's ("group/app#7"), and the code repository after ">" when it is another one
// ("group/app#7>group/api"): one issue can have a flow in each repository its change needs.
pub fn flow_key(path: &str, iid: i64, code: &str) -> String {
    if code.is_empty() || code == path { key_of(path, iid) } else { format!("{}>{code}", key_of(path, iid)) }
}
// In another repository, the worktree and the branch carry the issue repository's name ("tracker-7-…"), so they are
// not taken for that repository's own issue 7; in its own, as before ("7-…").
fn from_tag(path: &str, code: &str) -> String {
    if code.is_empty() || code == path { String::new() } else { format!("{}-", slug(path.rsplit('/').next().unwrap_or(path))) }
}

impl Flow {
    // The log read again, and the status it sets (done / blocked; running once its plan was approved).
    fn reread(&mut self) {
        let read = read_log_of(self);
        self.log_missing = read.is_none();
        if let Some(l) = read { if l != self.log { self.log = l; } }
        let st = parse_log(&self.log)["status"].as_str().unwrap_or("").to_string();
        if matches!(st.as_str(), "done" | "blocked") && self.status != "approve" { self.status = st; }
        else if st == "running" && self.status == "planning" && !self.plan.is_empty() { self.status = "running".into(); }
    }
}
// The merge request's text, from the work log: the issue it closes, the plan, the assumptions, the checks. Shown to you
// (and editable) before anything is sent: the work log itself stays on this machine. A log in another language than
// English gives its own `## MR (English)` section instead, written by the session for the team (never a translation
// made here); without it there is only the close line, and the box says so (mr_ready).
pub fn mr_title(f: &Flow) -> String { head16(&f.title, 200).to_string() }
pub fn english(f: &Flow) -> bool { matches!(f.lang.as_str(), "" | "en") }
pub fn mr_ready(f: &Flow) -> bool { english(f) || !parse_log(&f.log)["mr"].as_str().unwrap_or("").trim().is_empty() }
pub fn mr_body(f: &Flow) -> String {
    let p = parse_log(&f.log);
    let close = if f.code_repo() == f.path { format!("Closes #{}\n", f.iid) } else { format!("Closes {}#{}\n", f.path, f.iid) };
    if !english(f) { return head16(&format!("{close}\n{}\n", p["mr"].as_str().unwrap_or("").trim()), 20_000).trim_end().to_string() + "\n"; }
    let plan = p["plan"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| f.plan.clone());
    let list = |k: &str| p[k].as_array().into_iter().flatten().filter_map(|x| x.as_str()).map(|x| format!("- {x}")).collect::<Vec<_>>().join("\n");
    let checks = p["checksText"].as_str().unwrap_or("").to_string();
    // in another repository the close reference names the issue's (GitLab and GitHub both read "group/app#7")
    let mut b = close;
    for (h, t) in [("Plan", plan), ("Assumptions", list("assumptions")), ("Checks", if checks.is_empty() { "None pasted.".into() } else { checks })] {
        if !t.trim().is_empty() { b += &format!("\n## {h}\n\n{}\n", t.trim()); }
    }
    head16(&b, 20_000).to_string()
}
impl Flow {
    // The folder Bobix made for this flow: the only one Clean up and Remove ever delete.
    pub fn made(&self) -> &str { if self.own.is_empty() { &self.dir } else { &self.own } }
    // Where git is asked about its branch: its folder, or the repository's main folder once that folder is gone (a
    // worktree deleted by hand leaves its branch and commits in the repository).
    pub fn git_dir(&self) -> &str { if Path::new(&self.dir).is_dir() { &self.dir } else { &self.repo } }
    pub fn github(&self) -> bool { self.forge == "github" }
    pub fn code_repo(&self) -> &str { if self.code.is_empty() { &self.path } else { &self.code } }
    // Where the session is told to keep the log: its file in Bobix's folder, or for a flow from before, the old place in its folder.
    pub fn log_at(&self) -> String {
        if self.log_file.is_empty() { Path::new(&self.dir).join(LOG_DIR).join(LOG_FILE).to_string_lossy().into_owned() } else { self.log_file.clone() }
    }
    // The folder a terminal start adds to Claude (`--add-dir`) so it can write the log: the log file's folder, none for an old flow.
    pub fn log_dir(&self) -> Option<String> { dir_of(&self.log_file) }
}
// The folder of a file path, cut at either separator so a Windows path is cut the same on any system; None for a bare name.
pub fn dir_of(file: &str) -> Option<String> { file.rsplit_once(['/', '\\']).map(|(d, _)| d.to_string()).filter(|d| !d.is_empty()) }
// The merge request's (pull request's) link in what glab (gh) printed.
pub fn mr_url(out: &str) -> Option<String> {
    static U: std::sync::LazyLock<fancy_regex::Regex> = std::sync::LazyLock::new(|| fancy_regex::Regex::new(r"https?://\S+(?:/-/merge_requests|/pull)/\d+").unwrap());
    U.find(out).ok().flatten().map(|m| m.as_str().to_string())
}
// The merge request's number, from its link.
pub fn mr_iid(url: &str) -> Option<i64> { url.rsplit_once("/merge_requests/").or_else(|| url.rsplit_once("/pull/"))?.1.split(['/', '?', '#']).next()?.parse().ok() }
// The MR's head pipeline as the flow shows it (GitLab's statuses folded into four), and the MR's own state.
pub fn ci_of(mr: &Value) -> Value {
    let p = &mr["head_pipeline"];
    let st = match p["status"].as_str() {
        Some("success") => "passed",
        Some("failed") => "failed",
        Some("running" | "pending" | "created" | "preparing" | "waiting_for_resource" | "scheduled" | "manual") => "running",
        Some(_) => "other",
        None => "none",
    };
    json!({ "status": st, "url": p["web_url"], "state": mr["state"] })
}
// Reads the MR (a GET through your own glab) for its pipeline. None when the read fails.
pub fn read_ci(glab: &str, host: &str, path: &str, iid: i64) -> Option<Value> {
    let api = format!("projects/{}/merge_requests/{iid}", crate::work::enc(path));
    let r = procs::exec(glab, &["api", "--hostname", host, &api], 30, &[("NO_PROMPT", "1"), ("NO_COLOR", "1")]);
    serde_json::from_str::<Value>(&r.out).ok().filter(|v| v.is_object()).map(|v| ci_of(&v))
}

// A pull request's checks (`gh pr checks --json bucket,link`) folded the same way: any failing → failed, any pending →
// running, all passed or skipped → passed, none → none; `state` from `gh pr view --json state` (OPEN, MERGED, CLOSED).
pub fn ci_of_checks(checks: &Value, state: &str) -> Value {
    let a: Vec<&Value> = checks.as_array().into_iter().flatten().collect();
    let has = |b: &str| a.iter().any(|c| c["bucket"] == b);
    let st = if a.is_empty() { "none" } else if has("fail") { "failed" } else if has("pending") { "running" } else if a.iter().all(|c| matches!(c["bucket"].as_str(), Some("pass" | "skipping"))) { "passed" } else { "other" };
    let url = a.iter().find(|c| c["bucket"] == "fail").or(a.first()).map(|c| c["link"].clone()).unwrap_or(Value::Null);
    let state = match state { "MERGED" => "merged", "CLOSED" => "closed", _ => "opened" };
    json!({ "status": st, "url": url, "state": state })
}
const GH_ENV: &[(&str, &str)] = &[("GH_PROMPT_DISABLED", "1"), ("GH_NO_UPDATE_NOTIFIER", "1"), ("NO_COLOR", "1")];
// Reads a pull request's checks and state through your own gh (GETs). None when the read fails.
pub fn read_ci_gh(gh: &str, host: &str, path: &str, n: i64) -> Option<Value> {
    let env: Vec<(&str, &str)> = GH_ENV.iter().copied().chain([("GH_HOST", host)]).collect();
    let n = n.to_string();
    let v = procs::exec(gh, &["pr", "view", &n, "--repo", path, "--json", "state"], 30, &env);
    let state: Value = serde_json::from_str(&v.out).ok()?;
    let c = procs::exec(gh, &["pr", "checks", &n, "--repo", path, "--json", "bucket,link,name"], 30, &env); // exits 8 while pending
    let checks: Value = serde_json::from_str(c.out.trim()).unwrap_or(json!([]));
    Some(ci_of_checks(&checks, state["state"].as_str().unwrap_or("")))
}

// A commit id of `rev` in a folder ("" when git cannot say), for the smoke check that a worktree sits on the remote branch's commit.
pub fn head_commit_of(dir: &str, rev: &str) -> String { git(dir, &["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")]).out.trim().to_string() }
pub fn head_commit(dir: &str) -> String { head_commit_of(dir, "HEAD") }
// Commits on the ticket's branch that no remote has.
pub fn unpushed(dir: &str, branch: &str) -> u64 {
    let r = git(dir, &["rev-list", "--count", branch, "--not", "--remotes"]);
    if r.ok { r.out.trim().parse().unwrap_or(0) } else { 0 }
}
// Push the ticket's branch (never the default one) and open its merge request with your own glab, or its pull request
// with your own gh (`bin`, for the flow's forge). → the link.
// The branch the ticket was cut from is where its merge / pull request goes (`flag`: the forge's option for it);
// a flow from before the branch was asked passes none, and the forge uses its own default as it did.
fn target<'a>(f: &'a Flow, flag: &'a str) -> Vec<&'a str> { if f.base_branch.is_empty() { vec![] } else { vec![flag, &f.base_branch] } }
// The arguments of `gh pr create` and of `glab mr create` as open_mr runs them.
fn gh_pr_args<'a>(f: &'a Flow, title: &'a str, body: &'a str) -> Vec<&'a str> {
    ["pr", "create", "--repo", f.code_repo(), "--head", &f.branch, "--title", title, "--body", body].into_iter().chain(target(f, "--base")).collect()
}
fn glab_mr_args<'a>(f: &'a Flow, repo: &'a str, title: &'a str, body: &'a str) -> Vec<&'a str> {
    ["mr", "create", "--repo", repo, "--source-branch", &f.branch, "--title", title, "--description", body, "--yes"].into_iter().chain(target(f, "--target-branch")).collect()
}
pub fn open_mr(bin: &str, host: &str, f: &Flow, title: &str, body: &str) -> Result<String, String> {
    if f.branch.is_empty() || ["main", "master", "develop"].contains(&f.branch.as_str()) { return Err("not a ticket branch".into()); }
    // nothing committed on it (the session worked in another folder, or did not commit): said, not pushed as an empty MR
    if !f.base.is_empty() && git(f.git_dir(), &["rev-list", "--count", &format!("{}..{}", f.base, f.branch)]).out.trim() == "0" { return Err("nocommits".into()); }
    let r = git(f.git_dir(), &["push", "-u", "origin", &f.branch]);
    if !r.ok { return Err(format!("git push: {}", one_line(&r))); }
    if f.github() {
        let env: Vec<(&str, &str)> = GH_ENV.iter().copied().chain([("GH_HOST", host)]).collect();
        let r = procs::exec(bin, &gh_pr_args(f, title, body), 60, &env);
        return mr_url(&format!("{}\n{}", r.out, r.err)).ok_or_else(|| format!("gh pr create: {}", one_line(&r)));
    }
    let glab = bin;
    let repo = format!("https://{host}/{}", f.code_repo());
    let r = procs::exec(glab, &glab_mr_args(f, &repo, title, body), 60, &[("GITLAB_HOST", host), ("NO_PROMPT", "1"), ("NO_COLOR", "1")]);
    let out = format!("{}\n{}", r.out, r.err);
    match mr_url(&out) { Some(u) => Ok(u), None => Err(format!("glab mr create: {}", one_line(&r))) }
}

// The model of the review: the split's review step, else Opus (a strong reviewer, read-only).
pub fn review_model(models: &Value) -> String {
    models["steps"].as_array().into_iter().flatten().find(|s| s["step"] == "review").and_then(|s| s["model"].as_str())
        .filter(|m| ["haiku", "sonnet", "opus"].contains(m)).unwrap_or("opus").to_string()
}

// "Cart total ignores discount" → "cart-total-ignores-discount", ASCII only, cut at a word near 40 characters.
pub fn slug(title: &str) -> String {
    let mut s = String::new();
    for w in title.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()) {
        if !s.is_empty() && s.len() + w.len() + 1 > 40 { break; }
        if !s.is_empty() { s.push('-'); }
        s.push_str(&w.to_ascii_lowercase());
    }
    s.truncate(40);
    s
}
// The branch names the issue the way Bobix already reads branches ("<iid>-…": model::issue_of_branch); in another
// repository than the issue's, after the issue repository's name (`from_tag`).
pub fn branch_for(iid: i64, title: &str, path: &str, code: &str) -> String {
    let (s, t) = (slug(title), from_tag(path, code));
    if s.is_empty() { format!("{t}{iid}-ticket") } else { format!("{t}{iid}-{s}") }
}
// The flow's worktree: <repo>/.bobix/worktrees/<repo's name>-<iid>. Inside the repository, so Claude trusts it as it
// trusts the repository, in a folder that ignores itself (WT_DIR), and not where Claude keeps its own worktrees:
// Claude desktop's `claude://code/new?folder=` handler cuts a folder at or under `<repo>/.claude/worktrees/<name>`
// down to `<repo>` (read in the app, 5.11492.0), so a start there opened the repository's main folder instead.
pub const WT_DIR: &str = ".bobix/worktrees";
pub fn worktree_dir(repo: &str, iid: i64, path: &str, code: &str) -> PathBuf {
    let name = Some(slug(repo.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or(""))).filter(|s| !s.is_empty()).unwrap_or_else(|| "ticket".into());
    WT_DIR.split('/').fold(PathBuf::from(repo), |p, x| p.join(x)).join(format!("{name}-{}{iid}", from_tag(path, code)))
}
// Where Bobix made worktrees before (where Claude keeps its own, which Claude desktop's link cuts down to the
// repository). One still there on the ticket's branch (a flow you removed and kept) is moved to `to` with git, its
// changes and log with it, so a new Start uses it again in a folder the link keeps. Ok(true) when moved.
pub fn move_kept(repo: &str, iid: i64, path: &str, code: &str, branch: &str, to: &Path) -> Result<bool, String> {
    let old = Path::new(repo).join(".claude").join("worktrees").join(format!("bobix-{}{iid}", from_tag(path, code)));
    if to.exists() || !old.is_dir() || git(&old.to_string_lossy(), &["rev-parse", "--abbrev-ref", "HEAD"]).out.trim() != branch { return Ok(false); }
    ignored_parent(to);
    let r = git(repo, &["worktree", "move", &old.to_string_lossy(), &to.to_string_lossy()]);
    if r.ok { Ok(true) } else { Err(one_line(&r)) }
}
// The folder that holds Bobix's worktrees ignores itself: the main folder's git status never lists them.
fn ignored_parent(dir: &Path) {
    if let Some(p) = dir.parent().filter(|p| p.ends_with(WT_DIR)) {
        let _ = fs::create_dir_all(p).and_then(|_| if p.join(".gitignore").exists() { Ok(()) } else { fs::write(p.join(".gitignore"), "*\n") });
    }
}
// A worktree of `branch` whose folder was deleted by hand still holds the branch: git forgets that one (only that
// one; other worktrees whose folder is missing, on a disk not mounted now, are left alone).
fn forget_missing(repo: &str, branch: &str) {
    let list = git(repo, &["worktree", "list", "--porcelain"]).out;
    for e in list.split("\n\n") {
        let path = e.lines().find_map(|l| l.strip_prefix("worktree "));
        let on = e.lines().any(|l| l == format!("branch refs/heads/{branch}"));
        if let Some(p) = path.filter(|p| on && !Path::new(p).exists()) { let _ = git(repo, &["worktree", "remove", "--force", p]); }
    }
}
// Where a flow's session came up, from the top folder git reports for it: "" (the flow's worktree), "main" (its
// repository's main folder), "claude" (a worktree Claude made, under the repository's `.claude/worktrees`), else None
// (a folder that is not this flow's business). Folders are compared as folders (separators, a trailing one, case).
pub fn place_of(f: &Flow, top: &str) -> Option<&'static str> {
    use crate::paths::{dir_key, same_dir};
    if same_dir(top, &f.dir) { return Some(""); }
    if f.repo.is_empty() { return None; }
    if same_dir(top, &f.repo) { return Some("main"); }
    dir_key(top).starts_with(&(dir_key(&f.repo) + "/.claude/worktrees/")).then_some("claude")
}

// Free bytes on the disk that holds `dir`; None where Bobix can't tell (Windows), and then nothing is capped.
#[cfg(unix)]
pub fn free_bytes(dir: &Path) -> Option<u64> {
    let c = std::ffi::CString::new(dir.as_os_str().as_encoded_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 { return None; }
    Some(st.f_bavail as u64 * st.f_frsize as u64)
}
#[cfg(not(unix))]
pub fn free_bytes(_: &Path) -> Option<u64> { None }
// How many more worktrees fit on that disk.
pub fn room(free: Option<u64>) -> Option<u64> { free.map(|f| f / WORKTREE_BYTES) }

// The work log's template: Bobix writes it into the worktree, the session fills it in. Not in English: one more
// section, the merge request's description in English.
pub fn template(iid: i64, title: &str, lang: &str) -> String {
    let mr = if lang_name(lang).is_some() { "\n## MR (English)\n" } else { "" };
    format!("# Work log: #{iid} {title}\n\nStatus: planning\n\n## Ticket\n\n## Questions\n\n## Later\n\n## Plan\n\n## Mapping\n\n## Steps\n\n## Assumptions\n\n## Checks\n\n## Result\n{mr}")
}
// The languages a work log can be in other than English (Bobix's own languages).
pub fn lang_name(lang: &str) -> Option<&'static str> { match lang { "vi" => Some("Vietnamese"), _ => None } }

// How the session works, appended to the issue in the Start prompt (Claude reads English here whatever the panel's
// language). `lang`: Bobix's language: the work log is written in it; what the team reads stays English.
// `dir`: the flow's worktree. Claude may open the session somewhere else (rule 2 says what then): Bobix follows it.
pub fn rules(branch: &str, dir: &str, log: &str, lang: &str) -> String {
    let base = rules_en(branch, dir, log);
    let Some(name) = lang_name(lang) else { return base };
    format!("{base}\n\
6. Language: restate the ticket and ask every question in {name}: the question, its choices and the \"Out of scope for this ticket\" choice (translate that one), keeping the tags `[scope]`, `[approach]`, `[edge case]`, `[done criteria]` in English. Then write the work log in {name}: the ticket summary, your questions and the choices offered, the plan and its mapping, the assumptions, the steps, your notes under Checks, what blocks you and the result. Keep code, file names, commands, command output and identifiers exactly as they are. Keep the template's headings, `Status:` with its English values (planning, running, done, blocked) and the `- Review: ` / `- None: ` prefixes in English, as written.\n\
7. What the team reads stays English: write commit messages in English, and fill `## MR (English)` in the work log with the merge request's description for the team, in English: what changed and why, the assumptions, the checks you ran. Write it for that reader; do not translate the log line by line.")
}
fn rules_en(branch: &str, dir: &str, log: &str) -> String {
    format!("How to work on this ticket (Bobix ticket flow):\n\
1. Call the EnterPlanMode tool now, before changing anything. Read the ticket and the code first, then interview me before you plan. Facts are your job: never ask what the ticket or the code already answers. Decisions are mine.\n\
   a. Restate the ticket in one screen: the goal, what to do, what not to do, what \"done\" means. Show it, then ask me to confirm it with the AskUserQuestion tool (choices: Confirm, or Correct it; no tag, source or \"Out of scope\" choice, and it does not count toward the ~8 questions) and wait for my answer. Ask nothing else before I confirm. Every later question must serve this summary. (Plan mode lets you edit only the plan file: once the plan is approved, write `## Ticket`, `## Questions`, `## Later` and `## Mapping` into the work log in one go.)\n\
   b. If nothing is left to decide, say \"No questions, because ...\" in one line, ask me to confirm that with AskUserQuestion too (so I see you are asking), and go to the plan. Do not ask for the sake of asking.\n\
   c. Otherwise ask with the AskUserQuestion tool, in rounds. A round holds only questions whose answer does not depend on another open question; ask the dependent ones in the next round. Give each question choices, your recommended choice first (say why, with the evidence), and as the last choice \"Out of scope for this ticket\". Start each question with its tag in brackets: [scope], [approach], [edge case] or [done criteria]. Name its source in the question: a quote from the ticket, or a file and line in the code. No source, no question. If I pick \"Out of scope for this ticket\", drop it, add it under `## Later` and keep it out of the plan. If I answer \"up to you\", write it under Assumptions and do not ask it again.\n\
   d. Stop when nothing is left silently assumed, or after about 8 questions in all. Keep each question for `## Questions`, one `- ` line: `[tag] the question (source) -> my answer`.\n\
   e. Then write the plan. It must end with a section \"Mapping\" (a table: each requirement of the ticket -> the plan step that does it; each answer of mine -> the step that uses it; a requirement with no step, or a step with no source, must show) and a section \"Assumptions\": what you assume without asking. Put the summary under `## Ticket` and the Mapping under `## Mapping` in the work log after approval. Then call ExitPlanMode. Approving that plan is the only approval.\n\
2. This ticket has its own git worktree: `{dir}`, on branch `{branch}`. Work there to the end. If this session did not start in that folder: when it started in a worktree Claude made for it (a folder under `.claude/worktrees`), stay in that worktree and on its branch; when it started anywhere else (the repository's main folder), run `cd` into `{dir}` before anything else and read, change and commit only there. Files that were already changed before you started are not this ticket's: leave them as they are. After approval, do not stop to ask, except for these: pushing to the main branch, deleting data, calling hosts this project does not already use, and touching secrets. Commit on the branch of the folder you work in; do not push to the main branch.\n\
3. Keep the work log in `{log}` (the full path; Bobix made it from the template; it is outside every repository and worktree on purpose, it is for me only: never copy it into the repository, never commit or share it). It is the only place for the work log: do not start another one in the repository or a worktree. Fill in its template: `Status:` (planning, running, done or blocked), `## Ticket`, `## Questions` and `## Later` (rule 1), `## Plan` (the approved plan in short), `## Mapping` (rule 1), `## Steps` (one line per step as you go), `## Assumptions` (one `- ` line for each assumption you make along the way, also after approval), `## Checks` (steps 4 and 5), `## Result` (what you did, or what blocks you). Update it as you work.\n\
4. Before you set `Status: done`, run the project's tests, build and lint: the commands in CLAUDE.md, else the ones the project clearly uses. Under `## Checks`, add one `- ` line per command with the command in backticks, then the last lines of its output in a code block. If the project has none, write `- None: ` and why.\n\
5. Then review your change against the approved plan and the Assumptions: run `/code-review`, or ask a read-only reviewer subagent. Fix what affects correctness, run the checks again, and add a `- Review: ` line under `## Checks` with what it found and what you fixed. Then set the status.")
}

// The proposal at Start (Haiku's classification, or "small" when you skip it), as lines for the prompt.
// models: { size: small | large, steps: [{ step: explore | edit | review, model: haiku | sonnet | opus }] }
pub fn models_text(models: &Value) -> String {
    let steps: Vec<(&str, &str)> = models["steps"].as_array().into_iter().flatten()
        .filter_map(|s| Some((s["step"].as_str()?, s["model"].as_str()?)))
        .filter(|(s, m)| ["explore", "edit", "review"].contains(s) && ["haiku", "sonnet", "opus"].contains(m)).collect();
    if models["size"] != "large" || steps.is_empty() { return "Models: a small ticket. Do it in this one session; no subagents needed.".into(); }
    let mut t = String::from("Models: split the work.");
    for (s, m) in steps {
        t += &match s {
            "explore" if m == "haiku" => "\n- Explore with the built-in Explore subagent (read-only; it runs on Haiku).".to_string(),
            "explore" => format!("\n- Explore with a read-only subagent on model `{m}`."),
            "review" => format!("\n- Review the finished change with a subagent on model `{m}` before you set the status."),
            _ => format!("\n- Edit in this session (`{m}` suits it)."),
        };
    }
    t
}
// The model this session should run on: the edit step's, else Sonnet.
pub fn main_model(models: &Value) -> String {
    models["steps"].as_array().into_iter().flatten().find(|s| s["step"] == "edit").and_then(|s| s["model"].as_str())
        .filter(|m| ["haiku", "sonnet", "opus"].contains(m)).unwrap_or("sonnet").to_string()
}
// The same start in a terminal, where the model and plan mode can be preset (the desktop link can't): one line to paste.
// `add`: a folder Claude may write in besides `dir` (the work log's), as `--add-dir=<folder>`: written with `=` so the
// prompt after it is not taken for one more folder (the option takes any number of values).
pub fn cli_command(dir: &str, model: &str, prompt: &str, add: Option<&str>) -> String { cli_in_mode(dir, model, "plan", prompt, add) }
pub fn cli_in_mode(dir: &str, model: &str, mode: &str, prompt: &str, add: Option<&str>) -> String {
    let q = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    let p = if prompt.is_empty() { String::new() } else { format!(" {}", q(prompt)) };
    let a = add.map(|d| format!(" --add-dir={}", q(d))).unwrap_or_default();
    format!("cd {} && claude --model {} --permission-mode {mode}{a}{p}", q(dir), q(model))
}

// The text of a list line: `- x`, `* x` or `1. x` / `1) x`; None for a line that is not a list item.
fn listed(t: &str) -> Option<&str> {
    if let Some(i) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) { return Some(i); }
    let d = t.trim_start_matches(|c: char| c.is_ascii_digit());
    if d.len() < t.len() { d.strip_prefix(". ").or_else(|| d.strip_prefix(") ")) } else { None }
}
// What the log says, section by section. Assumptions are the `- ` / `* ` lines under "## Assumptions".
pub fn parse_log(text: &str) -> Value {
    let (mut status, mut sec) = (String::new(), String::new());
    let (mut plan, mut steps, mut assume, mut result, mut checks, mut raw, mut mr) = (vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
    let (mut ticket, mut questions, mut later, mut mapping) = (vec![], vec![], vec![], vec![]);
    let mut fence = false;
    for l in text.lines() {
        let t = l.trim();
        if sec.starts_with("check") && !t.starts_with("## ") { raw.push(l); }
        // command output pasted under Checks, in code blocks: not items
        if sec.starts_with("check") && t.starts_with("```") { fence = !fence; continue; }
        if fence { continue; }
        if let Some(h) = t.strip_prefix("## ") {
            sec = h.trim().to_lowercase();
            if sec.starts_with("map") { mapping.clear(); } // one Mapping: the last one wins (a plan may carry its own)
            continue;
        }
        if sec.starts_with("mr") { mr.push(l); continue; } // the merge request's own text, kept as written (blank lines too)
        if sec.starts_with("map") { mapping.push(l); continue; } // a table: kept as written
        if t.starts_with('#') { continue; }
        if sec.is_empty() { if let Some(s) = t.strip_prefix("Status:") { status = s.trim().to_lowercase(); } continue; }
        if t.is_empty() { continue; }
        let item = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).unwrap_or(t);
        match sec.as_str() {
            s if s.starts_with("ticket") => ticket.push(t),
            s if s.starts_with("question") => if let Some(i) = listed(t) { questions.push(i) },
            s if s.starts_with("later") => if let Some(i) = listed(t) { later.push(i) },
            s if s.starts_with("plan") => plan.push(t),
            s if s.starts_with("step") => steps.push(item),
            s if s.starts_with("assum") => if t != item { assume.push(item) },
            s if s.starts_with("check") => if t != item { checks.push(item) },
            s if s.starts_with("result") || s.starts_with("block") => result.push(t),
            _ => {}
        }
    }
    // English values, as the template has them; a session that wrote them in Vietnamese anyway is read too
    const VI: [(&str, &str); 6] = [("xong", "done"), ("hoàn thành", "done"), ("bị chặn", "blocked"), ("chặn", "blocked"), ("đang chạy", "running"), ("lập kế hoạch", "planning")];
    let status = ["planning", "running", "done", "blocked"].into_iter().find(|s| status.starts_with(s))
        .or_else(|| VI.iter().find(|(v, _)| status.starts_with(v)).map(|x| x.1)).unwrap_or("");
    // A command run (not "None: …", not the review), and whether a review is noted.
    let low = |c: &&str| c.to_lowercase();
    let ran = checks.iter().filter(|c| { let c = low(c); !c.starts_with("none") && !c.starts_with("review") }).count();
    let reviewed = checks.iter().any(|c| low(&c).starts_with("review"));
    json!({ "status": status, "ticket": ticket.join("\n"), "questions": questions, "later": later, "mapping": mapping.join("\n").trim().to_string(), "plan": plan.join("\n"), "steps": steps, "assumptions": assume, "result": result.join("\n"),
        "checks": checks, "ran": ran, "reviewed": reviewed, "checksText": raw.join("\n").trim().to_string(), "mr": mr.join("\n").trim().to_string() })
}

// The prompt of a second session that reviews a done ticket, against its plan and assumptions; it changes no code and
// notes what it found under ## Checks. The branch and the ticket's folder (`dir`) are named, not "this folder":
// Claude desktop may open the review somewhere else in the repository (the main folder).
pub fn review_prompt(branch: &str, base: &str, dir: &str, log: &str, lang: &str) -> String {
    let say = lang_name(lang).map(|n| format!(" Write what you found in {n}; keep the `- Review (second session): ` prefix as it is.")).unwrap_or_default();
    format!("Review the change on branch `{branch}` (`git diff {base}...{branch}`; its folder is `{dir}`) against the approved plan and the Assumptions in the work log `{log}` (the full path; it is outside the repository on purpose). \
Run `/code-review`, or read the diff yourself. Do not change any code or commit. Look for what breaks the plan, a wrong assumption, a missing check, or a bug. \
When you are done, add one line under `## Checks` in that work log: `- Review (second session): ` and what you found, or `no issues`.{say}")
}
pub fn assumptions(log: &str) -> usize { parse_log(log)["assumptions"].as_array().map_or(0, |a| a.len()) }

fn git(dir: &str, args: &[&str]) -> procs::Ran { git_for(dir, args, 60) }
fn git_for(dir: &str, args: &[&str], secs: u64) -> procs::Ran {
    let mut a = vec!["-C", dir];
    a.extend_from_slice(args);
    procs::exec("git", &a, secs, &[("LC_ALL", "C"), ("GIT_TERMINAL_PROMPT", "0")])
}
// A fetch before a Full start gets this long; offline it fails at once, a slow network is not waited for.
pub const FETCH_SECS: u64 = 30;

// What a ticket branch starts from: the remote branch you picked (`name` "origin/develop", `branch` "develop",
// `commit` its id), fetched at that moment. Never a local copy and never a guessed default branch.
#[derive(Debug, Clone, PartialEq)]
pub struct Base { pub name: String, pub branch: String, pub commit: String }
// Why a start from a branch stopped. kind: noremote | offline (the remote could not be asked) | none (no branch on it) |
// gone (the picked one is not there any more) | otherbase (the ticket's branch was cut from another one, `detail`) |
// mismatch (the new worktree is not on that commit) | git.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseErr { pub kind: &'static str, pub detail: String }
fn berr(kind: &'static str, detail: impl Into<String>) -> BaseErr { BaseErr { kind, detail: detail.into() } }
fn remote_of(repo: &str) -> Result<String, BaseErr> {
    let remotes = git(repo, &["remote"]).out;
    remotes.lines().map(str::trim).find(|r| *r == "origin").or_else(|| remotes.lines().map(str::trim).find(|r| !r.is_empty()))
        .map(String::from).ok_or_else(|| berr("noremote", "this repository has no remote to start from"))
}
// The branches the remote has right now (`git ls-remote`, asked each time). Offline or refused: an error, not a stale list.
pub fn branches(repo: &str) -> Result<Vec<String>, BaseErr> {
    let remote = remote_of(repo)?;
    let r = git_for(repo, &["ls-remote", "--heads", &remote], FETCH_SECS);
    if !r.ok { return Err(berr("offline", one_line(&r))); }
    let mut v: Vec<String> = r.out.lines().filter_map(|l| l.split_once('\t')).filter_map(|(_, n)| n.trim().strip_prefix("refs/heads/")).map(String::from).collect();
    v.sort();
    if v.is_empty() { return Err(berr("none", format!("{remote} has no branch"))); }
    Ok(v)
}
// The picked branch, fetched now, and its commit.
pub fn base_of(repo: &str, branch: &str) -> Result<Base, BaseErr> {
    if !branches(repo)?.iter().any(|b| b == branch) { return Err(berr("gone", branch)); }
    let remote = remote_of(repo)?;
    let r = git_for(repo, &["fetch", "--quiet", &remote, &format!("+refs/heads/{branch}:refs/remotes/{remote}/{branch}")], FETCH_SECS);
    if !r.ok { return Err(berr("offline", one_line(&r))); }
    let name = format!("{remote}/{branch}");
    let commit = git(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/remotes/{name}^{{commit}}")]).out.trim().to_string(); // the remote-tracking ref, never a local branch or tag of that name
    if commit.is_empty() { return Err(berr("git", format!("{name} could not be read"))); }
    Ok(Base { name, branch: branch.into(), commit })
}
fn one_line(r: &procs::Ran) -> String { head16(&format!("{} {}", r.err, r.out).split_whitespace().collect::<Vec<_>>().join(" "), 300).to_string() }

// Is the new worktree on the commit it was meant to be cut from? When not, the worktree and its new branch are removed.
fn verify_cut(repo: &str, dir: &Path, branch: &str, want: &str) -> Result<(), BaseErr> {
    let got = git(&dir.to_string_lossy(), &["rev-parse", "HEAD"]).out.trim().to_string();
    if got == want { return Ok(()); }
    let _ = git(repo, &["worktree", "remove", "--force", &dir.to_string_lossy()]);
    let _ = git(repo, &["branch", "-D", branch]);
    Err(berr("mismatch", format!("the worktree is on {} instead of {}", head16(&got, 10), head16(want, 10))))
}
// A new worktree for the ticket, on a new branch cut from `base` (fetched by base_of), with the work log's template in
// its self-ignoring folder. The cut is checked: HEAD must be `base.commit`. A branch of that name already there (or the
// worktree of a flow you removed but kept) is checked out instead, its log kept, but only when it was cut from the same
// branch (`prior`, what the flow recorded) or you said to use it anyway (`keep`): else `otherbase`. → the base the ticket
// really has: `base` for a new cut; for a branch kept as it is, the branch it was cut from (`prior`; when unknown, the
// one you picked, which is then where its merge request goes) with the commit they share, so the count of its own
// commits and the merge request's target are right.
pub fn create_worktree(repo: &str, dir: &Path, branch: &str, log: &str, log_file: &Path, base: &Base, prior: Option<&str>, keep: bool) -> Result<Base, BaseErr> {
    let d = dir.to_string_lossy();
    let kept = dir.exists() && git(&d, &["rev-parse", "--abbrev-ref", "HEAD"]).out.trim() == branch;
    if dir.exists() && !kept { return Err(berr("git", format!("{} is already there", dir.display()))); }
    let has = git(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).ok;
    let reuse = kept || has;
    if reuse && !keep && prior != Some(base.branch.as_str()) { return Err(berr("otherbase", prior.unwrap_or(""))); }
    if !kept {
        ignored_parent(dir);
        forget_missing(repo, branch);
        let r = if has { git(repo, &["worktree", "add", &d, branch]) } else { git(repo, &["worktree", "add", "-b", branch, &d, &base.commit]) };
        if !r.ok { return Err(berr("git", one_line(&r))); }
        if !has { verify_cut(repo, dir, branch, &base.commit)?; }
    }
    seed_log(log_file, log, kept, dir).map_err(|e| berr("git", format!("{}: {e}", log_file.display())))?;
    Ok(if reuse { reused_base(repo, branch, base, prior) } else { base.clone() })
}
// The base of a branch kept as it is: `prior` (else the picked branch) and the commit the branch shares with it.
fn reused_base(repo: &str, branch: &str, picked: &Base, prior: Option<&str>) -> Base {
    let target = prior.filter(|p| !p.is_empty()).unwrap_or(&picked.branch);
    if target == picked.branch && prior.is_some_and(|p| p == picked.branch) { return picked.clone(); }
    let remote = picked.name.strip_suffix(&format!("/{}", picked.branch)).unwrap_or("origin");
    let name = format!("{remote}/{target}");
    let tip = git(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/remotes/{name}^{{commit}}")]);
    // no remote-tracking copy of it (never fetched, or deleted since): no commit to count from, the target still named
    let commit = if tip.ok { git(repo, &["merge-base", &format!("refs/heads/{branch}"), &format!("refs/remotes/{name}")]).out.trim().to_string() } else { String::new() };
    Base { name, branch: target.to_string(), commit }
}
// Start's whole preparation, in the order that never leaves a mark on a refused start: a branch was picked (never
// guessed), it is on the remote now and fetched, only then a kept worktree is moved and the worktree made.
// `pick`: what the page sent. → the base the ticket has.
#[allow(clippy::too_many_arguments)]
pub fn prepare_start(repo: &str, wt: &Path, branch: &str, log: &str, log_file: &Path, iid: i64, path: &str, code: &str, pick: Option<&str>, prior: Option<&str>, keep: bool) -> Result<Base, BaseErr> {
    let Some(pick) = pick.map(str::trim).filter(|p| crate::settings::branch_name(p)) else { return Err(berr("nobase", "")) };
    let from = base_of(repo, pick)?;
    move_kept(repo, iid, path, code, branch, wt).map_err(|e| berr("git", e))?;
    create_worktree(repo, wt, branch, log, log_file, &from, prior, keep)
}
// The ticket's work log: `log` (the template) written to `file` in Bobix's folder, with its folder made. A worktree kept
// from an earlier flow (`kept`: the folder was still there) keeps its log: the file when there is one, else the one inside
// the worktree (`old`: `.bobix-log/work-log.md`, from before the log moved out) copied, the old one left where it is.
// Any other start is a new flow whose log begins from the template: what an earlier flow of the same ticket left in
// `file` (its log, kept after Clean up) is moved aside to `<name>.<seconds it was last written>.md`, never overwritten.
pub fn seed_log(file: &Path, log: &str, kept: bool, old: &Path) -> std::io::Result<()> {
    if kept && file.exists() { return Ok(()); }
    if let Some(d) = file.parent() { fs::create_dir_all(d)?; }
    if file.exists() {
        let at = fs::metadata(file).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
        fs::rename(file, file.with_extension(format!("{at}.md")))?;
    }
    let text = if kept { fs::read_to_string(old.join(LOG_DIR).join(LOG_FILE)).ok().map(|t| head16(&t, LOG_MAX).to_string()).unwrap_or_else(|| log.to_string()) } else { log.to_string() };
    fs::write(file, text)
}
// What would be lost with a flow's worktree: files changed and not committed, and commits no remote has (they stay on
// the branch either way). None when the folder is gone.
pub fn at_risk(dir: &str, branch: &str) -> Option<(usize, u64)> {
    if !Path::new(dir).is_dir() { return None; }
    let changed = git(dir, &["status", "--porcelain"]).out.lines().filter(|l| !l.trim().is_empty()).count();
    Some((changed, unpushed(dir, branch)))
}
// The branch a folder is on; None when detached or not a git folder.
pub fn head_branch(dir: &str) -> Option<String> {
    let r = git(dir, &["rev-parse", "--abbrev-ref", "HEAD"]);
    Some(r.out.trim().to_string()).filter(|b| r.ok && !b.is_empty() && b != "HEAD")
}
// Files changed and not committed in a repository's main folder: yours. The worktrees Claude keeps under
// .claude/worktrees are not counted (git lists that folder as untracked unless the repository ignores it).
pub fn uncommitted(dir: &str) -> usize {
    git(dir, &["status", "--porcelain", "--", ".", ":(exclude).claude/worktrees"]).out.lines().filter(|l| !l.trim().is_empty()).count()
}
// `git worktree remove`: without `force`, git refuses when there is work not committed, and says so. With it (Remove
// and delete the folder, after you saw what is not committed), that work goes; the branch and its commits stay.
pub fn remove_worktree(repo: &str, dir: &str, force: bool) -> Result<(), String> {
    if !Path::new(dir).exists() { let _ = git(repo, &["worktree", "remove", "--force", dir]); return Ok(()); } // git forgets that one only
    let r = if force { git(repo, &["worktree", "remove", "--force", dir]) } else { git(repo, &["worktree", "remove", dir]) };
    if r.ok { Ok(()) } else { Err(one_line(&r)) }
}
// The log's text, up to LOG_MAX: from the flow's file in Bobix's folder, or for a flow from before (no `log_file`) from
// its folder's `.bobix-log`. None when the file is not there (deleted by hand, or its worktree is gone).
pub fn read_log_of(f: &Flow) -> Option<String> {
    let old = |d: &str| fs::read_to_string(Path::new(d).join(LOG_DIR).join(LOG_FILE));
    // a flow from before: the folder it works in, else the worktree Bobix made when its session came up in Claude's own
    let t = if f.log_file.is_empty() { old(&f.dir).or_else(|_| old(&f.own)) } else { fs::read_to_string(&f.log_file) }.ok()?;
    Some(head16(&t, LOG_MAX).to_string())
}
// Every ticket's log lives in this folder of Bobix's own, so it outlives the worktree and the repository.
pub fn logs_dir(data: &Path) -> PathBuf { data.join("worklogs") }
// A flow's file name: its key made safe for a file name, and a short digest of the whole key, so two keys that differ
// only in the characters replaced ("a/b_c#1", "a_b/c#1") never get the same file; cut so a long group path still fits.
pub fn log_name(key: &str) -> String {
    use sha2::{Digest, Sha256};
    let stem: String = key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).take(100).collect();
    let d = Sha256::digest(key.as_bytes());
    format!("{stem}-{}.md", d[..4].iter().map(|b| format!("{b:02x}")).collect::<String>())
}
pub fn log_path(data: &Path, key: &str) -> PathBuf { logs_dir(data).join(log_name(key)) }
// A copy of a log kept for a flow from before, whose log was in its worktree (the new ones are in `worklogs/` already).
pub fn keep_log(data: &Path, f: &Flow) {
    if f.log.is_empty() || !f.log_file.is_empty() { return; }
    let d = logs_dir(data);
    if fs::create_dir_all(&d).is_ok() { let _ = fs::write(d.join(log_name(&f.key)), &f.log); }
}

// ---------- Haiku, only when you click: through your own `claude -p`, no tools, no hooks (no pet for it) ----------
fn haiku(what: &str, claude: &str, prompt: &str, schema: Option<&str>) -> Result<Value, String> {
    let mut a = vec!["-p", "--model", "haiku", "--tools", "", "--no-session-persistence", "--strict-mcp-config", "--settings", r#"{"disableAllHooks":true}"#, "--output-format", "json"];
    if let Some(s) = schema { a.extend(["--json-schema", s]); }
    a.push(prompt);
    let r = procs::exec(claude, &a, 120, &[]);
    if r.missing { return Err("claude is not installed".into()); }
    let v: Value = serde_json::from_str(r.out.trim()).map_err(|_| one_line(&r))?;
    crate::effects::own(&crate::paths::data_dir(), what, "haiku", Some(&v), crate::state::now()); // Bobix's own cost, counted
    if v["is_error"] == true { return Err(head16(v["result"].as_str().unwrap_or("claude -p failed"), 300).into()); }
    Ok(v)
}
const SCHEMA: &str = r#"{"type":"object","properties":{"size":{"enum":["small","large"]},"why":{"type":"string"},"steps":{"type":"array","items":{"type":"object","properties":{"step":{"enum":["explore","edit","review"]},"model":{"enum":["haiku","sonnet","opus"]}},"required":["step","model"]}}},"required":["size","why","steps"]}"#;
// Small or large, and which model for exploring, editing and reviewing. { size, why, steps }.
pub fn classify(claude: &str, title: &str, body: &str) -> Result<Value, String> {
    let p = format!("Classify this ticket for a Claude Code session. small: one session does it. large: split it, exploring with a cheap subagent and reviewing with a strong one. Say why in one sentence.\n\nTicket: {}\n\n{}", head16(title, 300), head16(body, 3000));
    let v = haiku("size", claude, &p, Some(SCHEMA))?;
    let o = &v["structured_output"];
    if !o["size"].is_string() { return Err("no classification in the answer".into()); }
    Ok(json!({ "size": o["size"], "why": head16(o["why"].as_str().unwrap_or(""), 400), "steps": o["steps"], "by": "haiku" }))
}
// The gaps in a log (no steps, no result), filled from what git shows of the branch. Text for you, never sent on.
pub fn summarize(claude: &str, f: &Flow) -> Result<String, String> {
    let range = format!("{}..{}", f.base, f.branch);
    let commits = git(f.git_dir(), &["log", "--format=- %s", &range]).out;
    let stat = git(f.git_dir(), &["diff", "--stat", &range]).out;
    let say = lang_name(&f.lang).map(|n| format!(" Write it in {n}; keep code, file names and commands as they are.")).unwrap_or_default();
    let p = format!("Summarize this coding session's work for its developer in at most 8 short lines: what was done, what is left or blocked. Only from the facts below.{say}\n\nWork log:\n{}\n\nCommits:\n{}\n\nChanged files:\n{}",
        head16(&f.log, 8000), head16(&commits, 3000), head16(&stat, 3000));
    let v = haiku("summary", claude, &p, None)?;
    Ok(head16(v["result"].as_str().unwrap_or("").trim(), 2000).to_string())
}

// ---------- the flows, kept in Bobix's folder (this machine only) ----------
#[derive(Default)]
pub struct Flows { pub list: indexmap::IndexMap<String, Flow>, file: Option<PathBuf> }

impl Flows {
    pub fn load(file: PathBuf) -> Flows {
        let list = fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<Vec<Flow>>(&t).ok()).unwrap_or_default();
        let mut f = Flows { list: list.into_iter().map(|f| (f.key.clone(), f)).collect(), file: Some(file) };
        f.prune();
        f
    }
    pub fn save(&mut self) {
        self.prune();
        let Some(f) = &self.file else { return };
        let v: Vec<&Flow> = self.list.values().collect();
        if let Some(d) = f.parent() { let _ = fs::create_dir_all(d); }
        let _ = crate::paths::write_private(f, &serde_json::to_string_pretty(&v).unwrap_or_default());
    }
    pub fn by_dir(&mut self, dir: &str) -> Option<&mut Flow> { self.list.values_mut().find(|f| !f.cleaned && crate::paths::same_dir(&f.dir, dir)) }
    // The flow a waiting start (Work's wait: { dir, path, iid }) belongs to: a Full start waits in its flow's worktree.
    // A Quick start has none.
    pub fn key_of_start(&self, w: &Value) -> Option<String> {
        let (dir, path, iid) = (w["dir"].as_str()?, w["path"].as_str()?, w["iid"].as_i64()?);
        self.list.values().find(|f| !f.cleaned && f.path == path && f.iid == iid && crate::paths::same_dir(f.made(), dir)).map(|f| f.key.clone())
    }
    pub fn by_session(&self, id: &str) -> Option<&Flow> { self.list.values().find(|f| f.session.as_deref() == Some(id)) }
    pub fn by_session_mut(&mut self, id: &str) -> Option<&mut Flow> { self.list.values_mut().find(|f| f.session.as_deref() == Some(id)) }

    // A hook event of the session working on a flow (or the first one started in its worktree). Returns true when the
    // flow changed. `plan`: the plan Claude asks you to approve (ExitPlanMode's input).
    pub fn on_event(&mut self, sid: &str, cwd: &str, ev: &str, tool: &str, plan: Option<&str>, pid: Option<u32>, now: u64) -> bool {
        let known = self.by_session(sid).is_some();
        let f = if known { self.by_session_mut(sid) } else { self.by_dir(cwd) };
        let Some(f) = f else { return false };
        let before = f.clone();
        if known && pid.is_some() { f.pid = pid; }
        // stopped (its session seemed gone, or Bobix restarted), and it speaks again: followed again
        if known && f.status == "stopped" && !f.cleaned { f.status = if f.plan.is_empty() { "planning" } else { "running" }.into(); }
        if !known {
            // the first session started in its worktree takes the flow; a later one (a review) only refreshes its log
            if !(ev == "SessionStart" && f.session.is_none()) { f.reread(); return if *f != before { f.at = now; true } else { false }; }
            f.session = Some(sid.into());
            f.pid = pid;
            f.status = "planning".into();
        }
        if ev == "PermissionRequest" && tool == "ExitPlanMode" {
            f.status = "approve".into();
            if let Some(p) = plan { f.plan = head16(p, 20_000).to_string(); }
        }
        // answered in Claude itself (Bobix was not asked, or restarted while it waited): the plan tool went through, so
        // it was approved; a new prompt means it was sent back
        if f.status == "approve" && ev == "PostToolUse" && tool == "ExitPlanMode" { f.status = "running".into(); }
        if ev == "UserPromptSubmit" && f.status == "approve" { f.status = "planning".into(); }
        f.reread();
        if *f != before { f.at = now; true } else { false }
    }
    // The session that took a waiting start (its first prompt named the issue: Work::claim) is the flow's, wherever
    // Claude opened it. A flow that has a session keeps it, and a session that has a flow takes no other.
    pub fn take(&mut self, key: &str, sid: &str, pid: Option<u32>, now: u64) -> bool {
        if self.by_session(sid).is_some() { return false; }
        let Some(f) = self.list.get_mut(key).filter(|f| !f.cleaned && f.session.is_none()) else { return false };
        (f.session, f.pid, f.status, f.at) = (Some(sid.into()), pid, "planning".into(), now);
        true
    }
    // Where that session runs, once git said (`top`: its folder's top; `branch`: the branch there; `yours`: in the main
    // folder, the files not committed there; in a worktree Claude made, the commits there that no remote has). In a
    // worktree Claude made, the flow works there from now on; in the repository's main folder it keeps its worktree.
    // Set once. → the place, when it changed.
    // Only for the flow's own session (`sid`): another session, or a flow removed and started again meanwhile, is left as it is.
    pub fn placed(&mut self, key: &str, sid: &str, top: &str, branch: Option<&str>, yours: u64, now: u64) -> Option<&'static str> {
        let f = self.list.get_mut(key).filter(|f| !f.cleaned && f.place.is_empty() && f.session.as_deref() == Some(sid))?;
        let place = place_of(f, top).filter(|p| !p.is_empty())?;
        if place == "claude" {
            f.own = std::mem::replace(&mut f.dir, top.into());
            if let Some(b) = branch { f.branch = b.into(); }
            f.carried = yours;
        } else { f.mixed = yours as usize; }
        (f.place, f.at) = (place.into(), now);
        f.reread();
        Some(place)
    }
    // A flow in a worktree Claude made follows that folder's branch (Claude names it; the session may make another).
    pub fn branch_at(&mut self, top: &str, branch: &str) -> bool {
        let mut changed = false;
        for f in self.list.values_mut().filter(|f| !f.cleaned && f.place == "claude" && f.branch != branch && crate::paths::same_dir(&f.dir, top)) { f.branch = branch.into(); changed = true; }
        changed
    }
    // Starts whose session has not come yet: they wait again after Bobix restarts (the waits are kept in memory only).
    pub fn waiting(&self) -> impl Iterator<Item = &Flow> { self.list.values().filter(|f| !f.cleaned && f.status == "starting" && f.session.is_none()) }
    // The plan was answered from Bobix: approved (running) or sent back with changes (planning).
    pub fn answered(&mut self, sid: &str, approved: bool, now: u64) {
        if let Some(f) = self.by_session_mut(sid).filter(|f| f.status == "approve") { f.status = if approved { "running" } else { "planning" }.into(); f.at = now; }
    }
    // The pipeline read for a flow's MR; `since` keeps when it turned red (for "CI red for …"). True when it changed.
    pub fn set_ci(&mut self, key: &str, mut ci: Value, now: u64) -> bool {
        let Some(f) = self.list.get_mut(key) else { return false };
        let same = |a: &Value, b: &Value| a["status"] == b["status"] && a["url"] == b["url"] && a["state"] == b["state"];
        if same(&f.ci, &ci) { f.ci["at"] = json!(now); return false; }
        ci["since"] = if ci["status"] == "failed" && f.ci["status"] == "failed" { f.ci["since"].clone() } else { json!(now) };
        ci["at"] = json!(now);
        f.ci = ci;
        true
    }
    // Flows whose MR is still open: their pipeline is worth reading again.
    // → (key, host, project path, number, github)
    pub fn open_mrs(&self) -> Vec<(String, String, String, i64, bool)> {
        self.list.values().filter(|f| !f.mr.is_empty() && !matches!(f.ci["state"].as_str(), Some("merged" | "closed")))
            .filter_map(|f| Some((f.key.clone(), tauri::Url::parse(&f.mr).ok()?.host_str()?.to_string(), f.code_repo().to_string(), mr_iid(&f.mr)?, f.github()))).collect()
    }
    // A new assumption since you last looked: the pet gets a mark (the session does not stop).
    pub fn marked(&self, sid: &str) -> bool { self.by_session(sid).is_some_and(|f| !f.cleaned && assumptions(&f.log) > f.seen) }

    // What the pages get with every update: each flow without its texts (the log, the plan, the summary, the MR's draft:
    // up to ~150 KB each, sent on every hook event before), with what the list shows of its log; and how much disk is
    // left for more. A flow's texts are read when its details are open (detail).
    pub fn view(&self, free: Option<u64>) -> Value {
        let list: Vec<Value> = self.list.values().rev().map(|f| {
            let mut v = serde_json::to_value(f).unwrap_or_default();
            if let Some(o) = v.as_object_mut() { for k in ["log", "plan", "summary"] { o.remove(k); } }
            let log = parse_log(&f.log);
            v["new"] = json!(log["assumptions"].as_array().map_or(0, |a| a.len()).saturating_sub(f.seen));
            v["parsed"] = json!({ "status": log["status"], "ran": log["ran"], "reviewed": log["reviewed"] });
            v["cmd"] = json!(if f.cleaned || f.session.is_some() { String::new() } else { cli_command(&f.dir, &main_model(&f.models), "", f.log_dir().as_deref()) });
            v
        }).collect();
        json!({ "list": list, "free": free, "room": room(free), "need": WORKTREE_BYTES })
    }
    // Its log read again when its details open: a session that is not the flow's own and works in another folder (a
    // review Claude opened in the main folder) writes to it without an event the flow sees. True when it changed.
    pub fn refresh(&mut self, key: &str, now: u64) -> bool {
        let Some(f) = self.list.get_mut(key).filter(|f| !f.cleaned) else { return false };
        let before = (f.log.clone(), f.status.clone(), f.log_missing);
        f.reread();
        let changed = before != (f.log.clone(), f.status.clone(), f.log_missing);
        if changed { f.at = now; }
        changed
    }
    // A flow's texts, for its details: its log read into sections, the plan, the summary, and the MR's draft.
    pub fn detail(&self, key: &str) -> Option<Value> {
        let f = self.list.get(key)?;
        let mut v = json!({ "key": f.key, "at": f.at, "parsed": parse_log(&f.log), "plan": f.plan, "summary": f.summary });
        if matches!(f.status.as_str(), "done" | "blocked") && !f.cleaned && f.mr.is_empty() { v["mrTitle"] = json!(mr_title(f)); v["mrBody"] = json!(mr_body(f)); v["mrReady"] = json!(mr_ready(f)); }
        Some(v)
    }
    // Flows whose session ended before done or blocked (it left Bobix's list and its claude process is gone; after a
    // restart Bobix knows no session until it speaks, so only the process counts): stopped, with Remove offered. Its log
    // is read once more first (it may have set done just before). True when one changed.
    pub fn stop_gone(&mut self, listed: impl Fn(&str) -> bool, alive: impl Fn(u32) -> bool, now: u64) -> bool {
        let mut changed = false;
        for f in self.list.values_mut().filter(|f| !f.cleaned && matches!(f.status.as_str(), "planning" | "approve" | "running")) {
            let Some(sid) = f.session.as_deref() else { continue };
            if listed(sid) || f.pid.is_some_and(&alive) { continue; }
            f.reread();
            if matches!(f.status.as_str(), "done" | "blocked") { f.at = now; changed = true; continue; }
            f.status = "stopped".into();
            f.at = now;
            changed = true;
        }
        changed
    }
    // Cleaned flows are history: the newest CLEANED_KEEP stay (each one's log is in worklogs/), older ones go.
    fn prune(&mut self) {
        let mut cleaned: Vec<(u64, String)> = self.list.values().filter(|f| f.cleaned).map(|f| (f.at, f.key.clone())).collect();
        if cleaned.len() <= CLEANED_KEEP { return; }
        cleaned.sort();
        for (_, k) in &cleaned[..cleaned.len() - CLEANED_KEEP] { self.list.shift_remove(k); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_branches_and_folders() {
        assert_eq!(slug("[Bug] Cart total ignores discount codes!"), "bug-cart-total-ignores-discount-codes");
        assert_eq!(slug("Sửa lỗi đăng nhập"), "s-a-l-i-ng-nh-p");
        assert_eq!(branch_for(7, "Add subtract()", "g/shop", ""), "7-add-subtract");
        assert_eq!(branch_for(8, "€€", "g/shop", "g/shop"), "8-ticket");
        assert!(slug(&"word ".repeat(40)).len() <= 40);
        // the worktree: in the repository, named after it and the issue, and not where Claude desktop's link handler
        // would cut the folder down to the repository (`<repo>/.claude/worktrees/<name>`)
        assert_eq!(worktree_dir("/src/shop", 7, "g/shop", ""), Path::new("/src/shop/.bobix/worktrees/shop-7"));
        assert_eq!(worktree_dir("/src/Web Shop/", 7, "g/shop", ""), Path::new("/src/Web Shop/.bobix/worktrees/web-shop-7"));
        assert!(worktree_dir("C:\\src\\shop", 7, "g/shop", "").to_string_lossy().replace('\\', "/").ends_with("shop/.bobix/worktrees/shop-7"), "a Windows folder gives the same name");
        assert!(!worktree_dir("/src/shop", 7, "g/shop", "").to_string_lossy().contains(".claude"));
        assert_eq!(crate::model::issue_of_branch(&branch_for(42, "x", "g/shop", "")), Some(42), "the branch names its issue the way Bobix reads branches");
        // the code in another repository: names that are not taken for that repository's own issue 7
        assert_eq!(branch_for(7, "Add subtract()", "demo/Tracker", "demo/service"), "tracker-7-add-subtract");
        assert_eq!(crate::model::issue_of_branch("tracker-7-add-subtract"), None);
        assert_eq!(worktree_dir("/src/service", 7, "demo/tracker", "demo/service"), Path::new("/src/service/.bobix/worktrees/service-tracker-7"));
        assert_eq!((flow_key("g/app", 7, ""), flow_key("g/app", 7, "g/app"), flow_key("g/app", 7, "g/api")), ("g/app#7".into(), "g/app#7".into(), "g/app#7>g/api".into()));
        assert_eq!(room(Some(WORKTREE_BYTES * 2 + 1)), Some(2));
        assert_eq!(room(Some(WORKTREE_BYTES - 1)), Some(0));
        assert!(free_bytes(Path::new(env!("CARGO_MANIFEST_DIR"))).is_some_and(|b| b > 0) || cfg!(windows));
    }

    #[test]
    fn the_log_reads_back_section_by_section() {
        let t = template(7, "Add subtract", "en");
        assert_eq!(parse_log(&t), json!({ "status": "planning", "ticket": "", "questions": [], "later": [], "mapping": "", "plan": "", "steps": [], "assumptions": [], "result": "", "checks": [], "ran": 0, "reviewed": false, "checksText": "", "mr": "" }));
        let filled = t.replace("Status: planning", "Status: done (all tests pass)")
            .replace("## Plan\n", "## Plan\nAdd `subtract` next to `add`.\n")
            .replace("## Steps\n", "## Steps\n- Read math.js\n- Added subtract\n")
            .replace("## Assumptions\n", "## Assumptions\n- Integers only\n* No new test file\nfree text is not an assumption\n")
            .replace("## Result\n", "## Result\nDone: subtract(5, 3) is 2.\n");
        let p = parse_log(&filled);
        assert_eq!(p["status"], "done");
        assert_eq!(p["steps"], json!(["Read math.js", "Added subtract"]));
        assert_eq!(p["assumptions"], json!(["Integers only", "No new test file"]));
        assert_eq!(p["result"], "Done: subtract(5, 3) is 2.");
        assert_eq!(parse_log("Status: maybe\n")["status"], "");
        assert_eq!(assumptions(&filled), 2);
        assert_eq!((&p["ran"], &p["reviewed"]), (&json!(0), &json!(false)), "done, but no checks");
        // checks: one line per command, its output in a code block (not items), a review line; "None: …" is not a run
        let checked = filled.replace("## Checks\n", "## Checks\n- `npm test`\n```\n- 2 passing\n## not a heading\n```\n- `npm run lint`: clean\n- Review: /code-review, 1 issue fixed\n");
        let c = parse_log(&checked);
        assert_eq!(c["checks"], json!(["`npm test`", "`npm run lint`: clean", "Review: /code-review, 1 issue fixed"]));
        assert_eq!((&c["ran"], &c["reviewed"], &c["result"]), (&json!(2), &json!(true), &json!("Done: subtract(5, 3) is 2.")));
        assert!(c["checksText"].as_str().unwrap().starts_with("- `npm test`\n```\n- 2 passing"), "{}", c["checksText"]);
        let none = parse_log(&filled.replace("## Checks\n", "## Checks\n- None: no tests in this repo\n"));
        assert_eq!((&none["ran"], &none["checks"]), (&json!(0), &json!(["None: no tests in this repo"])));
        let plan = parse_log("## Plan\n```\ncode\n```\n## Steps\n- a\n");
        assert_eq!((&plan["plan"], &plan["steps"]), (&json!("```\ncode\n```"), &json!(["a"])), "code in the plan stays");
    }

    #[test]
    fn the_interview_is_kept_in_the_log() {
        let log = template(7, "Add subtract", "en")
            .replace("## Ticket\n", "## Ticket\nGoal: subtract. Not: divide. Done: tests pass.\n")
            .replace("## Questions\n", "## Questions\n- [scope] Negative results? (ticket: \"any numbers\") -> yes\nfree text is not a question\n")
            .replace("## Later\n", "## Later\n- [approach] Rename add (math.js:3)\n")
            .replace("## Mapping\n", "## Mapping\n| Requirement | Step |\n|---|---|\n| subtract | 1 |\n\n## x\n");
        let p = parse_log(&log);
        assert_eq!(p["ticket"], "Goal: subtract. Not: divide. Done: tests pass.");
        assert_eq!(p["questions"], json!(["[scope] Negative results? (ticket: \"any numbers\") -> yes"]));
        assert_eq!(p["later"], json!(["[approach] Rename add (math.js:3)"]));
        assert_eq!(p["mapping"], "| Requirement | Step |\n|---|---|\n| subtract | 1 |");
        // numbered questions are kept; a Mapping given twice shows once (the last)
        let q = parse_log("## Questions\n1. [scope] a (x) -> y\n2) [edge case] b (z) -> w\nfree\n## Mapping\n| old |\n## Mapping\n| new |\n");
        assert_eq!((&q["questions"], &q["mapping"]), (&json!(["[scope] a (x) -> y", "[edge case] b (z) -> w"]), &json!("| new |")));
    }

    #[test]
    fn the_full_start_interviews_before_the_plan() {
        let r = rules("7-add-subtract", "/d", "/data/worklogs/l.md", "en");
        for must in ["Restate the ticket", "wait for my answer", "AskUserQuestion", "[scope], [approach], [edge case] or [done criteria]", "Out of scope for this ticket", "No source, no question", "about 8 questions", "\"No questions, because ...\"", "up to you", "`## Questions`", "`## Later`", "\"Mapping\"", "`## Mapping`"] {
            assert!(r.contains(must), "{must}");
        }
        assert!(!r.contains("because nobody can answer now") && !r.contains(".bobix-log"), "the log is not in the worktree");
        // order: restate, then confirm, then the questions, then the plan with its mapping
        let at = |k: &str| r.find(k).unwrap_or_else(|| panic!("{k}"));
        assert!(at("Restate the ticket") < at("confirm it with the AskUserQuestion") && at("confirm it with the AskUserQuestion") < at("ask with the AskUserQuestion tool, in rounds") && at("ask with the AskUserQuestion tool, in rounds") < at("\"Mapping\"") && at("\"Mapping\"") < at("Approving that plan"));
        assert!(r.contains("`## Ticket`, `## Questions`, `## Later` and `## Mapping` into the work log in one go"));
        // Bobix's language: the interview is in it too, the tags stay English
        let vi = rules("7-add-subtract", "/d", "/data/worklogs/l.md", "vi");
        assert!(vi.contains("restate the ticket and ask every question in Vietnamese") && vi.contains("translate that one") && vi.contains("tags `[scope]`"));
        assert!(!r.contains("ask every question in"), "English has no language rule");
    }

    #[test]
    fn prompt_parts() {
        let r = rules("7-add-subtract", "/src/shop/.bobix/worktrees/shop-7", "/data/Application Support/Bobix/worklogs/g_shop_7-1a2b3c4d.md", "en");
        // the ticket's worktree is named, and what to do when Claude opened the session somewhere else
        for must in ["`/src/shop/.bobix/worktrees/shop-7`, on branch `7-add-subtract`", "a folder under `.claude/worktrees`), stay in that worktree", "run `cd` into `/src/shop/.bobix/worktrees/shop-7` before anything else", "already changed before you started are not this ticket's", "`/data/Application Support/Bobix/worklogs/g_shop_7-1a2b3c4d.md` (the full path", "do not start another one in the repository or a worktree"] {
            assert!(r.contains(must), "{must}");
        }
        for must in ["EnterPlanMode", "ExitPlanMode", "Assumptions", "pushing to the main branch", "deleting data", "calling hosts", "touching secrets", "worklogs/g_shop_7-1a2b3c4d.md", "`7-add-subtract`", "## Checks", "tests, build and lint", "`- None: `", "/code-review", "`- Review: `"] {
            assert!(r.contains(must), "{must}");
        }
        assert!(models_text(&json!({ "size": "small" })).contains("one session"));
        let big = json!({ "size": "large", "steps": [{ "step": "explore", "model": "haiku" }, { "step": "edit", "model": "sonnet" }, { "step": "review", "model": "opus" }, { "step": "x", "model": "gpt" }] });
        let t = models_text(&big);
        assert!(t.contains("built-in Explore subagent") && t.contains("`sonnet` suits") && t.contains("model `opus`") && !t.contains("gpt"), "{t}");
        assert!(models_text(&json!({ "size": "large", "steps": [{ "step": "explore", "model": "sonnet" }] })).contains("read-only subagent on model `sonnet`"));
        let rp = review_prompt("7-add-subtract", "origin/HEAD", "/src/shop/.bobix/worktrees/shop-7", "/data/Application Support/Bobix/worklogs/g_shop_7-1a2b3c4d.md", "");
        assert!(rp.contains("`git diff origin/HEAD...7-add-subtract`") && rp.contains("work log `/data/Application Support/Bobix/worklogs/g_shop_7-1a2b3c4d.md`") && !rp.contains(".bobix-log") && rp.contains("/code-review") && rp.contains("Do not change any code") && rp.contains("- Review (second session): "), "{rp}");
        assert_eq!(cli_in_mode("/w", "opus", "auto", "r", None), "cd '/w' && claude --model 'opus' --permission-mode auto 'r'");
        // the log's folder is added with `=`, so the prompt after it is not read as one more folder; spaces and quotes survive
        assert_eq!(cli_in_mode("/w", "opus", "auto", "r", Some("/d/Application Support/Bobix/worklogs")), "cd '/w' && claude --model 'opus' --permission-mode auto --add-dir='/d/Application Support/Bobix/worklogs' 'r'");
        assert_eq!(cli_in_mode("/w", "opus", "plan", "", Some("C:\\Users\\An's PC\\AppData\\Roaming\\Bobix\\worklogs")), r#"cd '/w' && claude --model 'opus' --permission-mode plan --add-dir='C:\Users\An'\''s PC\AppData\Roaming\Bobix\worklogs'"#);
        assert_eq!((main_model(&big), main_model(&json!({}))), ("sonnet".to_string(), "sonnet".to_string()));
        assert_eq!(cli_command("/src/it's", "opus", "do 'x'", None), r#"cd '/src/it'\''s' && claude --model 'opus' --permission-mode plan 'do '\''x'\'''"#);
    }

    #[test]
    fn a_flow_goes_plan_approve_run_done_and_marks_new_assumptions() {
        let root = std::env::temp_dir().join(format!("bobix-flow-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let dir = root.join("wt");
        fs::create_dir_all(dir.join(LOG_DIR)).unwrap();
        let write = |t: &str| fs::write(dir.join(LOG_DIR).join(LOG_FILE), t).unwrap();
        write(&template(7, "Add subtract", "en"));
        let d = dir.to_string_lossy().to_string();
        let mut fl = Flows::default();
        fl.list.insert("g/shop#7".into(), Flow { key: "g/shop#7".into(), path: "g/shop".into(), iid: 7, dir: d.clone(), status: "starting".into(), ..Default::default() });
        assert!(!fl.on_event("s1", "/elsewhere", "SessionStart", "", None, None, 1), "another folder");
        fl.on_event("s1", &d, "PreToolUse", "Read", None, None, 1);
        assert!(fl.list[0].session.is_none() && fl.list[0].status == "starting", "only a session that starts there takes it");
        assert!(fl.on_event("s1", &d, "SessionStart", "", None, None, 2));
        assert_eq!((fl.list[0].status.as_str(), fl.list[0].session.as_deref()), ("planning", Some("s1")));
        assert!(!fl.on_event("s2", &d, "SessionStart", "", None, None, 3), "taken: a second session there is not the flow's");
        assert!(!fl.on_event("s2", &d, "PermissionRequest", "ExitPlanMode", Some("other"), None, 3) && fl.list[0].status == "planning", "its plan is not the flow's");
        assert!(fl.on_event("s1", &d, "PermissionRequest", "ExitPlanMode", Some("1. add it\n## Assumptions\n- ints"), None, 4));
        assert_eq!((fl.list[0].status.as_str(), fl.list[0].plan.as_str()), ("approve", "1. add it\n## Assumptions\n- ints"));
        fl.answered("s1", true, 5);
        assert_eq!(fl.list[0].status, "running");
        assert!(!fl.marked("s1"));
        write(&template(7, "Add subtract", "en").replace("## Assumptions\n", "## Assumptions\n- Integers only\n"));
        assert!(fl.on_event("s1", &d, "PostToolUse", "Edit", None, None, 6));
        assert!(fl.marked("s1"), "a new assumption marks the pet");
        fl.list[0].seen = 1;
        assert!(!fl.marked("s1"), "seen");
        write(&fl.list[0].log.replace("Status: planning", "Status: done"));
        assert!(fl.on_event("s1", &d, "Stop", "", None, None, 7));
        assert_eq!(fl.list[0].status, "done");
        // a review session in the worktree notes its result: the flow's log is read again on its events
        write(&fl.list[0].log.replace("## Checks\n", "## Checks\n- Review (second session): no issues\n"));
        assert!(fl.on_event("r1", &d, "Stop", "", None, None, 8) && fl.list[0].log.contains("second session") && fl.list[0].session.as_deref() == Some("s1"));
        let v = fl.view(Some(WORKTREE_BYTES * 3));
        assert_eq!((&v["room"], &v["list"][0]["parsed"]["status"], &v["list"][0]["new"]), (&json!(3), &json!("done"), &json!(0)));
        // sent back with changes
        fl.list[0].status = "approve".into();
        fl.answered("s1", false, 8);
        assert_eq!(fl.list[0].status, "planning");
        // kept across restarts
        let file = root.join("tickets.json");
        let mut a = Flows::load(file.clone());
        a.list = fl.list.clone();
        a.save();
        assert_eq!(Flows::load(file).list, fl.list);
        let _ = fs::remove_dir_all(&root);
    }

    // A session that ends (or a Bobix restart that finds its claude process gone) before done or blocked leaves its
    // flow stopped, never hanging; the same session speaking again picks it up. Its log is read once more first.
    #[test]
    fn a_flow_whose_session_is_gone_is_stopped() {
        let root = std::env::temp_dir().join(format!("bobix-stop-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let dir = root.join("wt");
        fs::create_dir_all(dir.join(LOG_DIR)).unwrap();
        fs::write(dir.join(LOG_DIR).join(LOG_FILE), template(7, "x", "en")).unwrap();
        let d = dir.to_string_lossy().to_string();
        let mut fl = Flows::default();
        for (k, st) in [("a#1", "planning"), ("a#2", "running"), ("a#3", "starting"), ("a#4", "done")] {
            fl.list.insert(k.into(), Flow { key: k.into(), dir: d.clone(), status: st.into(), session: (st != "starting").then(|| format!("s-{k}")), pid: Some(4242), ..Default::default() });
        }
        assert!(!fl.stop_gone(|_| true, |_| false, 1), "listed: still followed");
        assert!(!fl.stop_gone(|_| false, |_| true, 1), "after a restart, its claude process still runs: still followed");
        assert!(fl.stop_gone(|_| false, |_| false, 2));
        let st: Vec<&str> = fl.list.values().map(|f| f.status.as_str()).collect();
        assert_eq!(st, ["stopped", "stopped", "starting", "done"], "a flow with no session yet waits; done stays done");
        assert!(fl.on_event("s-a#1", &d, "UserPromptSubmit", "", None, Some(99), 3) && fl.list[0].status == "planning" && fl.list[0].pid == Some(99), "it speaks again");
        // it set done just before it ended: done, not stopped
        fs::write(dir.join(LOG_DIR).join(LOG_FILE), template(7, "x", "en").replace("Status: planning", "Status: done")).unwrap();
        fl.list[0].pid = None;
        assert!(fl.stop_gone(|_| false, |_| false, 4) && fl.list[0].status == "done");
        let _ = fs::remove_dir_all(&root);
    }

    // The pages get each flow without its texts (sent on every hook event, they were the bulk of it); a flow's details
    // read them. Cleaned flows past CLEANED_KEEP leave the list (their logs stay in worklogs/).
    #[test]
    fn the_pages_get_flows_without_their_texts() {
        let big = format!("{}\n## Steps\n{}", template(7, "x", "en").replace("Status: planning", "Status: done").split("## Steps").next().unwrap(), "- step\n".repeat(8000));
        let mut fl = Flows::default();
        for n in 0..(CLEANED_KEEP + 30) {
            let k = format!("g/a#{n}");
            fl.list.insert(k.clone(), Flow { key: k, iid: n as i64, log: big.clone(), plan: "p".repeat(20_000), status: "done".into(), cleaned: n < CLEANED_KEEP + 25, at: n as u64, ..Default::default() });
        }
        let v = fl.view(None);
        let one = &v["list"][0];
        assert!(one.get("log").is_none() && one.get("plan").is_none() && one.get("mrBody").is_none());
        assert_eq!((&one["parsed"]["status"], &one["parsed"]["ran"]), (&json!("done"), &json!(0)));
        assert!(v.to_string().len() < 50 * 1024, "{} bytes for {} flows", v.to_string().len(), fl.list.len());
        let d = fl.detail("g/a#49").unwrap();
        assert_eq!(d["parsed"]["steps"].as_array().unwrap().len(), 8000);
        assert!(d["mrBody"].as_str().unwrap().starts_with("Closes #49") && d["plan"].as_str().unwrap().len() == 20_000);
        assert!(fl.detail("nope").is_none());
        let root = std::env::temp_dir().join(format!("bobix-prune-{}", std::process::id()));
        let _ = fs::create_dir_all(&root);
        let mut saved = Flows::load(root.join("tickets.json"));
        saved.list = fl.list.clone();
        saved.save();
        let cleaned: Vec<i64> = saved.list.values().filter(|f| f.cleaned).map(|f| f.iid).collect();
        assert_eq!(cleaned, (25..CLEANED_KEEP as i64 + 25).collect::<Vec<_>>(), "the newest cleaned ones stay");
        assert_eq!(saved.list.values().filter(|f| !f.cleaned).count(), 5, "flows not cleaned all stay");
        let _ = fs::remove_dir_all(&root);
    }

    // A work log's file for a test worktree: in a `logs` folder next to the test's own folders, outside every worktree.
    fn lf(dir: &Path) -> PathBuf {
        let mut top = dir;
        while let Some(p) = top.parent().filter(|p| *p != std::env::temp_dir()) { top = p; }
        top.join("logs").join(format!("{}.md", dir.file_name().unwrap().to_string_lossy()))
    }
    // Cut `branch`'s worktree from the remote branch `from` the way Start does.
    fn cut_from(repo: &str, dir: &Path, branch: &str, from: &str) -> Result<Base, BaseErr> {
        let b = base_of(repo, from)?;
        create_worktree(repo, dir, branch, "log", &lf(dir), &b, Some(from), false).map(|_| b)
    }

    // Full start from a picked branch: the list comes from the remote now, the branch is fetched now and the worktree
    // is cut from origin/<branch> (not from a local copy that is behind). Offline, a branch that is gone, no remote:
    // stopped with why, never a stale copy or the default branch. A worktree kept from a removed flow is used again only
    // for the branch it was cut from.
    #[test]
    fn a_worktree_starts_from_the_picked_fetched_branch() {
        let root = std::env::temp_dir().join(format!("bobix-base-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let r = |p: &Path| p.to_string_lossy().into_owned();
        let g = |dir: &Path, a: &[&str]| { let x = git(&r(dir), a); assert!(x.ok, "{a:?}: {}", x.err); x.out };
        let commit = |dir: &Path, m: &str| g(dir, &["-c", "user.name=t", "-c", "user.email=t@example.test", "commit", "-q", "--allow-empty", "-m", m]);
        let (up, me) = (root.join("up"), root.join("me"));
        fs::create_dir_all(&up).unwrap();
        g(&up, &["init", "-q", "-b", "trunk"]);
        commit(&up, "one");
        g(&up, &["branch", "develop"]);
        assert!(procs::exec("git", &["clone", "-q", &r(&up), &r(&me)], 30, &[]).ok);
        // the remote moves on after the clone: develop gets a commit, and the local develop stays behind
        g(&me, &["branch", "develop", "origin/develop"]);
        g(&up, &["checkout", "-q", "develop"]);
        commit(&up, "two on develop");
        let new = g(&up, &["rev-parse", "develop"]).trim().to_string();
        let old = g(&me, &["rev-parse", "develop"]).trim().to_string();
        assert_ne!(new, old);
        // the list is the remote's, asked now
        assert_eq!(branches(&r(&me)).unwrap(), ["develop", "trunk"]);
        let b = base_of(&r(&me), "develop").unwrap();
        assert_eq!((b.name.as_str(), b.branch.as_str(), b.commit.as_str()), ("origin/develop", "develop", new.as_str()), "fetched now: the newest commit, not the local copy");
        let wt = root.join("me/.claude/worktrees/bobix-7");
        assert_eq!(cut_from(&r(&me), &wt, "7-x", "develop").unwrap().commit, new);
        assert_eq!(g(&wt, &["rev-parse", "HEAD"]).trim(), new, "HEAD is the commit of origin/develop");
        // a local branch that is called like the remote one must not win over the remote-tracking branch
        g(&me, &["branch", "origin/develop", &old]);
        assert_eq!(base_of(&r(&me), "develop").unwrap().commit, new, "the remote-tracking ref, not a local branch named origin/develop");
        assert_eq!(fs::read_to_string(lf(&wt)).unwrap(), "log", "the log is made outside the worktree");
        assert!(!wt.join(LOG_DIR).exists(), "and no .bobix-log folder in it");
        fs::write(lf(&wt), "my notes").unwrap();
        assert!(cut_from(&r(&me), &wt, "7-x", "develop").is_ok(), "kept from a removed flow, cut from the same branch: used again");
        assert_eq!(fs::read_to_string(lf(&wt)).unwrap(), "my notes", "its log kept");
        // cut from another branch: not silently reused; said which, and the person can keep it
        let t = base_of(&r(&me), "trunk").unwrap();
        let e = create_worktree(&r(&me), &wt, "7-x", "log", &lf(&wt), &t, Some("develop"), false).unwrap_err();
        assert_eq!((e.kind, e.detail.as_str()), ("otherbase", "develop"));
        assert!(create_worktree(&r(&me), &wt, "7-x", "log", &lf(&wt), &t, Some("develop"), true).is_ok(), "use it anyway");
        assert_eq!(create_worktree(&r(&me), &wt, "7-x", "log", &lf(&wt), &t, None, false).unwrap_err().kind, "otherbase", "a branch whose origin is not known is asked about too");
        assert!(cut_from(&r(&me), &wt, "8-other", "develop").is_err(), "a folder there for another branch: refused");
        // a branch that is not on the remote (any more)
        assert_eq!(base_of(&r(&me), "nope").unwrap_err().kind, "gone");
        // offline: the remote is gone; no stale list, no stale copy
        fs::rename(&up, root.join("up-away")).unwrap();
        assert_eq!(branches(&r(&me)).unwrap_err().kind, "offline");
        assert_eq!(base_of(&r(&me), "develop").unwrap_err().kind, "offline");
        // no remote
        let lone = root.join("lone");
        fs::create_dir_all(&lone).unwrap();
        g(&lone, &["init", "-q"]);
        commit(&lone, "x");
        assert_eq!(branches(&r(&lone)).unwrap_err().kind, "noremote");
        let _ = fs::remove_dir_all(&root);
    }

    // A branch kept as it is keeps the branch it was cut from (so its merge request goes there) and the commit it shares
    // with it; when that is not known the picked branch is used, and the commit it shares with that one.
    #[test]
    fn a_kept_branch_keeps_the_base_it_was_cut_from() {
        let root = std::env::temp_dir().join(format!("bobix-keep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let r = |p: &Path| p.to_string_lossy().into_owned();
        let g = |dir: &Path, a: &[&str]| { let x = git(&r(dir), a); assert!(x.ok, "{a:?}: {}", x.err); x.out };
        let commit = |dir: &Path, m: &str| g(dir, &["-c", "user.name=t", "-c", "user.email=t@example.test", "commit", "-q", "--allow-empty", "-m", m]);
        let (up, me) = (root.join("up"), root.join("me"));
        fs::create_dir_all(&up).unwrap();
        g(&up, &["init", "-q", "-b", "main"]);
        commit(&up, "old release");
        g(&up, &["checkout", "-q", "-b", "develop"]);
        commit(&up, "new work");
        assert!(procs::exec("git", &["clone", "-q", &r(&up), &r(&me)], 30, &[]).ok);
        let main_tip = g(&up, &["rev-parse", "main"]).trim().to_string();
        let develop_tip = g(&up, &["rev-parse", "develop"]).trim().to_string();
        let wt = root.join("me/.bobix/worktrees/me-7");
        // the ticket started from main (an old start), with a commit of its own
        cut_from(&r(&me), &wt, "7-x", "main").unwrap();
        commit(&wt, "own work");
        let develop = base_of(&r(&me), "develop").unwrap();
        // the flow recorded main: keeping it gives main, and the commit shared with main
        let k = create_worktree(&r(&me), &wt, "7-x", "log", &lf(&wt), &develop, Some("main"), true).unwrap();
        assert_eq!((k.branch.as_str(), k.name.as_str(), k.commit.as_str()), ("main", "origin/main", main_tip.as_str()), "the merge request goes where it was cut from, not into the pick");
        assert_eq!(git(&r(&me), &["rev-list", "--count", &format!("{}..7-x", k.commit)]).out.trim(), "1", "its own commits are counted from there");
        // nothing recorded (a flow from before): the picked branch, said in the page as the merge request's target
        let k = create_worktree(&r(&me), &wt, "7-x", "log", &lf(&wt), &develop, None, true).unwrap();
        assert_eq!((k.branch.as_str(), k.commit.as_str()), ("develop", main_tip.as_str()), "the commit it shares with develop, which is where main was");
        assert_ne!(k.commit, develop_tip);
        // cut from the picked branch already: the picked base as it is
        let same = create_worktree(&r(&me), &wt, "7-x", "log", &lf(&wt), &develop, Some("develop"), false).unwrap();
        assert_eq!(same, develop);
        // a recorded branch that has no remote-tracking copy: still named, no commit to count from
        let gone = create_worktree(&r(&me), &wt, "7-x", "log", &lf(&wt), &develop, Some("retired"), true).unwrap();
        assert_eq!((gone.branch.as_str(), gone.commit.as_str()), ("retired", ""));
        let _ = fs::remove_dir_all(&root);
    }

    // Start is refused before anything on disk moves: no branch picked (never a guess), a branch the remote does not
    // have. A kept worktree in the old place stays where it is until the start is allowed.
    #[test]
    fn a_refused_start_moves_nothing() {
        let root = std::env::temp_dir().join(format!("bobix-nobase-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let r = |p: &Path| p.to_string_lossy().into_owned();
        let g = |dir: &Path, a: &[&str]| { let x = git(&r(dir), a); assert!(x.ok, "{a:?}: {}", x.err); x.out };
        let (up, me) = (root.join("up"), root.join("me"));
        fs::create_dir_all(&up).unwrap();
        g(&up, &["init", "-q", "-b", "develop"]);
        g(&up, &["-c", "user.name=t", "-c", "user.email=t@example.test", "commit", "-q", "--allow-empty", "-m", "one"]);
        assert!(procs::exec("git", &["clone", "-q", &r(&up), &r(&me)], 30, &[]).ok);
        // a worktree of the ticket's branch kept in the old place
        let old = me.join(".claude/worktrees/bobix-7");
        g(&me, &["worktree", "add", "-q", "-b", "7-x", &r(&old)]);
        let wt = worktree_dir(&r(&me), 7, "g/r", "");
        for pick in [None, Some(""), Some("   "), Some("-x"), Some("a..b")] {
            let e = prepare_start(&r(&me), &wt, "7-x", "log", &lf(&wt), 7, "g/r", "", pick, None, false).unwrap_err();
            assert_eq!(e.kind, "nobase", "{pick:?}: no branch picked, nothing started");
        }
        let e = prepare_start(&r(&me), &wt, "7-x", "log", &lf(&wt), 7, "g/r", "", Some("gone"), None, false).unwrap_err();
        assert_eq!(e.kind, "gone");
        assert!(old.is_dir() && !wt.exists(), "the kept worktree was not moved by a refused start");
        // allowed: the kept worktree moves, and is used again for the same branch
        let b = prepare_start(&r(&me), &wt, "7-x", "log", &lf(&wt), 7, "g/r", "", Some("develop"), Some("develop"), false).unwrap();
        assert_eq!(b.branch, "develop");
        assert!(wt.is_dir() && !old.exists());
        let _ = fs::remove_dir_all(&root);
    }

    // After the cut Bobix checks HEAD is the commit of origin/<branch>; a mismatch removes the worktree and its branch.
    #[test]
    fn a_cut_on_the_wrong_commit_is_removed() {
        let root = std::env::temp_dir().join(format!("bobix-head-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let repo = root.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let r = repo.to_string_lossy().into_owned();
        for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.name=t", "-c", "user.email=t@example.test", "commit", "-q", "--allow-empty", "-m", "one"]] { assert!(git(&r, a).ok); }
        let dir = root.join("wt");
        assert!(git(&r, &["worktree", "add", "-q", "-b", "7-x", &dir.to_string_lossy()]).ok);
        let head = git(&r, &["rev-parse", "HEAD"]).out.trim().to_string();
        assert_eq!(verify_cut(&r, &dir, "7-x", &head), Ok(()));
        assert!(dir.exists());
        let e = verify_cut(&r, &dir, "7-x", &"0".repeat(40)).unwrap_err();
        assert_eq!(e.kind, "mismatch");
        assert!(!dir.exists() && !git(&r, &["rev-parse", "--verify", "--quiet", "refs/heads/7-x"]).ok, "the worktree and its new branch are gone");
        let _ = fs::remove_dir_all(&root);
    }

    // The merge / pull request goes to the branch the ticket was cut from.
    #[test]
    fn the_merge_request_targets_the_base_branch() {
        let f = Flow { base_branch: "develop".into(), ..Default::default() };
        let has = |a: &[&str], k: &str, v: &str| a.windows(2).any(|w| w == [k, v]);
        let (gh, gl) = (gh_pr_args(&f, "t", "b"), glab_mr_args(&f, "https://h/g/p", "t", "b"));
        assert!(has(&gh, "--base", "develop") && has(&gh, "--head", ""), "{gh:?}");
        assert!(has(&gl, "--target-branch", "develop") && has(&gl, "--source-branch", ""), "{gl:?}");
        let old = Flow::default();
        assert!(!gh_pr_args(&old, "t", "b").contains(&"--base") && !glab_mr_args(&old, "https://h/g/p", "t", "b").contains(&"--target-branch"), "a flow from before the branch was asked: the forge's own default, as before");
    }

    // The work log follows Bobix's language (kept per flow from its start); what the team reads stays English: the
    // MR / PR text comes from the session's own `## MR (English)` section, never from the log's other sections.
    #[test]
    fn a_vietnamese_log_reads_back_and_the_mr_stays_english() {
        // English: nothing changes
        assert_eq!(rules("7-x", "/w", "/l.md", "en"), rules("7-x", "/w", "/l.md", ""));
        assert!(!rules("7-x", "/w", "/l.md", "en").contains("Language") && !template(7, "x", "en").contains("MR (English)"));
        assert!(!review_prompt("7-x", "b", "/w", "/l.md", "en").contains("Vietnamese"));
        // Vietnamese: the log in Vietnamese, the markers and what the team reads in English
        let r = rules("7-x", "/w", "/l.md", "vi");
        assert!(r.starts_with(&rules("7-x", "/w", "/l.md", "en")) && r.contains("write the work log in Vietnamese") && r.contains("commit messages in English") && r.contains("## MR (English)"));
        assert!(review_prompt("7-x", "b", "/w", "/l.md", "vi").contains("Write what you found in Vietnamese"));
        let t = template(7, "Sửa tổng giỏ hàng", "vi");
        assert!(t.ends_with("## Result\n\n## MR (English)\n"), "{t}");
        let log = t.replace("Status: planning", "Status: done")
            .replace("## Plan\n", "## Plan\nSửa hàm `cartTotal` để trừ mã giảm giá.\n")
            .replace("## Steps\n", "## Steps\n- Đọc `src/cart.js`\n- Thêm phép trừ mã giảm giá\n")
            .replace("## Assumptions\n", "## Assumptions\n- Mã giảm giá không cộng dồn\n")
            .replace("## Checks\n", "## Checks\n- `npm test`\n```\nok 12 tests passed\n```\n- Review: /code-review, không có lỗi\n")
            .replace("## Result\n", "## Result\nĐã xong: tổng giỏ hàng trừ mã giảm giá.\n")
            .replace("## MR (English)\n", "## MR (English)\nThe cart total now subtracts the discount code.\n\n- Assumes codes do not stack.\n- Checked with `npm test`.\n");
        let p = parse_log(&log);
        assert_eq!((&p["status"], &p["ran"], &p["reviewed"]), (&json!("done"), &json!(1), &json!(true)), "parsing works with Vietnamese content");
        assert_eq!(p["assumptions"], json!(["Mã giảm giá không cộng dồn"]));
        assert!(p["checksText"].as_str().unwrap().contains("```\nok 12 tests passed\n```"), "command output exactly as printed");
        assert_eq!(p["result"], "Đã xong: tổng giỏ hàng trừ mã giảm giá.", "the MR section is not part of the result");
        assert_eq!(p["mr"], "The cart total now subtracts the discount code.\n\n- Assumes codes do not stack.\n- Checked with `npm test`.", "kept as written, blank lines too");
        let mut f = Flow { iid: 7, path: "demo/shop".into(), title: "Cart total".into(), log: log.clone(), lang: "vi".into(), ..Default::default() };
        let b = mr_body(&f);
        assert_eq!(b, "Closes #7\n\nThe cart total now subtracts the discount code.\n\n- Assumes codes do not stack.\n- Checked with `npm test`.\n");
        assert!(mr_ready(&f) && !b.contains("Mã") && !b.contains("Đã"), "nothing of the Vietnamese log goes to the MR");
        // no English section yet: only the close line, and the box says so
        f.log = log.split("## MR (English)").next().unwrap().to_string() + "## MR (English)\n";
        assert_eq!((mr_body(&f), mr_ready(&f)), ("Closes #7\n".to_string(), false));
        // the language was changed after the start: the flow keeps its own
        let old = Flow { lang: String::new(), ..f.clone() };
        assert!(english(&old) && mr_ready(&old), "a flow from before (no language): English, as before");
        // a session that wrote the status in Vietnamese anyway
        assert_eq!(parse_log("Status: xong\n")["status"], "done");
        assert_eq!(parse_log("Status: bị chặn vì thiếu quyền\n")["status"], "blocked");
    }

    #[test]
    fn merge_request_text_and_link() {
        let mut f = Flow { iid: 7, title: "Add subtract".into(), plan: "1. add it".into(), ..Default::default() };
        f.log = template(7, "Add subtract", "en").replace("## Assumptions\n", "## Assumptions\n- ints only\n").replace("## Checks\n", "## Checks\n- `npm test`\n```\nok 2\n```\n");
        let b = mr_body(&f);
        assert!(b.starts_with("Closes #7\n\n## Plan\n\n1. add it\n\n## Assumptions\n\n- ints only\n\n## Checks\n\n- `npm test`\n```\nok 2\n```"), "{b}");
        f.log = template(7, "x", "en");
        assert!(mr_body(&f).contains("## Checks\n\nNone pasted."), "no checks: said so");
        // the code in another repository: the close reference names the issue's repository, the MR goes to the code's
        let x = Flow { path: "group/space-v2".into(), iid: 123, code: "group/space-langchain-agent".into(), log: template(123, "x", "en"), ..Default::default() };
        assert!(mr_body(&x).starts_with("Closes group/space-v2#123\n"), "{}", mr_body(&x));
        assert_eq!((x.code_repo(), Flow { path: "a/b".into(), ..Default::default() }.code_repo()), ("group/space-langchain-agent", "a/b"));
        let mut fl = Flows::default();
        fl.list.insert("k".into(), Flow { key: "k".into(), path: "group/space-v2".into(), code: "group/space-langchain-agent".into(), mr: "https://gitlab.example.test/group/space-langchain-agent/-/merge_requests/4".into(), ..Default::default() });
        assert_eq!(fl.open_mrs()[0].2, "group/space-langchain-agent", "CI follows the code repository");
        assert_eq!(mr_url("Creating merge request for 7-x into main in demo/shop\n\nhttps://gitlab.example.test/demo/shop/-/merge_requests/12\n").as_deref(), Some("https://gitlab.example.test/demo/shop/-/merge_requests/12"));
        assert_eq!(mr_url("error"), None);
        assert!(open_mr("glab", "h", &Flow { branch: "main".into(), ..Default::default() }, "t", "b").is_err(), "never the default branch");
    }

    #[test]
    fn ci_status_of_the_merge_request() {
        assert_eq!(mr_iid("https://h/g/p/-/merge_requests/12"), Some(12));
        assert_eq!(mr_iid("https://h/g/p/-/merge_requests/12/pipelines"), Some(12));
        let ci = |st: Value| ci_of(&json!({ "state": "opened", "head_pipeline": { "status": st, "web_url": "https://h/g/p/-/pipelines/9" } }))["status"].clone();
        assert_eq!([ci(json!("success")), ci(json!("failed")), ci(json!("pending")), ci(json!("canceled"))], [json!("passed"), json!("failed"), json!("running"), json!("other")]);
        assert_eq!(ci_of(&json!({ "state": "opened", "head_pipeline": null }))["status"], "none");
        let mut fl = Flows::default();
        fl.list.insert("g/p#7".into(), Flow { key: "g/p#7".into(), path: "g/p".into(), mr: "https://h/g/p/-/merge_requests/12".into(), ..Default::default() });
        assert_eq!(fl.open_mrs(), [("g/p#7".to_string(), "h".to_string(), "g/p".to_string(), 12, false)]);
        let red = || ci_of(&json!({ "state": "opened", "head_pipeline": { "status": "failed", "web_url": "u" } }));
        assert!(fl.set_ci("g/p#7", red(), 100));
        assert!(!fl.set_ci("g/p#7", red(), 200), "still red: no change");
        assert_eq!((&fl.list[0].ci["since"], &fl.list[0].ci["at"]), (&json!(100), &json!(200)), "red since the first read");
        assert!(fl.set_ci("g/p#7", ci_of(&json!({ "state": "merged", "head_pipeline": { "status": "success", "web_url": "u" } })), 300));
        assert!(fl.open_mrs().is_empty(), "merged: not read again");
    }

    #[test]
    fn github_pull_requests_and_their_checks() {
        assert_eq!(mr_url("Creating pull request for 7-x into main in demo/shop\n\nhttps://github.com/demo/shop/pull/12\n").as_deref(), Some("https://github.com/demo/shop/pull/12"));
        assert_eq!(mr_iid("https://github.com/demo/shop/pull/12"), Some(12));
        let ch = |b: &[&str]| json!(b.iter().map(|x| json!({ "name": "t", "bucket": x, "link": format!("https://github.com/demo/shop/actions/runs/{x}") })).collect::<Vec<_>>());
        assert_eq!(ci_of_checks(&ch(&["pass", "fail", "pending"]), "OPEN"), json!({ "status": "failed", "url": "https://github.com/demo/shop/actions/runs/fail", "state": "opened" }));
        assert_eq!(ci_of_checks(&ch(&["pass", "pending"]), "OPEN")["status"], "running");
        assert_eq!(ci_of_checks(&ch(&["pass", "skipping"]), "MERGED"), json!({ "status": "passed", "url": "https://github.com/demo/shop/actions/runs/pass", "state": "merged" }));
        assert_eq!(ci_of_checks(&json!([]), "CLOSED"), json!({ "status": "none", "url": null, "state": "closed" }));
        let mut fl = Flows::default();
        fl.list.insert("demo/shop#7".into(), Flow { key: "demo/shop#7".into(), path: "demo/shop".into(), forge: "github".into(), mr: "https://github.com/demo/shop/pull/12".into(), ..Default::default() });
        assert_eq!(fl.open_mrs(), [("demo/shop#7".to_string(), "github.com".to_string(), "demo/shop".to_string(), 12, true)]);
        assert!(!Flow::default().github(), "a flow from before GitHub is GitLab's");
    }

    // A Full start opened in Claude desktop: the session whose first prompt names the issue is the flow's, wherever
    // Claude opened it. In a worktree Claude made the flow works there (its folder, its branch, its work log); in the
    // repository's main folder the flow keeps its own worktree. Bobix only ever deletes the folder it made.
    #[test]
    fn a_flow_follows_the_session_claude_opened_elsewhere() {
        let root = std::env::temp_dir().join(format!("bobix-follow-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let s = |p: &Path| p.to_string_lossy().into_owned();
        let repo = root.join("shop");
        let (own, theirs) = (worktree_dir(&s(&repo), 7, "g/shop", ""), repo.join(".claude").join("worktrees").join("brave-otter"));
        // flows from before the log moved out of the worktree: theirs is in the folder where the session works
        for d in [&own, &theirs] { fs::create_dir_all(d.join(LOG_DIR)).unwrap(); fs::write(d.join(LOG_DIR).join(LOG_FILE), template(7, "Add subtract", "en")).unwrap(); }
        let flow = |iid: i64, dir: &Path| Flow { key: key_of("g/shop", iid), path: "g/shop".into(), iid, repo: s(&repo), dir: s(dir), branch: format!("{iid}-add-subtract"), status: "starting".into(), ..Default::default() };
        let mut fl = Flows::default();
        for f in [flow(7, &own), flow(8, &worktree_dir(&s(&repo), 8, "g/shop", ""))] { fl.list.insert(f.key.clone(), f); }
        // the start's wait leads to its flow; a Quick start (it waits in the main folder) has none
        let wait = |iid: i64, dir: &str| json!({ "dir": dir, "repo": s(&repo), "path": "g/shop", "iid": iid });
        assert_eq!(fl.key_of_start(&wait(7, &s(&own))).as_deref(), Some("g/shop#7"));
        assert_eq!(fl.key_of_start(&wait(7, &format!("{}/", s(&own)))).as_deref(), Some("g/shop#7"), "the folder with a trailing separator");
        assert_eq!((fl.key_of_start(&wait(7, &s(&repo))), fl.key_of_start(&wait(9, &s(&own)))), (None, None));
        assert_eq!(fl.waiting().map(|f| f.iid).collect::<Vec<_>>(), [7, 8], "both wait again after a restart");
        // Claude opened it in a worktree of its own: the first prompt took the start, so the flow takes the session
        assert!(fl.take("g/shop#7", "s7", Some(41), 1));
        assert_eq!((fl.list[0].status.as_str(), fl.list[0].session.as_deref(), fl.list[0].pid), ("planning", Some("s7"), Some(41)));
        assert!(!fl.take("g/shop#7", "other", None, 2), "a flow that has its session keeps it");
        assert!(!fl.take("g/shop#8", "s7", None, 2) && fl.list[1].session.is_none(), "a session that has a flow takes no other");
        assert!(!fl.take("g/shop#nope", "x", None, 2));
        assert_eq!(fl.waiting().map(|f| f.iid).collect::<Vec<_>>(), [8]);
        assert_eq!(fl.placed("g/shop#7", "s7", &s(&own), Some("7-add-subtract"), 0, 3), None, "in the flow's own worktree: nothing changes");
        assert_eq!(fl.placed("g/shop#7", "s7", &s(&root.join("elsewhere")), Some("x"), 0, 3), None, "a folder that is not this repository's");
        assert_eq!(fl.placed("g/shop#7", "other", &s(&theirs), Some("claude/brave-otter"), 2, 3), None, "only the flow's own session moves it");
        assert_eq!(fl.list[0].dir, s(&own));
        assert_eq!(fl.placed("g/shop#7", "s7", &s(&theirs), Some("claude/brave-otter"), 2, 3), Some("claude"));
        let f = fl.list[0].clone();
        assert_eq!((f.place.as_str(), f.dir.as_str(), f.own.as_str(), f.branch.as_str(), f.mixed, f.carried), ("claude", s(&theirs).as_str(), s(&own).as_str(), "claude/brave-otter", 0, 2), "2 commits of yours came with Claude's worktree");
        assert_eq!((f.made(), fl.key_of_start(&wait(7, &s(&own))).as_deref()), (s(&own).as_str(), Some("g/shop#7")), "the folder Bobix made is still its own one");
        assert_eq!(fl.placed("g/shop#7", "s7", &s(&repo), None, 5, 4), None, "set once");
        // its plan, its work log and its branch are read where it works now
        assert!(fl.on_event("s7", &s(&theirs), "PermissionRequest", "ExitPlanMode", Some("1. add it"), None, 5) && fl.list[0].status == "approve");
        fs::write(theirs.join(LOG_DIR).join(LOG_FILE), template(7, "Add subtract", "en").replace("Status: planning", "Status: done").replace("## Steps\n", "## Steps\n- Added subtract\n")).unwrap();
        // approved in Claude itself (Bobix restarted while the plan waited, or you answered there): the plan tool went through
        assert!(fl.on_event("s7", &s(&theirs), "PostToolUse", "ExitPlanMode", None, None, 6));
        assert_eq!((fl.list[0].status.as_str(), fl.detail("g/shop#7").unwrap()["parsed"]["steps"].clone()), ("done", json!(["Added subtract"])), "approved there, and its log says done");
        assert!(fl.branch_at(&format!("{}/", s(&theirs)), "7-fix-subtract") && fl.list[0].branch == "7-fix-subtract", "the session made another branch there");
        assert!(!fl.branch_at(&s(&theirs), "7-fix-subtract") && !fl.branch_at(&s(&own), "main") && !fl.branch_at(&s(&repo), "main"));
        // Claude opened the other one in the repository's main folder: the flow takes the session and keeps its worktree
        assert!(fl.take("g/shop#8", "s8", None, 7));
        assert_eq!(fl.placed("g/shop#8", "s8", &s(&repo), Some("main"), 3, 8), Some("main"));
        let g = fl.list[1].clone();
        assert_eq!((g.place.as_str(), g.dir.as_str(), g.own.as_str(), g.branch.as_str(), g.mixed), ("main", s(&worktree_dir(&s(&repo), 8, "g/shop", "")).as_str(), "", "8-add-subtract", 3), "3 files of yours were not committed there");
        assert_eq!(g.made(), g.dir);
        assert!(!fl.branch_at(&s(&repo), "develop") && fl.list[1].branch == "8-add-subtract", "the main folder's branch is never the ticket's");
        // kept across restarts, and a flow from before these fields reads back as one in its own worktree
        let file = root.join("tickets.json");
        assert_eq!(fl.list[1].carried, 0);
        let mut saved = Flows::load(file.clone());
        saved.list = fl.list.clone();
        saved.save();
        assert_eq!(Flows::load(file).list, fl.list);
        let old: Flow = serde_json::from_str(r#"{"key":"g/shop#3","dir":"/src/shop/.claude/worktrees/bobix-3","repo":"/src/shop","status":"starting"}"#).unwrap();
        assert_eq!((old.place.as_str(), old.made(), place_of(&old, "/src/shop")), ("", "/src/shop/.claude/worktrees/bobix-3", Some("main")), "an old start Claude opens in the main folder");
        let _ = fs::remove_dir_all(&root);
    }

    // The work log is one file per ticket in Bobix's folder, never inside a repository or a worktree: another ticket of
    // the same repository, and the same issue number in another repository, each get a file of their own, also when the
    // names differ only in characters a file name cannot hold; Bobix's folder may hold spaces ("Application Support").
    #[test]
    fn each_ticket_has_its_own_log_file_in_bobixs_folder() {
        let data = Path::new("/Users/demo/Library/Application Support/Bobix");
        let names: Vec<PathBuf> = ["acme/web#42", "acme/web#43", "acme/api#42", "acme_web/x#42", "acme/web_x#42", "acme/web.x#42", "acme/web#42>acme/api"].iter().map(|k| log_path(data, k)).collect();
        for (i, a) in names.iter().enumerate() { for b in &names[i + 1..] { assert_ne!(a, b); } }
        assert!(names.iter().all(|n| n.parent() == Some(&logs_dir(data)) && n.extension().is_some_and(|e| e == "md")));
        assert!(names[0].to_string_lossy().contains("Application Support/Bobix/worklogs/acme_web_42-"), "{:?}", names[0]);
        assert_eq!(log_path(data, "acme/web#42"), log_path(data, "acme/web#42"), "the same ticket, the same file, after a restart");
        assert!(log_path(data, &format!("{}/p#1", "g/".repeat(300))).file_name().unwrap().len() < 130, "a long group path still fits a file name");
    }

    // Start makes the log from the template in Bobix's folder, with a folder name that has spaces; a flow reads it back
    // on every event; deleted by hand it is said (not an error) and the text last read stays; made again it reads again.
    #[test]
    fn a_new_flow_reads_the_log_in_bobixs_folder_and_says_when_it_is_gone() {
        let root = std::env::temp_dir().join(format!("bobix-newlog-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let data = root.join("Application Support").join("Bobix");
        let wt = root.join("repo").join(".bobix").join("worktrees").join("web-42");
        fs::create_dir_all(&wt).unwrap();
        let key = "acme/web#42";
        let file = log_path(&data, key);
        let tpl = template(42, "Add subtract", "en");
        seed_log(&file, &tpl, false, &wt).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), tpl, "seeded from the template");
        assert_eq!(fs::read_dir(logs_dir(&data)).unwrap().count(), 1, "exactly one file");
        assert!(!wt.join(LOG_DIR).exists() && !root.join("repo").join(LOG_DIR).exists(), "no .bobix-log folder in the worktree or the repository");
        let d = wt.to_string_lossy().to_string();
        let mut fl = Flows::default();
        fl.list.insert(key.into(), Flow { key: key.into(), path: "acme/web".into(), iid: 42, dir: d.clone(), log_file: file.to_string_lossy().into(), status: "starting".into(), ..Default::default() });
        assert!(fl.on_event("s1", &d, "SessionStart", "", None, None, 1));
        assert!(!fl.list[0].log_missing && fl.list[0].log == tpl);
        // the session writes the file in the folder with spaces; the flow follows it
        fs::write(&file, tpl.replace("Status: planning", "Status: done").replace("## Assumptions\n", "## Assumptions\n- Integers only\n")).unwrap();
        assert!(fl.on_event("s1", &d, "Stop", "", None, None, 2));
        assert_eq!((fl.list[0].status.as_str(), assumptions(&fl.list[0].log)), ("done", 1));
        // an old log in the worktree is not this flow's: the file in Bobix's folder wins
        fs::create_dir_all(wt.join(LOG_DIR)).unwrap();
        fs::write(wt.join(LOG_DIR).join(LOG_FILE), "Status: blocked\n").unwrap();
        fl.on_event("s1", &d, "Stop", "", None, None, 3);
        assert_eq!(fl.list[0].status, "done");
        // deleted by hand: said, nothing crashes, the last text read is kept
        let before = fl.list[0].log.clone();
        fs::remove_file(&file).unwrap();
        fl.on_event("s1", &d, "PostToolUse", "Edit", None, None, 4);
        assert!(fl.list[0].log_missing && fl.list[0].log == before && fl.list[0].status == "done", "the flow keeps its last read text");
        assert!(!fl.refresh(key, 5) && fl.list[0].log_missing, "nothing new to tell");
        fs::write(&file, &before).unwrap();
        assert!(fl.refresh(key, 5) && !fl.list[0].log_missing, "the file is back: the pages are told");
        fs::remove_file(&file).unwrap();
        assert!(fl.refresh(key, 5) && fl.list[0].log_missing, "gone again: the pages are told");
        let v = fl.view(None);
        assert_eq!(v["list"][0]["logMissing"], json!(true), "the pages are told");
        assert!(v["list"][0].get("log").is_none());
        fs::write(&file, &before).unwrap();
        fl.on_event("s1", &d, "PostToolUse", "Edit", None, None, 6);
        assert!(!fl.list[0].log_missing, "made again: read again");
        // cleaning up (a copy for flows from before) never overwrites the live file with the text last read
        fs::write(&file, "newer text").unwrap();
        keep_log(&data, &fl.list[0]);
        assert_eq!(fs::read_to_string(&file).unwrap(), "newer text");
        // the worktree and then the whole repository folder go: the log stays
        fs::remove_dir_all(&wt).unwrap();
        fs::remove_dir_all(root.join("repo")).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "newer text");
        // started again for the same ticket: with its worktree kept, its log is kept; with none (a branch left after Clean
        // up counts for nothing), the log begins again and the earlier one is moved aside, not overwritten
        seed_log(&file, &tpl, true, &wt).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "newer text");
        let done = tpl.replace("Status: planning", "Status: done");
        fs::write(&file, &done).unwrap();
        seed_log(&file, &tpl, false, &wt).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), tpl, "a new flow starts from the template, not from the old done log");
        let aside: Vec<_> = fs::read_dir(logs_dir(&data)).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p != &file).collect();
        assert_eq!(aside.len(), 1, "the earlier log is moved aside");
        assert!(fs::read_to_string(&aside[0]).unwrap() == done && aside[0].to_string_lossy().contains("acme_web_42-"), "{aside:?}");
        let _ = fs::remove_dir_all(&root);
    }

    // A flow started before the log moved (no `log_file`) keeps being read from its folder, which is never moved or
    // deleted; its kept copy goes to Bobix's folder on clean up as before; a worktree kept from such a flow gives the
    // log of a new start its text, the old file left where it is.
    #[test]
    fn a_flow_from_before_is_still_read_from_its_worktree() {
        let root = std::env::temp_dir().join(format!("bobix-oldlog-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (data, wt) = (root.join("data"), root.join("repo").join(".bobix").join("worktrees").join("web-7"));
        fs::create_dir_all(wt.join(LOG_DIR)).unwrap();
        let old = wt.join(LOG_DIR).join(LOG_FILE);
        fs::write(&old, template(7, "x", "en").replace("Status: planning", "Status: running")).unwrap();
        let old_flow: Flow = serde_json::from_str(&format!(r#"{{"key":"g/web#7","dir":{},"status":"planning","plan":"p","session":"s1"}}"#, serde_json::to_string(&wt.to_string_lossy()).unwrap())).unwrap();
        assert!(old_flow.log_file.is_empty() && !old_flow.log_missing, "an old tickets.json reads back as an old flow");
        assert_eq!(old_flow.log_at(), old.to_string_lossy());
        assert_eq!(old_flow.log_dir(), None, "a terminal start adds no folder for it");
        let mut fl = Flows::default();
        fl.list.insert("g/web#7".into(), old_flow);
        let d = wt.to_string_lossy().to_string();
        assert!(fl.on_event("s1", &d, "PostToolUse", "Edit", None, None, 1));
        assert_eq!((fl.list[0].status.as_str(), fl.list[0].log_missing), ("running", false));
        keep_log(&data, &fl.list[0]);
        assert!(fs::read_to_string(log_path(&data, "g/web#7")).unwrap().contains("Status: running"), "the copy is kept as before");
        assert!(old.is_file(), "the old log is not moved or deleted");
        // a new start on the worktree kept from it: the new file begins with the old text; the old file stays
        let file = log_path(&data, "g/web#7");
        let _ = fs::remove_file(&file);
        seed_log(&file, &template(7, "x", "en"), true, &wt).unwrap();
        assert!(fs::read_to_string(&file).unwrap().contains("Status: running") && old.is_file());
        // its session came up in a worktree Claude made (`dir` moves, the one Bobix made is `own`): the log is still read
        let theirs = root.join("claude-wt");
        fs::create_dir_all(&theirs).unwrap();
        fl.list[0].own = d.clone();
        fl.list[0].dir = theirs.to_string_lossy().into();
        fl.on_event("s1", &theirs.to_string_lossy(), "Stop", "", None, None, 3);
        assert!(!fl.list[0].log_missing && fl.list[0].log.contains("Status: running"), "read from the worktree Bobix made");
        fs::create_dir_all(theirs.join(LOG_DIR)).unwrap();
        fs::write(theirs.join(LOG_DIR).join(LOG_FILE), "Status: done\n").unwrap();
        fl.on_event("s1", &theirs.to_string_lossy(), "Stop", "", None, None, 4);
        assert_eq!(fl.list[0].status, "done", "and where the session works, once it writes one there");
        fl.list[0].dir = d.clone();
        fl.list[0].own = String::new();
        // its worktree gone: Work says the log is missing
        fs::remove_dir_all(&wt).unwrap();
        fl.on_event("s1", &d, "Stop", "", None, None, 2);
        assert!(fl.list[0].log_missing && fl.list[0].log.contains("Status: done"));
        let _ = fs::remove_dir_all(&root);
    }

    // A Windows path (drive letter, backslashes, a space) goes into the prompt text, the terminal command and the
    // folder Claude is given exactly as it is, and a file at a path of this system's own kind reads back.
    #[test]
    fn windows_paths_reach_the_prompt_and_are_cut_at_either_separator() {
        let log = r"C:\Users\An Nguyen\AppData\Roaming\Bobix\worklogs\acme_web_42-1a2b3c4d.md";
        let r = rules("42-add", r"C:\src\web\.bobix\worktrees\web-42", log, "en");
        assert!(r.contains(&format!("`{log}` (the full path")), "{r}");
        assert!(review_prompt("42-add", "origin/HEAD", r"C:\src\web", log, "en").contains(&format!("`{log}`")));
        let f = Flow { log_file: log.into(), dir: r"C:\src\web".into(), ..Default::default() };
        assert_eq!(f.log_dir().as_deref(), Some(r"C:\Users\An Nguyen\AppData\Roaming\Bobix\worklogs"));
        assert_eq!(f.log_at(), log);
        // quoted for a POSIX shell, like the rest of the command
        assert!(cli_command(&f.dir, "sonnet", "go", f.log_dir().as_deref()).contains(r"--add-dir='C:\Users\An Nguyen\AppData\Roaming\Bobix\worklogs'"));
        assert_eq!((dir_of("/a/b/c.md").as_deref(), dir_of("c.md"), dir_of("/c.md")), (Some("/a/b"), None, None));
        // read back at a path of this system's kind (a drive letter and backslashes on Windows)
        let root = std::env::temp_dir().join(format!("bobix-winlog-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let file = log_path(&root.join("An Nguyen"), "acme/web#42");
        seed_log(&file, "Status: done\n", false, &root).unwrap();
        let f = Flow { log_file: file.to_string_lossy().into(), ..Default::default() };
        assert_eq!(read_log_of(&f).as_deref(), Some("Status: done\n"));
        let _ = fs::remove_dir_all(&root);
    }

    // Folders are compared as folders: git prints "C:/x/shop", Claude sends "C:\x\shop", a worktree path mixes both.
    #[test]
    fn where_a_session_came_up_is_read_from_folders_not_text() {
        let f = Flow { repo: "C:/Code/shop".into(), dir: "C:/Code/shop\\.bobix\\worktrees\\shop-7".into(), ..Default::default() };
        assert_eq!(place_of(&f, "C:\\Code\\shop\\.bobix\\worktrees\\shop-7\\"), Some(""));
        assert_eq!(place_of(&f, "C:\\Code\\shop"), Some("main"));
        assert_eq!(place_of(&f, "C:\\Code\\shop\\.claude\\worktrees\\brave-otter"), Some("claude"));
        assert_eq!(place_of(&f, "C:/Code/shop/.claude/worktrees/brave-otter/"), Some("claude"));
        assert_eq!(place_of(&f, "C:\\Code\\shop-old"), None, "a folder whose name only begins the same");
        assert_eq!(place_of(&f, "C:\\Code\\shop\\.claude\\worktrees"), None, "not a worktree, the folder that holds them");
        assert_eq!(place_of(&f, "C:\\Code\\shop\\src"), None);
        // case: one folder on Windows and macOS, two on Linux
        let folded = cfg!(any(windows, target_os = "macos"));
        assert_eq!(place_of(&f, "c:\\code\\SHOP"), folded.then_some("main"));
        assert_eq!(place_of(&f, "c:/code/shop/.Claude/Worktrees/brave-otter"), folded.then_some("claude"));
        let l = Flow { repo: "/home/dev/shop".into(), dir: "/home/dev/shop/.bobix/worktrees/shop-7".into(), ..Default::default() };
        assert_eq!((place_of(&l, "/home/dev/shop/"), place_of(&l, "/home/dev/shop/.claude/worktrees/x"), place_of(&l, "/home/dev/shop/.bobix/worktrees/shop-7")), (Some("main"), Some("claude"), Some("")));
        assert_eq!(place_of(&Flow { dir: "/w".into(), ..Default::default() }, "/src"), None, "a flow that knows no repository");
    }

    #[test]
    fn worktree_made_and_removed_with_git() {
        let root = std::env::temp_dir().join(format!("bobix-wt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (up, repo) = (root.join("up"), root.join("repo"));
        fs::create_dir_all(&up).unwrap();
        let u = up.to_string_lossy().to_string();
        for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=demo@example.test", "-c", "user.name=demo", "commit", "-q", "--allow-empty", "-m", "init"]] { assert!(git(&u, a).ok); }
        assert!(procs::exec("git", &["clone", "-q", &u, &repo.to_string_lossy()], 30, &[]).ok);
        let r = repo.to_string_lossy().to_string();
        let dir = worktree_dir(&r, 7, "g/r", "");
        assert_eq!(cut_from(&r, &dir, "7-add-subtract", "main").unwrap().name, "origin/main");
        assert_eq!(fs::read_to_string(lf(&dir)).unwrap(), "log");
        assert!(git(&dir.to_string_lossy(), &["status", "--porcelain"]).out.trim().is_empty() && !dir.join(LOG_DIR).exists(), "no log folder in the worktree: git has nothing to ignore");
        assert!(dir.starts_with(repo.join(".bobix").join("worktrees")) && git(&r, &["status", "--porcelain"]).out.trim().is_empty(), "the worktrees' folder ignores itself: the main folder's status stays clean");
        assert!(cut_from(&r, &dir, "8-other", "main").is_err(), "already there, for another branch");
        assert_eq!(at_risk(&dir.to_string_lossy(), "7-add-subtract"), Some((0, 0)));
        fs::write(dir.join("new.txt"), "x").unwrap();
        assert_eq!(at_risk(&dir.to_string_lossy(), "7-add-subtract"), Some((1, 0)), "what would be lost is counted first");
        // the main folder's own uncommitted files: Claude's worktrees there are not yours to count
        fs::create_dir_all(repo.join(".claude").join("worktrees").join("brave-otter")).unwrap();
        fs::write(repo.join(".claude").join("worktrees").join("brave-otter").join("x.txt"), "x").unwrap();
        assert_eq!(uncommitted(&r), 0);
        fs::write(repo.join("mine.txt"), "x").unwrap();
        assert_eq!(uncommitted(&r), 1);
        fs::remove_file(repo.join("mine.txt")).unwrap();
        fs::remove_dir_all(repo.join(".claude")).unwrap();
        assert!(remove_worktree(&r, &dir.to_string_lossy(), false).is_err(), "work not committed: Clean up never forces");
        remove_worktree(&r, &dir.to_string_lossy(), true).unwrap();
        assert!(!dir.exists() && at_risk(&dir.to_string_lossy(), "7-add-subtract").is_none(), "Remove and delete the folder, after you saw it");
        cut_from(&r, &dir, "7-add-subtract", "main").unwrap();
        assert!(git(&dir.to_string_lossy(), &["-c", "user.email=demo@example.test", "-c", "user.name=demo", "commit", "-q", "--allow-empty", "-m", "work"]).ok);
        remove_worktree(&r, &dir.to_string_lossy(), false).unwrap();
        assert!(!dir.exists());
        assert!(git(&r, &["rev-parse", "--verify", "refs/heads/7-add-subtract"]).ok, "the branch stays");
        assert_eq!(unpushed(&r, "7-add-subtract"), 1, "no remote has it");
        // the worktree deleted by hand (not with git): its branch and commit are still in the repository, so what is
        // not pushed is still said and Open MR still pushes it; a new Start makes the worktree again on that branch
        let base = cut_from(&r, &dir, "7-add-subtract", "main").unwrap().commit;
        fs::remove_dir_all(&dir).unwrap();
        let f = Flow { dir: dir.to_string_lossy().into(), repo: r.clone(), branch: "7-add-subtract".into(), base, ..Default::default() };
        assert_eq!((f.git_dir(), unpushed(f.git_dir(), &f.branch)), (r.as_str(), 1));
        assert!(open_mr("no-such-glab", "h", &f, "t", "b").unwrap_err().starts_with("glab mr create"), "pushed from the repository; only glab is missing here");
        assert!(git(&u, &["rev-parse", "--verify", "refs/heads/7-add-subtract"]).ok, "the ticket's branch reached the remote");
        assert_eq!(remove_worktree(&r, &dir.to_string_lossy(), false), Ok(()), "Clean up of a folder that is gone only forgets it");
        fs::create_dir_all(&dir).unwrap(); // git still lists the old one until pruned
        fs::remove_dir_all(&dir).unwrap();
        cut_from(&r, &dir, "7-add-subtract", "main").unwrap();
        assert_eq!(git(&dir.to_string_lossy(), &["rev-list", "--count", "HEAD", "--not", "origin/main"]).out.trim(), "1", "the same branch, its commit kept");
        // Claude cuts its own worktree from the main folder's HEAD: a commit of yours there that no remote has rides
        // along; it is counted (what Bobix shows at placement and in Open MR)
        assert!(git(&r, &["-c", "user.email=demo@example.test", "-c", "user.name=demo", "commit", "-q", "--allow-empty", "-m", "mine, not pushed"]).ok);
        let cut = repo.join(".claude").join("worktrees").join("brave-otter");
        assert!(git(&r, &["worktree", "add", "-q", "-b", "claude/brave-otter", &cut.to_string_lossy()]).ok, "from the main folder's HEAD, as Claude does");
        assert_eq!(unpushed(&cut.to_string_lossy(), "HEAD"), 1);
        assert_eq!(git(&r, &["worktree", "remove", "--force", &cut.to_string_lossy()]).ok, true);
        assert!(git(&r, &["reset", "-q", "--hard", "HEAD~1"]).ok);
        // a branch with nothing committed on it is never pushed as an empty merge request
        let empty = worktree_dir(&r, 8, "g/r", "");
        let b8 = cut_from(&r, &empty, "8-nothing-yet", "main").unwrap().commit;
        let f8 = Flow { dir: empty.to_string_lossy().into(), repo: r.clone(), branch: "8-nothing-yet".into(), base: b8, ..Default::default() };
        assert_eq!(open_mr("no-such-glab", "h", &f8, "t", "b"), Err("nocommits".into()));
        assert!(!git(&u, &["rev-parse", "--verify", "--quiet", "refs/heads/8-nothing-yet"]).ok);
        // a worktree Bobix made before, where Claude keeps its own (Claude desktop's link cuts it down to the
        // repository), still there on the ticket's branch: moved with git to the new place, its work with it
        let old = repo.join(".claude").join("worktrees").join("bobix-9");
        assert!(git(&r, &["worktree", "add", "-q", "-b", "9-old-place", &old.to_string_lossy()]).ok);
        fs::write(old.join("half-done.txt"), "not committed").unwrap();
        let to = worktree_dir(&r, 9, "g/r", "");
        assert_eq!(move_kept(&r, 9, "g/r", "", "9-another-title", &to), Ok(false), "on another branch: not this ticket's");
        assert_eq!(move_kept(&r, 10, "g/r", "", "10-x", &worktree_dir(&r, 10, "g/r", "")), Ok(false));
        assert_eq!(move_kept(&r, 9, "g/r", "", "9-old-place", &to), Ok(true));
        assert!(!old.exists() && to.join("half-done.txt").is_file() && !to.to_string_lossy().contains(".claude"));
        assert_eq!(cut_from(&r, &to, "9-old-place", "main").map(|_| ()), Ok(()), "a new Start uses it there");
        assert!(git(&r, &["status", "--porcelain"]).out.trim().is_empty());
        // a worktree deleted by hand holds only its own branch: git forgets that one, not another missing one
        let (a, b) = (root.join("away-a"), root.join("away-b"));
        assert!(git(&r, &["worktree", "add", "-q", "-b", "11-a", &a.to_string_lossy()]).ok && git(&r, &["worktree", "add", "-q", "-b", "12-b", &b.to_string_lossy()]).ok);
        fs::remove_dir_all(&a).unwrap();
        fs::remove_dir_all(&b).unwrap();
        forget_missing(&r, "11-a");
        let listed = git(&r, &["worktree", "list", "--porcelain"]).out;
        assert!(!listed.contains("away-a") && listed.contains("away-b"), "{listed}");
        let _ = fs::remove_dir_all(&root);
    }
}
