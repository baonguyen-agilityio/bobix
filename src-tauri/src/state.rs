// Session state machine fed by Claude Code hook events. apply() is pure so it can be unit-tested.
// Port of the 0.1 (Electron) app's core/state.js: same fields, same rules, same JSON for the pages.
use crate::js::{head16, len16, now_ms, round};
use fancy_regex::{Captures, Regex};
use serde::Serialize;
use serde_json::Value;
use std::{fs, io::{Read, Seek, SeekFrom}, path::Path, sync::LazyLock};

#[derive(Serialize, Clone, Debug, Default)]
pub struct LogLine { pub t: u64, pub kind: String, pub text: String, pub tool: String }

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String, pub name: String, pub cwd: String, pub title: String, pub transcript: String, pub state: String,
    pub since: u64, pub updated: u64, pub lv: u64, pub xp: u64, pub quest: String, pub prompt: String, pub tool: String, pub tool_arg: String,
    pub ask_tool: String, pub ask: String, pub summary: String, pub report: String, pub hits: u64, pub ctx: Option<i64>,
    pub claude_pid: Option<u32>, pub host_session: Option<String>, pub log: Vec<LogLine>,
    pub pet: String, // auto (settings decide) | hidden | shown (stays out even past the sleep timer); back to auto when it needs you
    #[serde(skip_serializing_if = "Option::is_none")] pub diag: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")] pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub answer: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")] pub git: Option<Value>, // the folder's git status (work::git_status, remotes left out)
    #[serde(skip_serializing_if = "Option::is_none")] pub started: Option<Value>, // { path, iid }: started from that issue (New)
    // Bookkeeping, never sent to the pages (publicView in state.js).
    #[serde(skip)] pub read_at: u64,
    #[serde(skip)] pub read_mark: Option<String>,
    #[serde(skip)] pub parent_host: Option<String>,
    #[serde(skip)] pub link_tries: u32,
    #[serde(skip)] pub link_at: u64,
    #[serde(skip)] pub seen_at: u64, // when Bobix first heard of it
    #[serde(skip)] pub held: bool,   // linking() as last broadcast
    #[serde(skip)] pub link_known: bool, // its Claude app session file was read: the parent link, if any, is known
    #[serde(skip)] pub pid_checked: Option<u32>,
    #[serde(skip)] pub pid_is_claude: Option<u32>,
    #[serde(skip)] pub branches: Vec<String>,
    #[serde(skip)] pub waited: Vec<(u64, u64)>, // waits for your answer that ended, not yet in the day's log (keep_waits)
}

pub type Sessions = indexmap::IndexMap<String, Session>;

// A time Claude waited for your answer (ms), in which project (its folder's name). The pages add today's up.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Wait { pub from: u64, pub to: u64, pub project: String }
// Kept this long: enough for "today" in any time zone.
pub const WAITS_KEEP: u64 = 36 * 3600 * 1000;

// The wait going on ends now (an answer, a new prompt, an interrupted turn, the session gone).
pub fn leave_wait(s: &mut Session, now: u64) { if s.state == "waiting" { s.waited.push((s.since, now)); } }

// Moves the session's ended waits into the log, and forgets the ones older than WAITS_KEEP.
pub fn keep_waits(waits: &mut Vec<Wait>, s: &mut Session, now: u64) {
    let dir = s.project.as_deref().filter(|p| !p.is_empty()).unwrap_or(&s.cwd);
    let project = Path::new(dir).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| s.name.clone());
    waits.extend(s.waited.drain(..).filter(|(from, to)| to > from).map(|(from, to)| Wait { from, to, project: project.clone() }));
    waits.retain(|w| now.saturating_sub(w.to) <= WAITS_KEEP);
}

// Each finished task (Stop) is 1 XP; this many make a level (the pets overlay draws the XP bar from it: XP_PER_LV in pets.html).
pub const XP_PER_LV: u64 = 3;

fn new_session(id: &str, now: u64) -> Session {
    Session { id: id.into(), name: "?".into(), state: "idle".into(), since: now, updated: now, seen_at: now, lv: 1, pet: "auto".into(), ..Default::default() }
}

// A side session (started from another Claude desktop session) learns its parent from that app's session file, which
// shows up a moment after the session's first hook events (measured on 9 real side sessions: up to 4.6 s), with the
// link already in it. Until the file is there it is held back, for up to LINK_HOLD_MS, and looked up every
// LINK_FAST_MS, so its pet shows once, already by its parent, instead of on its own first and then jumping over
// (another colour and size). A file with no link ends the hold at once. No file after the hold: looked up every 5 s,
// for about 12 minutes.
pub const LINK_HOLD_MS: u64 = 8000;
const LINK_FAST_MS: u64 = 400;
pub fn linking(s: &Session, now: u64) -> bool {
    s.host_session.is_some() && s.parent_host.is_none() && !s.link_known && now.saturating_sub(s.seen_at) < LINK_HOLD_MS
}
pub fn find_parent(s: &mut Session, now: u64, lookup: impl Fn(&str) -> Option<Option<String>>) -> bool {
    let Some(host) = s.host_session.clone() else { return false };
    let every = if linking(s, now) { LINK_FAST_MS } else { 5000 };
    if s.link_known || s.link_tries >= 150 || (s.link_tries > 0 && now.saturating_sub(s.link_at) < every) { return false; }
    s.link_at = now;
    s.link_tries += 1;
    let Some(parent) = lookup(&host) else { return false };
    s.link_known = true;
    s.parent_host = parent;
    true
}

