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
// The work log, inside the worktree, in a folder that ignores itself: git never commits or pushes it.
pub const LOG_DIR: &str = ".bobix-log";
pub const LOG_FILE: &str = "work-log.md";
const LOG_MAX: usize = 64 * 1024;

// One ticket in the flow. status: starting (worktree made, no session yet) | planning | approve (its plan waits for
// you) | running | done | blocked. cleaned: its worktree was removed (the log stays, copied into Bobix's folder).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Flow {
    pub key: String, pub path: String, pub iid: i64, pub title: String, pub url: String,
    pub repo: String, pub dir: String, pub branch: String, pub base: String,
    pub status: String, pub session: Option<String>, pub plan: String, pub log: String,
    pub seen: usize, // assumptions you have seen; more in the log put a mark on the pet
    pub summary: String, pub models: Value, pub at: u64, pub cleaned: bool,
    pub mr: String, // the merge request (GitLab) / pull request (GitHub) Bobix opened for it (its link), on your click
    pub forge: String, // its issue's tracker: gitlab (also when empty, from before GitHub) | github
    pub ci: Value,  // its head pipeline, as last read: { status: passed | running | failed | other | none, url, state, since, at }
}

pub fn key_of(path: &str, iid: i64) -> String { format!("{path}#{iid}") }

impl Flow {
    // The log read again, and the status it sets (done / blocked; running once its plan was approved).
    fn reread(&mut self) {
        if let Some(l) = read_log(&self.dir) { if l != self.log { self.log = l; } }
        let st = parse_log(&self.log)["status"].as_str().unwrap_or("").to_string();
        if matches!(st.as_str(), "done" | "blocked") && self.status != "approve" { self.status = st; }
        else if st == "running" && self.status == "planning" && !self.plan.is_empty() { self.status = "running".into(); }
    }
}
// The merge request's text, from the work log: the issue it closes, the plan, the assumptions, the checks. Shown to you
// (and editable) before anything is sent: the work log itself stays on this machine.
pub fn mr_title(f: &Flow) -> String { head16(&f.title, 200).to_string() }
pub fn mr_body(f: &Flow) -> String {
    let p = parse_log(&f.log);
    let plan = p["plan"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| f.plan.clone());
    let list = |k: &str| p[k].as_array().into_iter().flatten().filter_map(|x| x.as_str()).map(|x| format!("- {x}")).collect::<Vec<_>>().join("\n");
    let checks = p["checksText"].as_str().unwrap_or("").to_string();
    let mut b = format!("Closes #{}\n", f.iid);
    for (h, t) in [("Plan", plan), ("Assumptions", list("assumptions")), ("Checks", if checks.is_empty() { "None pasted.".into() } else { checks })] {
        if !t.trim().is_empty() { b += &format!("\n## {h}\n\n{}\n", t.trim()); }
    }
    head16(&b, 20_000).to_string()
}
impl Flow { pub fn github(&self) -> bool { self.forge == "github" } }
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