pub fn valid_id(id: &str) -> bool {
    (1..=128).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

// ---------- JavaScript value helpers ----------
fn truthy(v: &Value) -> bool {
    match v { Value::Null => false, Value::Bool(b) => *b, Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()), Value::String(s) => !s.is_empty(), _ => true }
}
// String(v ?? '')
fn js_string(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.as_f64().map(|x| if x.fract() == 0.0 && x.abs() < 1e21 { format!("{}", x as i64) } else { format!("{x}") }).unwrap_or_default(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Array(a)) => a.iter().map(|x| js_string(Some(x))).collect::<Vec<_>>().join(","),
        Some(Value::Object(_)) => "[object Object]".into(),
    }
}
// String(v || '')
fn js_string_or(v: Option<&Value>) -> String { v.filter(|x| truthy(x)).map(|x| js_string(Some(x))).unwrap_or_default() }

// ---------- text ----------
pub fn short(s: &str, n: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if len16(&s) > n { format!("{}…", head16(&s, n - 1)) } else { s }
}

// Like short, but keeps the lines (the prompt and the final report are Markdown): only blank-line runs and trailing
// spaces go. A longer text is cut at a line end when one is near, an open code block is closed, and "[…]" on its own
// line shows the cut.
pub fn clip(s: &str, n: usize) -> String {
    let s = s.replace("\r\n", "\n").replace('\r', "\n");
    let s = s.split('\n').map(|l| l.trim_end_matches([' ', '\t'])).collect::<Vec<_>>().join("\n");
    static RUNS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").unwrap());
    let s = RUNS.replace_all(&s, "\n\n").trim().to_string();
    if len16(&s) <= n { return s; }
    let mut s = head16(&s, n - 24).to_string(); // room for the closing fence and the mark
    if let Some(nl) = s.rfind('\n') {
        if len16(&s[..nl]) as i64 > len16(&s) as i64 - 2000 { s = s[..nl].trim_end().to_string(); }
    }
    static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^[ \t]*(`{3,}|~{3,})").unwrap());
    let fences: Vec<String> = FENCE.find_iter(&s).flatten().map(|m| m.as_str().trim().to_string()).collect();
    let close = if fences.len() % 2 == 1 { format!("\n{}", fences.last().unwrap()) } else { String::new() };
    format!("{s}{close}\n\n[…]")
}

// Kept whole up to here, so pasted reports of 10–20k fit. Still capped: the snapshot carries both texts of every
// session to the panel and the pets on every hook event.
pub const TEXT_MAX: usize = 32000;

// The answer's Markdown as plain text, for one-line previews and notifications (the panel renders the report itself).
pub fn plain(s: &str) -> String {
    static R: LazyLock<Vec<(Regex, &str)>> = LazyLock::new(|| [
        (r"(?m)^[ \t]*```.*$", ""),
        (r"(?m)^[ \t]*\|?[ \t]*:?-+:?[ \t]*\|[-|: \t]*$", ""),
        (r"(?m)^[ \t]*([-*_])([ \t]*\1){2,}[ \t]*$", ""),
    ].into_iter().map(|(r, t)| (Regex::new(r).unwrap(), t)).collect());
    static ROW: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^[ \t]*\|(.*)\|[ \t]*$").unwrap());
    static CELL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?<!\\)\|").unwrap());
    static R2: LazyLock<Vec<(Regex, &str)>> = LazyLock::new(|| [
        (r"(?m)^[ \t]*(#{1,6}|[-*+]|[0-9]+[.)]|>)[ \t]+", ""),
        (r"!?\[([^\]]*)\]\([^)\s]*\)", "$1"),
        (r"(\*\*|~~|\*)(?=\S)(.+?)(?<=\S)\1", "$2"),
        (r"(?<![A-Za-z0-9_])(__|_)(?=\S)(.+?)(?<=\S)\1(?![A-Za-z0-9_])", "$2"),
        (r"\*\*|`", ""),
    ].into_iter().map(|(r, t)| (Regex::new(r).unwrap(), t)).collect());
    let mut s = s.to_string();
    for (r, t) in R.iter() { s = r.replace_all(&s, *t).into_owned(); }
    s = ROW.replace_all(&s, |c: &Captures| {
        let row = &c[1];
        let mut cells = Vec::new();
        let mut last = 0;
        for m in CELL.find_iter(row).flatten() { cells.push(&row[last..m.start()]); last = m.end(); }
        cells.push(&row[last..]);
        cells.iter().map(|x| x.trim().replace("\\|", "|")).collect::<Vec<_>>().join(" · ")
    }).into_owned();
    for (r, t) in R2.iter() { s = r.replace_all(&s, *t).into_owned(); }
    s
}