// Commits on the ticket's branch that no remote has.
pub fn unpushed(dir: &str, branch: &str) -> u64 {
    let r = git(dir, &["rev-list", "--count", branch, "--not", "--remotes"]);
    if r.ok { r.out.trim().parse().unwrap_or(0) } else { 0 }
}
// Push the ticket's branch (never the default one) and open its merge request with your own glab, or its pull request
// with your own gh (`bin`, for the flow's forge). → the link.
pub fn open_mr(bin: &str, host: &str, f: &Flow, title: &str, body: &str) -> Result<String, String> {
    if f.branch.is_empty() || ["main", "master", "develop"].contains(&f.branch.as_str()) { return Err("not a ticket branch".into()); }
    let r = git(&f.dir, &["push", "-u", "origin", &f.branch]);
    if !r.ok { return Err(format!("git push: {}", one_line(&r))); }
    if f.github() {
        let env: Vec<(&str, &str)> = GH_ENV.iter().copied().chain([("GH_HOST", host)]).collect();
        let r = procs::exec(bin, &["pr", "create", "--repo", &f.path, "--head", &f.branch, "--title", title, "--body", body], 60, &env);
        return mr_url(&format!("{}\n{}", r.out, r.err)).ok_or_else(|| format!("gh pr create: {}", one_line(&r)));
    }
    let glab = bin;
    let repo = format!("https://{host}/{}", f.path);
    let r = procs::exec(glab, &["mr", "create", "--repo", &repo, "--source-branch", &f.branch, "--title", title, "--description", body, "--yes"], 60, &[("GITLAB_HOST", host), ("NO_PROMPT", "1"), ("NO_COLOR", "1")]);
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
// The branch names the issue the way Bobix already reads branches ("<iid>-…": model::issue_of_branch).
pub fn branch_for(iid: i64, title: &str) -> String { let s = slug(title); if s.is_empty() { format!("{iid}-ticket") } else { format!("{iid}-{s}") } }
// Where Claude itself keeps worktrees (`claude -w`, Claude desktop): <repo>/.claude/worktrees/<name>.
pub fn worktree_dir(repo: &str, iid: i64) -> PathBuf { Path::new(repo).join(".claude/worktrees").join(format!("bobix-{iid}")) }

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

// The work log's template: Bobix writes it into the worktree, the session fills it in.
pub fn template(iid: i64, title: &str) -> String {
    format!("# Work log: #{iid} {title}\n\nStatus: planning\n\n## Plan\n\n## Steps\n\n## Assumptions\n\n## Checks\n\n## Result\n")
}

// How the session works, appended to the issue in the Start prompt (Claude reads English here whatever the panel's language).
pub fn rules(branch: &str) -> String {
    format!("How to work on this ticket (Bobix ticket flow):\n\
1. Call the EnterPlanMode tool now, before changing anything. Read what you need, then write the plan. It must end with a section \"Assumptions\": what you assume because nobody can answer now. Then call ExitPlanMode. Approving that plan is the only approval.\n\
2. After approval, work to the end in this folder (a git worktree on branch `{branch}`). Do not stop to ask, except for these: pushing to the main branch, deleting data, calling hosts this project does not already use, and touching secrets. Commit on `{branch}`; do not push to the main branch.\n\
3. Keep the work log in `{LOG_DIR}/{LOG_FILE}` (git ignores it; it is for me only, never commit or share it). Fill in its template: `Status:` (planning, running, done or blocked), `## Plan` (the approved plan in short), `## Steps` (one line per step as you go), `## Assumptions` (one `- ` line for each assumption you make along the way, also after approval), `## Checks` (steps 4 and 5), `## Result` (what you did, or what blocks you). Update it as you work.\n\
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
pub fn cli_command(dir: &str, model: &str, prompt: &str) -> String { cli_in_mode(dir, model, "plan", prompt) }
pub fn cli_in_mode(dir: &str, model: &str, mode: &str, prompt: &str) -> String {
    let q = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    let p = if prompt.is_empty() { String::new() } else { format!(" {}", q(prompt)) };
    format!("cd {} && claude --model {} --permission-mode {mode}{p}", q(dir), q(model))
}

// What the log says, section by section. Assumptions are the `- ` / `* ` lines under "## Assumptions".
pub fn parse_log(text: &str) -> Value {
    let (mut status, mut sec) = (String::new(), String::new());
    let (mut plan, mut steps, mut assume, mut result, mut checks, mut raw) = (vec![], vec![], vec![], vec![], vec![], vec![]);
    let mut fence = false;
    for l in text.lines() {
        let t = l.trim();
        if sec.starts_with("check") && !t.starts_with("## ") { raw.push(l); }
        // command output pasted under Checks, in code blocks: not items
        if sec.starts_with("check") && t.starts_with("```") { fence = !fence; continue; }
        if fence { continue; }
        if let Some(h) = t.strip_prefix("## ") { sec = h.trim().to_lowercase(); continue; }
        if t.starts_with('#') { continue; }
        if sec.is_empty() { if let Some(s) = t.strip_prefix("Status:") { status = s.trim().to_lowercase(); } continue; }
        if t.is_empty() { continue; }
        let item = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).unwrap_or(t);
        match sec.as_str() {
            s if s.starts_with("plan") => plan.push(t),
            s if s.starts_with("step") => steps.push(item),
            s if s.starts_with("assum") => if t != item { assume.push(item) },
            s if s.starts_with("check") => if t != item { checks.push(item) },
            s if s.starts_with("result") || s.starts_with("block") => result.push(t),
            _ => {}
        }
    }
    let status = ["planning", "running", "done", "blocked"].into_iter().find(|s| status.starts_with(s)).unwrap_or("");
    // A command run (not "None: …", not the review), and whether a review is noted.
    let low = |c: &&str| c.to_lowercase();
    let ran = checks.iter().filter(|c| { let c = low(c); !c.starts_with("none") && !c.starts_with("review") }).count();
    let reviewed = checks.iter().any(|c| low(&c).starts_with("review"));
    json!({ "status": status, "plan": plan.join("\n"), "steps": steps, "assumptions": assume, "result": result.join("\n"),
        "checks": checks, "ran": ran, "reviewed": reviewed, "checksText": raw.join("\n").trim().to_string() })
}

// The prompt of a second session that reviews a done ticket in its worktree, against its plan and assumptions; it
// changes no code and notes what it found under ## Checks.
pub fn review_prompt(branch: &str, base: &str) -> String {
    format!("Review the change on branch `{branch}` in this folder (`git diff {base}...HEAD`) against the approved plan and the Assumptions in `{LOG_DIR}/{LOG_FILE}`. \
Run `/code-review`, or read the diff yourself. Do not change any code or commit. Look for what breaks the plan, a wrong assumption, a missing check, or a bug. \
When you are done, add one line under `## Checks` in that work log: `- Review (second session): ` and what you found, or `no issues`.")
}
pub fn assumptions(log: &str) -> usize { parse_log(log)["assumptions"].as_array().map_or(0, |a| a.len()) }

fn git(dir: &str, args: &[&str]) -> procs::Ran {
    let mut a = vec!["-C", dir];
    a.extend_from_slice(args);
    procs::exec("git", &a, 60, &[("LC_ALL", "C"), ("GIT_TERMINAL_PROMPT", "0")])
}
fn one_line(r: &procs::Ran) -> String { head16(&format!("{} {}", r.err, r.out).split_whitespace().collect::<Vec<_>>().join(" "), 300).to_string() }

// A new worktree for the ticket, on a new branch from the remote's default branch (else from HEAD), with the work log's
// template in its self-ignoring folder. A branch of that name already there is checked out instead. → the base used.
pub fn create_worktree(repo: &str, dir: &Path, branch: &str, log: &str) -> Result<String, String> {
    if dir.exists() { return Err(format!("{} is already there", dir.display())); }
    let d = dir.to_string_lossy();
    let base = ["origin/HEAD", "HEAD"].into_iter().find(|b| git(repo, &["rev-parse", "--verify", "--quiet", b]).ok).ok_or("no commit to start from")?;
    let has = git(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).ok;
    let r = if has { git(repo, &["worktree", "add", &d, branch]) } else { git(repo, &["worktree", "add", "-b", branch, &d, base]) };
    if !r.ok { return Err(one_line(&r)); }
    let l = dir.join(LOG_DIR);
    fs::create_dir_all(&l).and_then(|_| fs::write(l.join(".gitignore"), "*\n")).and_then(|_| fs::write(l.join(LOG_FILE), log)).map_err(|e| e.to_string())?;
    Ok(base.into())
}
// `git worktree remove` without --force: git refuses when there is work not committed, and says so.
pub fn remove_worktree(repo: &str, dir: &str) -> Result<(), String> {
    if !Path::new(dir).exists() { let _ = git(repo, &["worktree", "prune"]); return Ok(()); }
    let r = git(repo, &["worktree", "remove", dir]);
    if r.ok { Ok(()) } else { Err(one_line(&r)) }
}
pub fn read_log(dir: &str) -> Option<String> {
    let t = fs::read_to_string(Path::new(dir).join(LOG_DIR).join(LOG_FILE)).ok()?;
    Some(head16(&t, LOG_MAX).to_string())
}
// A copy of every log in Bobix's own folder, so it outlives the worktree.
pub fn logs_dir(data: &Path) -> PathBuf { data.join("worklogs") }
fn log_name(key: &str) -> String { key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect::<String>() + ".md" }
pub fn keep_log(data: &Path, f: &Flow) {
    if f.log.is_empty() { return; }
    let d = logs_dir(data);
    if fs::create_dir_all(&d).is_ok() { let _ = fs::write(d.join(log_name(&f.key)), &f.log); }
}

// ---------- Haiku, only when you click: through your own `claude -p`, no tools, no hooks (no pet for it) ----------
fn haiku(claude: &str, prompt: &str, schema: Option<&str>) -> Result<Value, String> {
    let mut a = vec!["-p", "--model", "haiku", "--tools", "", "--no-session-persistence", "--strict-mcp-config", "--settings", r#"{"disableAllHooks":true}"#, "--output-format", "json"];
    if let Some(s) = schema { a.extend(["--json-schema", s]); }
    a.push(prompt);
    let r = procs::exec(claude, &a, 120, &[]);
    if r.missing { return Err("claude is not installed".into()); }
    let v: Value = serde_json::from_str(r.out.trim()).map_err(|_| one_line(&r))?;
    if v["is_error"] == true { return Err(head16(v["result"].as_str().unwrap_or("claude -p failed"), 300).into()); }
    Ok(v)
}
const SCHEMA: &str = r#"{"type":"object","properties":{"size":{"enum":["small","large"]},"why":{"type":"string"},"steps":{"type":"array","items":{"type":"object","properties":{"step":{"enum":["explore","edit","review"]},"model":{"enum":["haiku","sonnet","opus"]}},"required":["step","model"]}}},"required":["size","why","steps"]}"#;
// Small or large, and which model for exploring, editing and reviewing. { size, why, steps }.
pub fn classify(claude: &str, title: &str, body: &str) -> Result<Value, String> {
    let p = format!("Classify this ticket for a Claude Code session. small: one session does it. large: split it, exploring with a cheap subagent and reviewing with a strong one. Say why in one sentence.\n\nTicket: {}\n\n{}", head16(title, 300), head16(body, 3000));
    let v = haiku(claude, &p, Some(SCHEMA))?;
    let o = &v["structured_output"];
    if !o["size"].is_string() { return Err("no classification in the answer".into()); }
    Ok(json!({ "size": o["size"], "why": head16(o["why"].as_str().unwrap_or(""), 400), "steps": o["steps"], "by": "haiku" }))
}
// The gaps in a log (no steps, no result), filled from what git shows of the branch. Text for you, never sent on.
pub fn summarize(claude: &str, f: &Flow) -> Result<String, String> {
    let range = format!("{}..HEAD", f.base);
    let commits = git(&f.dir, &["log", "--format=- %s", &range]).out;
    let stat = git(&f.dir, &["diff", "--stat", &f.base]).out;
    let p = format!("Summarize this coding session's work for its developer in at most 8 short lines: what was done, what is left or blocked. Only from the facts below.\n\nWork log:\n{}\n\nCommits:\n{}\n\nChanged files:\n{}",
        head16(&f.log, 8000), head16(&commits, 3000), head16(&stat, 3000));
    let v = haiku(claude, &p, None)?;
    Ok(head16(v["result"].as_str().unwrap_or("").trim(), 2000).to_string())
}

// ---------- the flows, kept in Bobix's folder (this machine only) ----------
#[derive(Default)]
pub struct Flows { pub list: indexmap::IndexMap<String, Flow>, file: Option<PathBuf> }

impl Flows {
    pub fn load(file: PathBuf) -> Flows {
        let list = fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<Vec<Flow>>(&t).ok()).unwrap_or_default();
        Flows { list: list.into_iter().map(|f| (f.key.clone(), f)).collect(), file: Some(file) }
    }
    pub fn save(&self) {
        let Some(f) = &self.file else { return };
        let v: Vec<&Flow> = self.list.values().collect();
        if let Some(d) = f.parent() { let _ = fs::create_dir_all(d); }
        let _ = crate::paths::write_private(f, &serde_json::to_string_pretty(&v).unwrap_or_default());
    }
    pub fn by_dir(&mut self, dir: &str) -> Option<&mut Flow> { self.list.values_mut().find(|f| !f.cleaned && f.dir == dir) }
    pub fn by_session(&self, id: &str) -> Option<&Flow> { self.list.values().find(|f| f.session.as_deref() == Some(id)) }
    pub fn by_session_mut(&mut self, id: &str) -> Option<&mut Flow> { self.list.values_mut().find(|f| f.session.as_deref() == Some(id)) }

    // A hook event of the session working on a flow (or the first one started in its worktree). Returns true when the
    // flow changed. `plan`: the plan Claude asks you to approve (ExitPlanMode's input).
    pub fn on_event(&mut self, sid: &str, cwd: &str, ev: &str, tool: &str, plan: Option<&str>, now: u64) -> bool {
        let known = self.by_session(sid).is_some();
        let f = if known { self.by_session_mut(sid) } else { self.by_dir(cwd) };
        let Some(f) = f else { return false };
        let before = f.clone();
        if !known {
            // the first session started in its worktree takes the flow; a later one (a review) only refreshes its log
            if !(ev == "SessionStart" && f.session.is_none()) { f.reread(); return if *f != before { f.at = now; true } else { false }; }
            f.session = Some(sid.into());
            f.status = "planning".into();
        }
        if ev == "PermissionRequest" && tool == "ExitPlanMode" {
            f.status = "approve".into();
            if let Some(p) = plan { f.plan = head16(p, 20_000).to_string(); }
        }
        if ev == "UserPromptSubmit" && f.status == "approve" { f.status = "planning".into(); } // you answered in Claude instead
        f.reread();
        if *f != before { f.at = now; true } else { false }
    }
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
            .filter_map(|f| Some((f.key.clone(), tauri::Url::parse(&f.mr).ok()?.host_str()?.to_string(), f.path.clone(), mr_iid(&f.mr)?, f.github()))).collect()
    }
    // A new assumption since you last looked: the pet gets a mark (the session does not stop).
    pub fn marked(&self, sid: &str) -> bool { self.by_session(sid).is_some_and(|f| !f.cleaned && assumptions(&f.log) > f.seen) }

    // What the pages see: each flow with its log read into sections, and how much disk is left for more.
    pub fn view(&self, free: Option<u64>) -> Value {
        let list: Vec<Value> = self.list.values().rev().map(|f| {
            let mut v = serde_json::to_value(f).unwrap_or_default();
            let log = parse_log(&f.log);
            v["new"] = json!(assumptions(&f.log).saturating_sub(f.seen));
            v["parsed"] = log;
            v["cmd"] = json!(if f.cleaned || f.session.is_some() { String::new() } else { cli_command(&f.dir, &main_model(&f.models), "") });
            if matches!(f.status.as_str(), "done" | "blocked") && !f.cleaned && f.mr.is_empty() { v["mrTitle"] = json!(mr_title(f)); v["mrBody"] = json!(mr_body(f)); }
            v
        }).collect();
        json!({ "list": list, "free": free, "room": room(free), "need": WORKTREE_BYTES })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_branches_and_folders() {
        assert_eq!(slug("[Bug] Cart total ignores discount codes!"), "bug-cart-total-ignores-discount-codes");
        assert_eq!(slug("Sửa lỗi đăng nhập"), "s-a-l-i-ng-nh-p");
        assert_eq!(branch_for(7, "Add subtract()"), "7-add-subtract");
        assert_eq!(branch_for(8, "€€"), "8-ticket");
        assert!(slug(&"word ".repeat(40)).len() <= 40);
        assert_eq!(worktree_dir("/src/shop", 7), Path::new("/src/shop/.claude/worktrees/bobix-7"));
        assert_eq!(crate::model::issue_of_branch(&branch_for(42, "x")), Some(42), "the branch names its issue the way Bobix reads branches");
        assert_eq!(room(Some(WORKTREE_BYTES * 2 + 1)), Some(2));
        assert_eq!(room(Some(WORKTREE_BYTES - 1)), Some(0));
        assert!(free_bytes(Path::new(env!("CARGO_MANIFEST_DIR"))).is_some_and(|b| b > 0) || cfg!(windows));
    }

    #[test]
    fn the_log_reads_back_section_by_section() {
        let t = template(7, "Add subtract");
        assert_eq!(parse_log(&t), json!({ "status": "planning", "plan": "", "steps": [], "assumptions": [], "result": "", "checks": [], "ran": 0, "reviewed": false, "checksText": "" }));
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
    fn prompt_parts() {
        let r = rules("7-add-subtract");
        for must in ["EnterPlanMode", "ExitPlanMode", "Assumptions", "pushing to the main branch", "deleting data", "calling hosts", "touching secrets", ".bobix-log/work-log.md", "`7-add-subtract`", "## Checks", "tests, build and lint", "`- None: `", "/code-review", "`- Review: `"] {
            assert!(r.contains(must), "{must}");
        }
        assert!(models_text(&json!({ "size": "small" })).contains("one session"));
        let big = json!({ "size": "large", "steps": [{ "step": "explore", "model": "haiku" }, { "step": "edit", "model": "sonnet" }, { "step": "review", "model": "opus" }, { "step": "x", "model": "gpt" }] });
        let t = models_text(&big);
        assert!(t.contains("built-in Explore subagent") && t.contains("`sonnet` suits") && t.contains("model `opus`") && !t.contains("gpt"), "{t}");
        assert!(models_text(&json!({ "size": "large", "steps": [{ "step": "explore", "model": "sonnet" }] })).contains("read-only subagent on model `sonnet`"));
        let rp = review_prompt("7-add-subtract", "origin/HEAD");
        assert!(rp.contains("`git diff origin/HEAD...HEAD`") && rp.contains("/code-review") && rp.contains("Do not change any code") && rp.contains("- Review (second session): "), "{rp}");
        assert_eq!(cli_in_mode("/w", "opus", "auto", "r"), "cd '/w' && claude --model 'opus' --permission-mode auto 'r'");
        assert_eq!((main_model(&big), main_model(&json!({}))), ("sonnet".to_string(), "sonnet".to_string()));
        assert_eq!(cli_command("/src/it's", "opus", "do 'x'"), r#"cd '/src/it'\''s' && claude --model 'opus' --permission-mode plan 'do '\''x'\'''"#);
    }

    #[test]
    fn a_flow_goes_plan_approve_run_done_and_marks_new_assumptions() {
        let root = std::env::temp_dir().join(format!("bobix-flow-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let dir = root.join("wt");
        fs::create_dir_all(dir.join(LOG_DIR)).unwrap();
        let write = |t: &str| fs::write(dir.join(LOG_DIR).join(LOG_FILE), t).unwrap();
        write(&template(7, "Add subtract"));
        let d = dir.to_string_lossy().to_string();
        let mut fl = Flows::default();
        fl.list.insert("g/shop#7".into(), Flow { key: "g/shop#7".into(), path: "g/shop".into(), iid: 7, dir: d.clone(), status: "starting".into(), ..Default::default() });
        assert!(!fl.on_event("s1", "/elsewhere", "SessionStart", "", None, 1), "another folder");
        fl.on_event("s1", &d, "PreToolUse", "Read", None, 1);
        assert!(fl.list[0].session.is_none() && fl.list[0].status == "starting", "only a session that starts there takes it");
        assert!(fl.on_event("s1", &d, "SessionStart", "", None, 2));
        assert_eq!((fl.list[0].status.as_str(), fl.list[0].session.as_deref()), ("planning", Some("s1")));
        assert!(!fl.on_event("s2", &d, "SessionStart", "", None, 3), "taken: a second session there is not the flow's");
        assert!(!fl.on_event("s2", &d, "PermissionRequest", "ExitPlanMode", Some("other"), 3) && fl.list[0].status == "planning", "its plan is not the flow's");
        assert!(fl.on_event("s1", &d, "PermissionRequest", "ExitPlanMode", Some("1. add it\n## Assumptions\n- ints"), 4));
        assert_eq!((fl.list[0].status.as_str(), fl.list[0].plan.as_str()), ("approve", "1. add it\n## Assumptions\n- ints"));
        fl.answered("s1", true, 5);
        assert_eq!(fl.list[0].status, "running");
        assert!(!fl.marked("s1"));
        write(&template(7, "Add subtract").replace("## Assumptions\n", "## Assumptions\n- Integers only\n"));
        assert!(fl.on_event("s1", &d, "PostToolUse", "Edit", None, 6));
        assert!(fl.marked("s1"), "a new assumption marks the pet");
        fl.list[0].seen = 1;
        assert!(!fl.marked("s1"), "seen");
        write(&fl.list[0].log.replace("Status: planning", "Status: done"));
        assert!(fl.on_event("s1", &d, "Stop", "", None, 7));
        assert_eq!(fl.list[0].status, "done");
        // a review session in the worktree notes its result: the flow's log is read again on its events
        write(&fl.list[0].log.replace("## Checks\n", "## Checks\n- Review (second session): no issues\n"));
        assert!(fl.on_event("r1", &d, "Stop", "", None, 8) && fl.list[0].log.contains("second session") && fl.list[0].session.as_deref() == Some("s1"));
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

    #[test]
    fn merge_request_text_and_link() {
        let mut f = Flow { iid: 7, title: "Add subtract".into(), plan: "1. add it".into(), ..Default::default() };
        f.log = template(7, "Add subtract").replace("## Assumptions\n", "## Assumptions\n- ints only\n").replace("## Checks\n", "## Checks\n- `npm test`\n```\nok 2\n```\n");
        let b = mr_body(&f);
        assert!(b.starts_with("Closes #7\n\n## Plan\n\n1. add it\n\n## Assumptions\n\n- ints only\n\n## Checks\n\n- `npm test`\n```\nok 2\n```"), "{b}");
        f.log = template(7, "x");
        assert!(mr_body(&f).contains("## Checks\n\nNone pasted."), "no checks: said so");
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

    #[test]
    fn worktree_made_and_removed_with_git() {
        let root = std::env::temp_dir().join(format!("bobix-wt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let repo = root.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let r = repo.to_string_lossy().to_string();
        for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=demo@example.test", "-c", "user.name=demo", "commit", "-q", "--allow-empty", "-m", "init"]] { assert!(git(&r, a).ok); }
        let dir = worktree_dir(&r, 7);
        assert_eq!(create_worktree(&r, &dir, "7-add-subtract", "log").unwrap(), "HEAD");
        assert_eq!(read_log(&dir.to_string_lossy()).as_deref(), Some("log"));
        assert!(git(&dir.to_string_lossy(), &["status", "--porcelain"]).out.trim().is_empty(), "the log folder ignores itself");
        assert!(create_worktree(&r, &dir, "7-add-subtract", "log").is_err(), "already there");
        fs::write(dir.join("new.txt"), "x").unwrap();
        assert!(remove_worktree(&r, &dir.to_string_lossy()).is_err(), "work not committed: git refuses, Bobix never forces");
        fs::remove_file(dir.join("new.txt")).unwrap();
        remove_worktree(&r, &dir.to_string_lossy()).unwrap();
        assert!(!dir.exists());
        assert!(git(&r, &["rev-parse", "--verify", "refs/heads/7-add-subtract"]).ok, "the branch stays");
        assert_eq!(unpushed(&r, "7-add-subtract"), 1, "no remote has it");
        let _ = fs::remove_dir_all(&root);
    }
}