fn arg_of(input: Option<&Value>) -> String {
    let Some(i) = input else { return String::new() };
    ["file_path", "command", "pattern", "url", "query", "description"].iter()
        .find_map(|k| i.get(*k).filter(|v| truthy(v))).map(|v| js_string(Some(v))).unwrap_or_default()
}
fn tool_arg(input: Option<&Value>) -> String { short(&arg_of(input), 60) }

// Claude Code also sends its own messages through UserPromptSubmit (a background task ended, a reminder…). They are
// not something the user asked: returns a short line for the log, or None for a real prompt. Only these exact tags.
fn system_prompt(prompt: &str) -> Option<String> {
    static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*<(task-notification|system-reminder|side-sessions-event|ci-monitor-event|cross-session-message|local-command-caveat|local-command-stdout|command-name|command-message|bash-input|bash-stdout)[\s>]").unwrap());
    static INNER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<(?:summary|command-name)>([\s\S]*?)</").unwrap());
    let m = TAG.captures(prompt).ok()??;
    let text = match INNER.captures(prompt).ok().flatten() { Some(c) => c[1].to_string(), None => m[1].replace('-', " ") };
    Some(short(&text, 120))
}

pub struct Notice { pub kind: &'static str, pub id: String }
pub enum Applied { Updated { id: String, notice: Option<Notice> }, Removed(Box<Session>) }

// Returns the session touched (with a notice for things the user should hear about), a removal, or None.
pub fn apply(sessions: &mut Sessions, e: &Value, now: u64) -> Option<Applied> {
    let id = e.get("session_id")?.as_str().filter(|id| valid_id(id))?.to_string();
    let ev = e.get("hook_event_name").and_then(|v| v.as_str()).unwrap_or("");
    if ev == "SessionEnd" { return sessions.shift_remove(&id).map(|mut s| { leave_wait(&mut s, now); Applied::Removed(Box::new(s)) }); }

    let s = sessions.entry(id.clone()).or_insert_with(|| new_session(&id, now));
    s.updated = now;
    if let Some(cwd) = e.get("cwd").and_then(|v| v.as_str()).filter(|c| !c.is_empty()) {
        s.cwd = cwd.into();
        s.name = Path::new(cwd).file_name().map(|n| n.to_string_lossy().into_owned()).filter(|n| !n.is_empty()).unwrap_or_else(|| cwd.into());
    }
    if let Some(t) = e.get("transcript_path").and_then(|v| v.as_str()).filter(|t| t.ends_with(".jsonl")) { s.transcript = t.into(); }
    if let Some(p) = e.get("bobix_claude_pid").and_then(|v| v.as_u64()).filter(|p| *p > 1 && *p <= u32::MAX as u64) { s.claude_pid = Some(p as u32); }
    if let Some(env) = e.get("bobix_env").and_then(|v| v.as_array()) {
        let keys: Vec<Value> = env.iter().filter(|k| k.is_string()).take(40).cloned().collect();
        let shape = e.get("bobix_host_shape").and_then(|v| v.as_str()).map(|h| Value::from(head16(h, 80)));
        s.diag = Some(serde_json::json!({ "env": keys, "hostShape": shape }));
    }
    if let Some(h) = e.get("bobix_host_session").and_then(|v| v.as_str()).filter(|h| crate::links::is_host_id(h)) { s.host_session = Some(h.into()); }

    fn log(s: &mut Session, now: u64, kind: &str, text: String, tool: String) {
        s.log.push(LogLine { t: now, kind: kind.into(), text, tool });
        if s.log.len() > 40 { s.log.remove(0); }
    }
    fn set(s: &mut Session, now: u64, st: &str) { if s.state != st { leave_wait(s, now); s.state = st.into(); s.since = now; } }
    fn ask_for(s: &mut Session, now: u64, tool: String, text: &str) -> Option<Notice> {
        let first = s.state != "waiting";
        if !tool.is_empty() { s.ask_tool = tool; }
        s.ask = short(text, 160);
        let (a, t) = (s.ask.clone(), s.ask_tool.clone());
        log(s, now, "ask", a, t);
        set(s, now, "waiting");
        s.pet = "auto".into();
        first.then(|| Notice { kind: "waiting", id: s.id.clone() }) // Notification + PermissionRequest both fire: notify once
    }
    let mut notice = None;
    let tool_name = js_string_or(e.get("tool_name"));

    match ev {
        "SessionStart" => { let src = js_string_or(e.get("source")); log(s, now, "start", if src.is_empty() { "startup".into() } else { src }, String::new()); }
        "UserPromptSubmit" => {
            let prompt = js_string(e.get("prompt"));
            if let Some(sys) = system_prompt(&prompt) {
                // not a new quest: keep the quest, the report and hidden pets as they are
                s.tool.clear(); s.tool_arg.clear();
                log(s, now, "system", sys, String::new());
                set(s, now, "working");
            } else {
                s.quest = short(&plain(head16(&prompt, 2000)), 240);
                s.prompt = clip(&prompt, TEXT_MAX);
                s.hits = 0;
                for f in [&mut s.summary, &mut s.report, &mut s.ask, &mut s.ask_tool, &mut s.tool, &mut s.tool_arg] { f.clear(); }
                let q = s.quest.clone();
                log(s, now, "prompt", q, String::new());
                set(s, now, "working");
                s.pet = "auto".into();
            }
        }
        "PreToolUse" => {
            s.tool = tool_name; s.tool_arg = tool_arg(e.get("tool_input")); s.hits += 1;
            if s.state == "waiting" { s.ask.clear(); s.ask_tool.clear(); }
            let t = s.tool.clone();
            log(s, now, "tool", short(&arg_of(e.get("tool_input")), 300), t); // the panel shows it whole on hover
            set(s, now, "working");
        }
        "PostToolUse" => {
            // Only the tool that asked clears "waiting": parallel tools / subagents finish while it still waits.
            if s.state == "waiting" && (s.ask_tool.is_empty() || tool_name == s.ask_tool) { s.ask.clear(); s.ask_tool.clear(); set(s, now, "working"); }
        }
        "PermissionRequest" => {
            let text = format!("{} {}", tool_name, tool_arg(e.get("tool_input")));
            notice = ask_for(s, now, tool_name, text.trim());
        }
        "Notification" => {
            // "Claude needs your permission to use X" → waiting. "waiting for your input" (after Stop) → ignore.
            let m = js_string_or(e.get("message"));
            if m.to_lowercase().contains("permission") {
                static USE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)use (\S+)").unwrap());
                let tool = USE.captures(&m).ok().flatten().map(|c| c[1].to_string()).unwrap_or_default();
                notice = ask_for(s, now, tool, &m);
            }
        }
        "Stop" => {
            s.xp += 1;
            if s.xp >= XP_PER_LV { s.lv += 1; s.xp = 0; }
            s.ask.clear(); s.ask_tool.clear();
            log(s, now, "done", String::new(), String::new());
            set(s, now, "done");
            notice = Some(Notice { kind: "done", id: id.clone() });
        }
        _ => {}
    }
    Some(Applied::Updated { id, notice })
}

// ---------- transcript ----------
#[derive(Debug, Default, PartialEq)]
pub struct TranscriptInfo { pub text: Option<String>, pub usage: Option<Value>, pub title: Option<String>, pub interrupted: bool, pub branches: Vec<String> }

const TAIL: u64 = 512 * 1024;

// Reads the transcript's tail (last answer + token usage) and titles. Titles can sit far from the end, so the head is
// scanned too. interrupted: the latest turn was cut short (Esc, or a denied permission). Claude runs no Stop hook then.
pub fn read_transcript(file: &str) -> TranscriptInfo {
    let mut out = TranscriptInfo::default();
    let Ok(meta) = fs::metadata(file) else { return out };
    if !meta.is_file() { return out; } // never open a FIFO / device: that would block
    let Ok(mut f) = fs::File::open(file) else { return out };
    let size = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mut chunk = |pos: u64, len: u64| -> Vec<String> {
        let mut b = vec![0u8; len as usize];
        if f.seek(SeekFrom::Start(pos)).is_err() { return vec![]; }
        let n = f.read(&mut b).unwrap_or(0);
        b.truncate(n);
        String::from_utf8_lossy(&b).split('\n').map(String::from).collect()
    };
    let (mut custom, mut ai): (Option<String>, Option<String>) = (None, None);
    // keep_first: scanning backwards, the first title met is the newest one.
    let titles = |j: &Value, keep_first: bool, custom: &mut Option<String>, ai: &mut Option<String>| {
        if j["type"] == "custom-title" { if let Some(t) = j["customTitle"].as_str().filter(|t| !t.is_empty()) { if !(keep_first && custom.is_some()) { *custom = Some(t.into()); } } }
        if j["type"] == "ai-title" { if let Some(t) = j["aiTitle"].as_str().filter(|t| !t.is_empty()) { if !(keep_first && ai.is_some()) { *ai = Some(t.into()); } } }
    };
    let tail = chunk(size.saturating_sub(TAIL), size.min(TAIL));
    let mut last_turn_seen = false;
    for line in tail.iter().rev() {
        let Ok(j) = serde_json::from_str::<Value>(line) else { continue };
        titles(&j, true, &mut custom, &mut ai);
        if let Some(b) = j["gitBranch"].as_str().filter(|b| !b.is_empty() && len16(b) < 256) {
            if out.branches.first().map(String::as_str) != Some(b) { out.branches.insert(0, b.into()); }
        }
        let side = truthy(&j["isSidechain"]);
        if !last_turn_seen && (j["type"] == "user" || j["type"] == "assistant") && !side {
            last_turn_seen = true;
            let c = &j["message"]["content"];
            let txt = match c { Value::String(s) => s.clone(), Value::Array(a) => a.iter().map(|x| x["text"].as_str().unwrap_or("")).collect::<Vec<_>>().join(" "), _ => String::new() };
            out.interrupted = j["type"] == "user" && txt.contains("[Request interrupted by user");
        }
        if j["type"] != "assistant" || !truthy(&j["message"]) || side { continue; }
        if out.usage.is_none() { out.usage = j["message"].get("usage").filter(|u| truthy(u)).cloned(); }
        if out.text.is_none() {
            if let Some(a) = j["message"]["content"].as_array() {
                let t = a.iter().filter(|c| c["type"] == "text").map(|c| js_string(c.get("text"))).collect::<Vec<_>>().join("\n").trim().to_string();
                if !t.is_empty() { out.text = Some(t); }
            }
        }
    }
    if custom.is_none() && ai.is_none() && size > TAIL {
        for l in chunk(0, size.min(256 * 1024)) {
            if let Ok(j) = serde_json::from_str::<Value>(&l) { titles(&j, false, &mut custom, &mut ai); }
        }
    }
    out.title = custom.or(ai);
    // each once, at its newest place
    let b = std::mem::take(&mut out.branches);
    out.branches = b.iter().enumerate().filter(|(i, x)| b.iter().rposition(|y| y == *x) == Some(*i)).map(|(_, x)| x.clone()).collect();
    out
}

pub fn ctx_left(u: &Value, forced_window: Option<f64>) -> Option<i64> {
    if !truthy(u) { return None; }
    let n = |k: &str| u.get(k).and_then(|v| v.as_f64()).filter(|x| *x != 0.0 && !x.is_nan()).unwrap_or(0.0);
    let used = n("input_tokens") + n("cache_read_input_tokens") + n("cache_creation_input_tokens") + n("output_tokens");
    // ponytail: window guessed from usage (200k, else 1M); pass forced_window when known.
    let win = forced_window.unwrap_or(if used > 200000.0 { 1000000.0 } else { 200000.0 });
    Some(round(100.0 - used / win * 100.0).max(0.0) as i64)
}

// Refresh title/context (throttled) and the final answer (on Stop) from the transcript. True when the state changed.
pub fn refresh(s: &mut Session, with_summary: bool, now: u64) -> bool {
    if s.transcript.is_empty() || (!with_summary && now.saturating_sub(s.read_at) < 5000) { return false; }
    s.read_at = now;
    let mark = fs::metadata(&s.transcript).ok().map(|m| format!("{}:{:?}", m.len(), m.modified().ok()));
    if !with_summary && mark.is_some() && mark == s.read_mark { return false; } // nothing new written: skip the read
    s.read_mark = mark;
    let r = read_transcript(&s.transcript);
    if let Some(u) = &r.usage { s.ctx = ctx_left(u, None); }
    if let Some(t) = &r.title { s.title = short(t, 80); }
    // Branches seen in the transcript, kept across reads (the tail moves on): a session may switch branches mid-way.
    if !r.branches.is_empty() {
        let mut b: Vec<String> = s.branches.iter().filter(|x| !r.branches.contains(x)).cloned().chain(r.branches.iter().cloned()).collect();
        if b.len() > 20 { b.drain(..b.len() - 20); }
        s.branches = b;
    }
    if with_summary { if let Some(t) = &r.text { s.summary = short(&plain(t), 600); s.report = clip(t, TEXT_MAX); } }
    if r.interrupted && (s.state == "working" || s.state == "waiting") {
        leave_wait(s, now);
        s.ask.clear(); s.ask_tool.clear(); s.state = "idle".into(); s.since = now;
        return true;
    }
    false
}

// The project of a session, kept on it. Claude cds out of it for a while (into a scratchpad, /tmp…): a folder outside
// every git repository keeps the project the session has, or its pet would turn into another project's pet.
pub fn pin_project(s: &mut Session) -> String {
    let p = crate::project::project_of(&s.cwd);
    if s.project.is_none() || (Path::new(&p).is_absolute() && Path::new(&p).join(".git").exists()) { s.project = Some(p); }
    s.project.clone().unwrap_or_default()
}

// A process name that is claude itself (or node, for an npm install).
pub fn is_claude_proc(name: &str) -> bool { name.eq_ignore_ascii_case("claude") || name.eq_ignore_ascii_case("node") }

// Sessions whose claude process went away without a SessionEnd. Only a pid once seen running as claude counts.
pub fn gone(sessions: &Sessions, is_alive: impl Fn(u32) -> bool) -> Vec<String> {
    sessions.iter().filter(|(_, s)| s.claude_pid.is_some() && s.pid_is_claude == s.claude_pid && !is_alive(s.claude_pid.unwrap())).map(|(id, _)| id.clone()).collect()
}

// Hide / show one pet by hand; "*" brings every pet back (and keeps sleeping ones out).
pub fn set_pet(sessions: &mut Sessions, id: &str, mode: &str) -> bool {
    if !["auto", "hidden", "shown"].contains(&mode) { return false; }
    let mut n = 0;
    for (k, s) in sessions.iter_mut() { if id == "*" || k == id { s.pet = mode.into(); n += 1; } }
    n > 0
}

pub fn now() -> u64 { now_ms() }

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tmp(name: &str) -> String { std::env::temp_dir().join(format!("bobix-{name}-{}.jsonl", std::process::id())).to_string_lossy().into_owned() }
    fn ap(m: &mut Sessions, e: Value) -> Option<Applied> { apply(m, &e, now()) }
    fn notice(r: Option<Applied>) -> Option<&'static str> { match r { Some(Applied::Updated { notice, .. }) => notice.map(|n| n.kind), _ => None } }

    // The timing side sessions really have (9 measured): the first hook events come up to 4.6 s before the Claude
    // app writes the session file, and the file carries the link from the start.
    #[test]
    fn side_session_held_until_linked_then_shown_once() {
        let t0 = 1_000_000;
        let ev = |name: &str| json!({ "session_id": "kid", "cwd": "/x/app", "hook_event_name": name, "bobix_host_session": "local_kid" });
        for file_at in [0, 900, 4600, 7000] {
            let mut m = Sessions::new();
            let lookup = |now: u64| move |_: &str| (now >= t0 + file_at).then(|| Some("local_par".to_string()));
            let mut seen = vec![];
            for (dt, name) in [(0, "SessionStart"), (150, "UserPromptSubmit"), (300, "PreToolUse")] {
                apply(&mut m, &ev(name), t0 + dt);
                let s = m.get_mut("kid").unwrap();
                find_parent(s, t0 + dt, lookup(t0 + dt));
                seen.push((linking(s, t0 + dt), s.parent_host.is_some()));
            }
            for t in (t0 + 400..t0 + 10_000).step_by(400) { let s = m.get_mut("kid").unwrap(); find_parent(s, t, lookup(t)); seen.push((linking(s, t), s.parent_host.is_some())); }
            // Shown standalone (not held, no parent) never; held until linked; linked once and for good.
            assert!(seen.iter().all(|&(held, linked)| held || linked), "file at {file_at}: {seen:?}");
            let first = seen.iter().position(|x| x.1).unwrap();
            assert!(seen[first..].iter().all(|x| x.1 && !x.0) && seen[..first].iter().all(|x| x.0 && !x.1));
        }
        // A session of the app with no parent: shown as soon as its file is read, once.
        let shown = |file_at: u64| -> Vec<bool> {
            let mut top = new_session("top", t0);
            top.host_session = Some("local_top".into());
            (0..30).map(|i| { let t = t0 + i * 400; find_parent(&mut top, t, |_| (t >= t0 + file_at).then_some(None)); !linking(&top, t) }).collect()
        };
        let s = shown(1200);
        assert_eq!(s.iter().position(|x| *x), Some(3), "shown the tick its file is read");
        assert!(s[3..].iter().all(|x| *x), "and for good");
        // No file at all: shown after the hold, for good.
        let s = shown(u64::MAX / 2);
        assert_eq!(s.iter().position(|x| *x), Some((LINK_HOLD_MS / 400) as usize));
        assert!(s[(LINK_HOLD_MS / 400) as usize..].iter().all(|x| *x));
        assert!(!linking(&new_session("cli", t0), t0), "a terminal session is never held");
    }

    #[test]
    fn session_lifecycle() {
        let mut m = Sessions::new();
        let base = |ev: &str, extra: Value| { let mut e = json!({ "session_id": "abc", "cwd": "/x/my-app", "hook_event_name": ev }); for (k, v) in extra.as_object().unwrap() { e[k] = v.clone(); } e };
        ap(&mut m, base("SessionStart", json!({ "bobix_claude_pid": 42, "bobix_host_session": "local_abc-1" })));
        assert_eq!(m["abc"].name, "my-app");
        assert_eq!(m["abc"].claude_pid, Some(42));
        assert_eq!(m["abc"].host_session.as_deref(), Some("local_abc-1"));
        ap(&mut m, base("PreToolUse", json!({ "bobix_host_session": "local_x; rm -rf /" })));
        assert_eq!(m["abc"].host_session.as_deref(), Some("local_abc-1"));
        ap(&mut m, base("UserPromptSubmit", json!({ "prompt": "Fix bug" })));
        assert_eq!(m["abc"].state, "working");
        ap(&mut m, base("PreToolUse", json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } })));
        assert_eq!(m["abc"].hits, 1);
        assert_eq!(notice(ap(&mut m, base("PermissionRequest", json!({ "tool_name": "Bash", "tool_input": { "command": "rm x" } })))), Some("waiting"));
        assert_eq!(notice(ap(&mut m, base("Notification", json!({ "message": "Claude needs your permission to use Bash" })))), None);
        assert_eq!(m["abc"].ask_tool, "Bash");
        ap(&mut m, base("PostToolUse", json!({ "tool_name": "Read" })));
        assert_eq!(m["abc"].state, "waiting");
        ap(&mut m, base("PostToolUse", json!({ "tool_name": "Bash" })));
        assert_eq!(m["abc"].state, "working");
        assert_eq!(notice(ap(&mut m, base("Stop", json!({})))), Some("done"));
        assert_eq!((m["abc"].lv, m["abc"].xp), (1, 1), "one finished task is 1 XP");
        for _ in 1..XP_PER_LV { ap(&mut m, base("Stop", json!({}))); }
        assert_eq!((m["abc"].lv, m["abc"].xp), (2, 0), "XP_PER_LV tasks make a level");
        assert_eq!(notice(ap(&mut m, base("Notification", json!({ "message": "Claude is waiting for your input" })))), None);
        assert_eq!(m["abc"].state, "done");
        assert!(matches!(ap(&mut m, base("SessionEnd", json!({}))), Some(Applied::Removed(_))));
        assert!(m.is_empty());
        assert!(ap(&mut m, json!({ "hook_event_name": "Stop" })).is_none());
        assert!(ap(&mut m, json!({ "session_id": "../etc", "hook_event_name": "Stop" })).is_none());
    }

    #[test]
    fn transcript_answer_usage_titles() {
        let f = tmp("tr");
        let lines = [
            json!({ "type": "ai-title", "aiTitle": "Old AI title" }), json!({ "type": "custom-title", "customTitle": "Old name" }),
            json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": "Done: fixed it" }], "usage": { "input_tokens": 10, "cache_read_input_tokens": 99990 } } }),
            json!({ "type": "custom-title", "customTitle": "Renamed" }), json!({ "type": "ai-title", "aiTitle": "New AI title" }),
        ];
        fs::write(&f, lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n")).unwrap();
        let r = read_transcript(&f);
        fs::remove_file(&f).unwrap();
        assert_eq!(r.text.as_deref(), Some("Done: fixed it"));
        assert_eq!(r.title.as_deref(), Some("Renamed"));
        assert_eq!(ctx_left(r.usage.as_ref().unwrap(), None), Some(50));
        assert_eq!(read_transcript("/nope/missing.jsonl"), TranscriptInfo::default());
    }

    #[test]
    fn transcript_branches_oldest_first() {
        let f = tmp("br");
        let l = ["main", "12-fix", "main", "12-fix", "12-fix"].map(|b| json!({ "type": "user", "gitBranch": b }).to_string());
        fs::write(&f, l.join("\n")).unwrap();
        assert_eq!(read_transcript(&f).branches, ["main", "12-fix"]);
        fs::remove_file(&f).unwrap();
    }

    #[test]
    fn system_messages_are_not_quests() {
        let mut m = Sessions::new();
        let p = |m: &mut Sessions, prompt: &str| { ap(m, json!({ "session_id": "q1", "hook_event_name": "UserPromptSubmit", "prompt": prompt })); };
        p(&mut m, "Fix the login bug");
        ap(&mut m, json!({ "session_id": "q1", "hook_event_name": "Stop" }));
        m.get_mut("q1").unwrap().pet = "hidden".into();
        p(&mut m, "<task-notification>\n<task-id>abc</task-id>\n<tool-use-id>x</tool-use-id>\n<summary>Background command \"Run tests\" completed</summary>\n</task-notification>");
        let s = &m["q1"];
        assert_eq!(s.quest, "Fix the login bug");
        assert_eq!(s.pet, "hidden");
        assert_eq!(s.state, "working");
        let l = s.log.last().unwrap();
        assert_eq!((l.kind.as_str(), l.text.as_str()), ("system", "Background command \"Run tests\" completed"));
        p(&mut m, "<system-reminder>\nsomething long\n</system-reminder>");
        assert_eq!(m["q1"].log.last().unwrap().text, "system reminder");
        p(&mut m, "<div>why is this HTML broken?</div>");
        assert_eq!(m["q1"].quest, "<div>why is this HTML broken?</div>");
        assert_eq!(m["q1"].pet, "auto");
    }

    #[test]
    fn interrupted_turn_rests() {
        let f = tmp("int");
        fs::write(&f, [json!({ "type": "assistant", "message": { "content": [{ "type": "tool_use" }] } }),
            json!({ "type": "user", "message": { "content": [{ "type": "text", "text": "[Request interrupted by user for tool use]" }] } })]
            .iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n")).unwrap();
        let mut m = Sessions::new();
        ap(&mut m, json!({ "session_id": "i1", "hook_event_name": "PermissionRequest", "tool_name": "Bash", "transcript_path": f }));
        assert_eq!(m["i1"].state, "waiting");
        assert!(refresh(m.get_mut("i1").unwrap(), false, now()));
        assert_eq!(m["i1"].state, "idle");
        assert_eq!(m["i1"].waited.len(), 1, "an interrupted turn ends the wait");
        m.get_mut("i1").unwrap().state = "working".into();
        assert!(!refresh(m.get_mut("i1").unwrap(), false, now() + 6000), "an unchanged transcript is not read again");
        fs::remove_file(&f).unwrap();
    }

    #[test]
    fn report_keeps_lines_summary_short() {
        let f = tmp("rep");
        let text = format!("## Done\n\n- one\n- two\n\n\n\n{}", "x".repeat(40000));
        fs::write(&f, json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": text }] } }).to_string()).unwrap();
        let mut m = Sessions::new();
        ap(&mut m, json!({ "session_id": "r1", "hook_event_name": "UserPromptSubmit", "prompt": "go", "transcript_path": f }));
        refresh(m.get_mut("r1").unwrap(), true, now());
        let s = &m["r1"];
        assert!(s.report.starts_with("## Done\n\n- one\n- two\n\nxxx"));
        assert!(s.report.len() <= 32000 && s.report.len() > 31000);
        assert!(s.report.ends_with("x\n\n[…]"));
        assert_eq!(len16(&s.summary), 600);
        assert!(!s.summary.contains('\n'));
        assert!(s.summary.starts_with("Done one two xxx"));
        ap(&mut m, json!({ "session_id": "r1", "hook_event_name": "UserPromptSubmit", "prompt": "again" }));
        assert_eq!(m["r1"].report, "");
        fs::remove_file(&f).unwrap();
    }

    #[test]
    fn plain_drops_markdown() {
        assert_eq!(plain("## Title\n- **bold** and `code` [link](http://x)\n| a | b \\| c |\n|---|---|\n_it_ snake_case"),
            "Title\nbold and code link\na · b | c\n\nit snake_case");
    }

    #[test]
    fn gone_only_checked_claude_pids() {
        let mut m = Sessions::new();
        for (id, pid) in [("dead", 101), ("live", 102), ("shell", 103), ("unchecked", 104), ("moved", 105)] {
            ap(&mut m, json!({ "session_id": id, "hook_event_name": "UserPromptSubmit", "prompt": "x", "bobix_claude_pid": pid }));
        }
        m.get_mut("dead").unwrap().pid_is_claude = Some(101);
        m.get_mut("live").unwrap().pid_is_claude = Some(102);
        m.get_mut("moved").unwrap().pid_is_claude = Some(99);
        assert_eq!(gone(&m, |p| p == 102), ["dead"]);
        assert!(["claude", "node", "Claude"].iter().all(|n| is_claude_proc(n)));
        assert!(["sh", "bash", "zsh", "cmd", "powershell", "Bobix", "Electron", "?", "claude-desktop"].iter().all(|n| !is_claude_proc(n)));
        let v = serde_json::to_value(&m["dead"]).unwrap();
        assert!(v.get("pidIsClaude").is_none() && v.get("pidChecked").is_none() && v.get("branches").is_none());
    }

    #[test]
    fn hidden_pet_returns_when_needed() {
        let mut m = Sessions::new();
        ap(&mut m, json!({ "session_id": "h1", "hook_event_name": "UserPromptSubmit", "prompt": "x" }));
        assert!(set_pet(&mut m, "h1", "hidden"));
        assert!(!set_pet(&mut m, "h1", "bogus"));
        ap(&mut m, json!({ "session_id": "h1", "hook_event_name": "PermissionRequest", "tool_name": "Bash" }));
        assert_eq!(m["h1"].pet, "auto");
    }

    #[test]
    fn waits_for_an_answer_are_logged() {
        let mut m = Sessions::new();
        let at = |m: &mut Sessions, ev: &str, t: u64, extra: Value| { let mut e = json!({ "session_id": "w1", "cwd": "/src/shop/sub", "hook_event_name": ev }); for (k, v) in extra.as_object().unwrap() { e[k] = v.clone(); } apply(m, &e, t) };
        at(&mut m, "UserPromptSubmit", 1_000, json!({ "prompt": "go" }));
        m.get_mut("w1").unwrap().project = Some("/src/shop".into());
        at(&mut m, "PermissionRequest", 2_000, json!({ "tool_name": "Bash" }));
        at(&mut m, "Notification", 2_500, json!({ "message": "Claude needs your permission to use Bash" }));
        at(&mut m, "PostToolUse", 3_000, json!({ "tool_name": "Read" })); // another tool: still waiting
        at(&mut m, "PostToolUse", 62_000, json!({ "tool_name": "Bash" })); // answered
        at(&mut m, "Notification", 70_000, json!({ "message": "Claude needs your permission to use Edit" }));
        at(&mut m, "UserPromptSubmit", 90_000, json!({ "prompt": "no, do this" })); // a new prompt ends it too
        let mut log = vec![];
        keep_waits(&mut log, m.get_mut("w1").unwrap(), 90_000);
        assert_eq!(log, [Wait { from: 2_000, to: 62_000, project: "shop".into() }, Wait { from: 70_000, to: 90_000, project: "shop".into() }]);
        assert!(m["w1"].waited.is_empty(), "moved, not copied");
        at(&mut m, "PermissionRequest", 100_000, json!({ "tool_name": "Bash" }));
        let Some(Applied::Removed(mut gone)) = at(&mut m, "SessionEnd", 130_000, json!({})) else { panic!("removed") };
        keep_waits(&mut log, &mut gone, 130_000);
        assert_eq!(log.last(), Some(&Wait { from: 100_000, to: 130_000, project: "shop".into() }), "the session ended while it waited");
        keep_waits(&mut log, &mut Session::default(), 90_000 + WAITS_KEEP + 1);
        assert_eq!(log.iter().map(|w| w.from).collect::<Vec<_>>(), [100_000], "older ones are forgotten");
        assert!(serde_json::to_value(Session { waited: vec![(1, 2)], ..Default::default() }).unwrap().get("waited").is_none(), "never sent to the pages");
    }
}
