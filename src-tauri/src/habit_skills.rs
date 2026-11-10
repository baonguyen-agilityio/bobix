// Habits become personal skills. Only while Settings → usageContent (read my prompts) and habitScan are both on:
//   scan     once a week, the prompts you typed in the last 14 days (Claude Code's transcripts under ~/.claude/projects)
//            go to Haiku through your own `claude -p`, which groups them into kinds of request. A kind asked at least
//            once in each of the last two weeks is a habit. Kept: each kind's short label and summary (generic, as
//            Haiku writes them), its counts per week and its session ids. Never the prompts: they stay in memory.
//   draft    on your click, `claude -p` (Sonnet, with Claude's skill-creator when you have it) writes a SKILL.md, with
//            the prompts of the kind's best sessions (fewest turns) as examples, read again from the transcripts.
//   approve  only on your click: the draft (as you edited it) goes to ~/.claude/skills/<name>/SKILL.md, never a
//            project. If a skill like it exists, the suggestion is to improve that one (yours) or use it (others').
//   versions every change Bobix makes to a skill is a numbered copy in <data>/habit-skills/versions/<name>/; Undo
//            writes the one before (a skill Bobix made goes to the skills audit's backup). A hand edit is kept as
//            a version before Bobix writes over it.
//   evidence a skill Bobix made is offered an improvement only on numbers from its uses (tokens, follow-up prompts,
//            prompts asking for a fix, per use), and removal when unused for a month. Nothing changes on its own.
// `claude -p` runs with no session saved, no hooks and no MCP servers, in Bobix's own folder. Claude's settings are
// never written.
use crate::{skill_audit, skills, usage};
use serde_json::{json, Value};
use std::{collections::{BTreeMap, HashMap, HashSet}, fs, io::{BufRead, BufReader, Read, Write}, path::{Path, PathBuf},
    process::{Command, Stdio}, sync::{atomic::{AtomicBool, Ordering}, Mutex}, time::{Duration, Instant, UNIX_EPOCH}};

const DAY_MS: u64 = 86_400_000;
const WEEK_MS: u64 = 7 * DAY_MS;
const MAX_PROMPTS: usize = 300; // newest first, each cut to PROMPT_CHARS: one Haiku call
const PROMPT_CHARS: usize = 400;
const EXAMPLE_CHARS: usize = 600;
const MAX_SKILL: usize = 32 * 1024;
const UNUSED_DAYS: u64 = 30;
// An improvement needs this many uses, and then this many follow-up prompts or fix requests per use.
const MIN_USES: u64 = 3;
const EDITS_PER_USE: f64 = 3.0;
const FIXES_PER_USE: f64 = 1.0;

static BUSY: AtomicBool = AtomicBool::new(false);
static WRITE: Mutex<()> = Mutex::new(());

pub fn on(st: &Value) -> bool { st["usageContent"] == true && st["habitScan"] == true }
fn root(data: &Path) -> PathBuf { data.join("habit-skills") }
fn state_file(data: &Path) -> PathBuf { root(data).join("state.json") }
fn version_file(data: &Path, name: &str, v: u64) -> PathBuf { root(data).join("versions").join(name).join(format!("{v}.md")) }
fn load(data: &Path) -> Value {
    let v: Value = fs::read(state_file(data)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
    if v.is_object() { v } else { json!({}) }
}
fn save(data: &Path, v: &Value) -> Result<(), String> { crate::settings::save(&state_file(data), v).map_err(|e| e.to_string()) }
fn err(e: impl ToString) -> Value { json!({ "ok": false, "error": e.to_string() }) }

// A skill's command name Bobix writes: lowercase letters, digits and "-", at most 40.
pub fn good_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 40 && s.as_bytes()[0].is_ascii_alphanumeric() && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn slug(s: &str) -> String {
    let s: String = s.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    s.split('-').filter(|w| !w.is_empty()).collect::<Vec<_>>().join("-").chars().take(40).collect::<String>().trim_end_matches('-').to_string()
}
// Text from Haiku or Claude shown in the panel: one line, no control characters, at most `max`.
fn clean(s: &str, max: usize) -> String { s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ").chars().take(max).collect() }
// A kind's label or summary as kept: Haiku is told to write them generically; on top of that, any word that looks like
// code, a path, an address, an assignment or a number of 4+ digits is dropped. ponytail: a token filter, not a secret
// scanner; a project's plain name can still pass.
fn scrub(s: &str, max: usize) -> String {
    let codey = |w: &str| { let t = w.trim_matches(|c: char| ",;:()[]\"'".contains(c)).trim_end_matches('.');
        t.contains(['/', '\\', '=', '@', '`', '<', '>', '{', '}', '$']) || t.split('.').filter(|p| !p.is_empty()).count() > 1 || t.chars().filter(char::is_ascii_digit).count() >= 4 };
    clean(&clean(s, 2000).split(' ').filter(|w| !codey(w)).collect::<Vec<_>>().join(" "), max)
}

// ---------- reading the transcripts (in memory only) ----------

pub struct Prompt { pub session: String, pub t: u64, pub text: String }

// What uses of one skill led to: per use, the prompts you typed after it (until the next skill or the session's end),
// how many of them ask for a fix, and the tokens spent. `follow` holds the texts only when a draft asks for them.
#[derive(Default, Debug, PartialEq)]
pub struct Ev { pub uses: u64, pub edits: u64, pub fixes: u64, pub tokens: u64, pub last: u64, pub follow: Vec<String> }

// The skill a line starts: the Skill tool's call, or a typed /command.
fn skill_used(j: &Value) -> Option<String> {
    if j["type"] == "assistant" {
        return j["message"]["content"].as_array()?.iter().find(|c| c["type"] == "tool_use" && c["name"] == "Skill")
            .and_then(|c| c["input"]["skill"].as_str()).map(|s| s.trim_start_matches('/').to_string());
    }
    let text = match &j["message"]["content"] { Value::String(s) => s.clone(), v => v.to_string() };
    text.split_once("<command-name>/").and_then(|(_, r)| r.split_once("</command-name>")).map(|x| x.0.to_string())
}

// One pass over the transcripts modified since `from_ev`: your prompts since `from_prompts`, and the evidence of
// each skill in `track` since `from_ev` and since Bobix last wrote it (its value: a use of an older version doesn't count). Subagents' files are skipped: their prompts are Claude's.
pub fn read(projects: &Path, from_prompts: u64, from_ev: u64, track: &HashMap<String, u64>, texts: bool) -> (Vec<Prompt>, HashMap<String, Ev>) {
    let (mut prompts, mut ev): (Vec<Prompt>, HashMap<String, Ev>) = (vec![], HashMap::new());
    for (p, _) in skill_audit::transcripts(projects, UNIX_EPOCH + Duration::from_millis(from_ev)) {
        if p.parent().and_then(Path::file_name).is_some_and(|n| n == "subagents") { continue; }
        let Ok(f) = fs::File::open(&p) else { continue };
        let session = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let (mut active, mut last_id): (Option<String>, String) = (None, String::new());
        for line in BufReader::new(f).split(b'\n').map_while(Result::ok) {
            let Ok(j) = serde_json::from_slice::<Value>(&line) else { continue };
            let Some(t) = j["timestamp"].as_str().and_then(usage::iso_ms) else { continue };
            if t < from_ev || j["isSidechain"] == true { continue; }
            if let Some(s) = skill_used(&j) {
                active = track.get(&s).is_some_and(|since| t >= *since).then(|| s.clone());
                if let Some(e) = active.as_ref().map(|s| ev.entry(s.clone()).or_default()) { e.uses += 1; e.last = e.last.max(t); }
            }
            if j["type"] == "assistant" {
                let id = j["message"]["id"].as_str().unwrap_or("").to_string();
                if let (Some(s), false) = (&active, !id.is_empty() && id == last_id) {
                    let u = &j["message"]["usage"];
                    let n: u64 = ["input_tokens", "output_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"].iter().filter_map(|k| u[k].as_u64()).sum();
                    ev.entry(s.clone()).or_default().tokens += n;
                }
                last_id = id;
                continue;
            }
            if j["type"] != "user" || j["isMeta"] == true { continue; }
            let Some(text) = usage::prompt_text(&j["message"]["content"]) else { continue };
            if let Some(s) = &active {
                let e = ev.entry(s.clone()).or_default();
                e.edits += 1;
                if usage::task_kind(&text) == "fix" { e.fixes += 1; }
                if texts && e.follow.len() < 10 { e.follow.push(text.chars().take(300).collect()); }
            }
            if t >= from_prompts { prompts.push(Prompt { session: session.clone(), t, text }); }
        }
    }
    prompts.sort_by_key(|p| std::cmp::Reverse(p.t));
    (prompts, ev)
}

// ---------- claude -p ----------

// Runs your own `claude -p` with the prompt on stdin: no session saved, no hooks, no MCP servers, only `tools`.
// Gives up after `secs`.
pub fn claude(bin: &str, cwd: &Path, model: &str, tools: &str, prompt: &str, secs: u64) -> Result<String, String> {
    fs::create_dir_all(cwd).map_err(|e| e.to_string())?;
    let mut child = Command::new(bin)
        .args(["-p", "--model", model, "--output-format", "text", "--no-session-persistence", "--strict-mcp-config",
            "--settings", r#"{"disableAllHooks":true}"#, "--tools", tools])
        .current_dir(cwd).env_remove("CLAUDECODE").env_remove("CLAUDE_CODE_ENTRYPOINT")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("claude: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("claude: no stdin")?;
    let p = prompt.to_string();
    let writer = std::thread::spawn(move || { let _ = stdin.write_all(p.as_bytes()); });
    let (mut out, mut errs) = (child.stdout.take().ok_or("claude: no stdout")?, child.stderr.take().ok_or("claude: no stderr")?);
    let r = std::thread::spawn(move || { let mut s = String::new(); let _ = out.read_to_string(&mut s); s });
    let e = std::thread::spawn(move || { let mut s = String::new(); let _ = errs.read_to_string(&mut s); s });
    let end = Instant::now() + Duration::from_secs(secs);
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? { break s; }
        if Instant::now() > end { let _ = child.kill(); let _ = child.wait(); return Err(format!("claude -p took more than {secs} s")); }
        std::thread::sleep(Duration::from_millis(200));
    };
    let _ = writer.join();
    let (out, errs) = (r.join().unwrap_or_default(), e.join().unwrap_or_default());
    if !status.success() { return Err(clean(&format!("claude -p failed: {} {}", errs.trim(), out.trim()), 300)); }
    Ok(out)
}

pub type Run<'a> = &'a dyn Fn(&str, &str, &str) -> Result<String, String>; // (model, tools, prompt) → answer

// ---------- the weekly scan ----------

pub fn busy() -> bool { BUSY.load(Ordering::SeqCst) }
pub fn due(st: &Value, data: &Path, now: u64) -> bool {
    on(st) && !busy() && now.saturating_sub(load(data)["scan"]["at"].as_u64().unwrap_or(0)) >= WEEK_MS
}

fn group_prompt(prompts: &[Prompt]) -> String {
    let mut s = String::from("You group a developer's requests to Claude Code into kinds of recurring work.\n\
        Below are numbered requests from the last 14 days. Find the kinds of request that come back: two or more requests for the same kind of task \
        (for example \"write release notes for a version\" or \"review a pull request against the checklist\"). Leave out one-off requests, greetings, \
        follow-up corrections and questions about one piece of code.\n\
        Answer with JSON only: {\"kinds\":[{\"label\":\"3 to 8 words, starting with a verb\",\"summary\":\"one sentence: what is asked and what a good result is\",\"items\":[numbers]}]}\n\
        Labels and summaries must be generic: no names, paths, URLs, ids, secrets or code from the requests.\n\nRequests:\n");
    for (i, p) in prompts.iter().enumerate() { s += &format!("[{}] {}\n", i + 1, clean(&p.text, PROMPT_CHARS)); }
    s
}

// Haiku's answer → kinds: (label, summary, the prompts' indexes). Anything else in it is ignored.
pub fn parse_kinds(answer: &str, n: usize) -> Result<Vec<(String, String, Vec<usize>)>, String> {
    let (a, b) = (answer.find('{').ok_or("no JSON in the answer")?, answer.rfind('}').ok_or("no JSON in the answer")?);
    let v: Value = serde_json::from_str(answer.get(a..=b).unwrap_or("")).map_err(|e| format!("not JSON ({e})"))?;
    Ok(v["kinds"].as_array().ok_or("no \"kinds\" list")?.iter().filter_map(|k| {
        let label = scrub(k["label"].as_str()?, 80);
        let items: Vec<usize> = k["items"].as_array()?.iter().filter_map(Value::as_u64).filter(|&i| i >= 1 && (i as usize) <= n).map(|i| i as usize - 1).collect::<HashSet<_>>().into_iter().collect();
        (!label.is_empty() && !slug(&label).is_empty() && items.len() >= 2).then(|| (label, scrub(k["summary"].as_str().unwrap_or(""), 300), items))
    }).collect())
}

// Turns per past session (usage.json), to tell the good sessions of a kind: fewest turns, least back-and-forth.
fn turns(data: &Path) -> HashMap<String, u64> {
    let v: Value = fs::read(data.join("usage.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
    v["sessions"].as_array().into_iter().flatten().filter_map(|s| Some((s["id"].as_str()?.to_string(), s["turns"].as_u64()?))).collect()
}

// The scan: prompts → Haiku → kinds with their weeks; and the evidence of the skills Bobix made. Stores derived facts only.
pub fn scan(home: &Path, data: &Path, now: u64, run: Run) -> Value {
    if BUSY.swap(true, Ordering::SeqCst) { return err("a scan is running"); }
    let r = scan_inner(home, data, now, run);
    BUSY.store(false, Ordering::SeqCst);
    r
}
fn scan_inner(home: &Path, data: &Path, now: u64, run: Run) -> Value {
    let made: HashMap<String, u64> = load(data)["made"].as_object().map(|m| m.iter().map(|(k, v)| (k.clone(), v["since"].as_u64().unwrap_or(0))).collect()).unwrap_or_default();
    let (mut prompts, ev) = read(&home.join(".claude/projects"), now.saturating_sub(2 * WEEK_MS), now.saturating_sub(UNUSED_DAYS * DAY_MS), &made, false);
    prompts.truncate(MAX_PROMPTS);
    let kinds = if prompts.len() < 2 { Ok(vec![]) } else { run("haiku", "", &group_prompt(&prompts)).and_then(|a| parse_kinds(&a, prompts.len())) };
    let turns = turns(data);
    let _w = WRITE.lock().unwrap();
    let mut s = load(data);
    s["scan"] = json!({ "at": now, "prompts": prompts.len(), "error": kinds.as_ref().err() });
    if let Ok(kinds) = kinds {
        s["kinds"] = json!(kinds.iter().map(|(label, summary, items)| {
            let mut weeks = [0u64; 2];
            let mut sessions: Vec<&str> = vec![];
            for &i in items {
                let p = &prompts[i];
                weeks[usize::from(now.saturating_sub(p.t) < WEEK_MS)] += 1;
                if !sessions.contains(&p.session.as_str()) { sessions.push(&p.session); }
            }
            let mut good = sessions.clone();
            good.sort_by_key(|id| turns.get(*id).copied().unwrap_or(u64::MAX));
            good.truncate(3);
            json!({ "id": slug(label), "label": label, "summary": summary, "weeks": weeks, "sessions": sessions.iter().take(20).collect::<Vec<_>>(), "good": good })
        }).collect::<Vec<_>>());
    }
    for name in made.keys() {
        let e = ev.get(name);
        s["made"][name]["evidence"] = json!({ "uses": e.map_or(0, |e| e.uses), "edits": e.map_or(0, |e| e.edits), "fixes": e.map_or(0, |e| e.fixes), "tokens": e.map_or(0, |e| e.tokens), "at": now });
    }
    match save(data, &s) { Ok(()) => json!({ "ok": true }), Err(e) => err(e) }
}

// ---------- what the panel shows ----------

// ponytail: a word-overlap heuristic (the skills audit's words), not semantics; Haiku could judge it if this misfires.
fn similar<'a>(label: &str, summary: &str, list: &'a [skills::Skill]) -> Option<&'a skills::Skill> {
    let a = skill_audit::words(&format!("{label} {summary}"));
    list.iter().filter(|s| !s.hidden).map(|s| {
        let b = skill_audit::words(&format!("{} {}", s.name.replace(['-', ':', '_'], " "), s.description));
        let both = a.intersection(&b).count();
        (s, both, both as f64 / a.len().min(b.len()).max(1) as f64)
    }).filter(|(_, both, r)| *both >= 2 && *r >= 0.5).max_by(|x, y| x.2.total_cmp(&y.2)).map(|x| x.0)
}

// Evidence counted after Bobix last wrote the skill, that passes the thresholds.
fn improve_due(m: &Value) -> bool { m["evidence"]["at"].as_u64().unwrap_or(0) > m["since"].as_u64().unwrap_or(0) && evidence_due(&m["evidence"]) }
fn evidence_due(e: &Value) -> bool {
    let uses = e["uses"].as_u64().unwrap_or(0);
    uses >= MIN_USES && (e["edits"].as_u64().unwrap_or(0) as f64 / uses as f64 >= EDITS_PER_USE || e["fixes"].as_u64().unwrap_or(0) as f64 / uses as f64 >= FIXES_PER_USE)
}

fn personal_dir(home: &Path, name: &str) -> PathBuf { home.join(".claude/skills").join(name) }
fn mine(home: &Path) -> Vec<skills::Skill> { skills::all(home, &home.join(".claude/no-project")) }

// { on, scan: { at, next, prompts, error, busy }, suggestions: [...], made: [...] }
pub fn view(home: &Path, data: &Path, st: &Value, now: u64) -> Value {
    let s = load(data);
    let list = mine(home);
    let used = skill_audit::uses(&home.join(".claude/projects"), now, st["usageContent"] == true);
    let made = s["made"].as_object().cloned().unwrap_or_default();
    let dismissed: Vec<&str> = s["dismissed"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let mut sugg = vec![];
    for k in s["kinds"].as_array().into_iter().flatten() {
        let (id, label, summary) = (k["id"].as_str().unwrap_or(""), k["label"].as_str().unwrap_or(""), k["summary"].as_str().unwrap_or(""));
        let w = |i: usize| k["weeks"][i].as_u64().unwrap_or(0);
        if w(0) == 0 || w(1) == 0 || dismissed.contains(&id) { continue; } // a habit: at least once in each of the two weeks
        let sim = similar(label, summary, &list);
        if sim.is_some_and(|x| made.contains_key(&x.name)) { continue; } // covered by a skill Bobix made: improved on evidence only
        let action = match sim { None => "create", Some(x) if x.source == "personal" && x.dir == personal_dir(home, &x.name) => "improve", Some(_) => "use" };
        sugg.push(json!({ "id": id, "label": label, "summary": summary, "weeks": [w(0), w(1)], "sessions": k["sessions"].as_array().map_or(0, Vec::len),
            "action": action, "similar": sim.map(|x| json!({ "name": x.name, "source": x.source, "description": x.description })), "draft": s["drafts"][id] }));
    }
    let mut rows: Vec<Value> = made.iter().map(|(name, m)| {
        let dir = personal_dir(home, name);
        let on_disk = dir.join("SKILL.md").is_file();
        let (n, last) = used.get(name).copied().unwrap_or((0, 0));
        let latest = m["versions"].as_array().into_iter().flatten().filter_map(|v| v["at"].as_u64()).max().unwrap_or(0);
        let e = &m["evidence"];
        json!({ "name": name, "kind": m["kind"], "created": m["created"], "made": m["made"], "cur": m["cur"], "versions": m["versions"], "onDisk": on_disk,
            "uses": n, "lastUsed": if last > 0 { json!(last) } else { Value::Null }, "evidence": e, "improve": on_disk && improve_due(m), "fresh": e["at"].as_u64().unwrap_or(0) > m["since"].as_u64().unwrap_or(0),
            "remove": on_disk && m["made"] == true && n == 0 && now.saturating_sub(latest) >= UNUSED_DAYS * DAY_MS, "draft": s["drafts"][format!("m:{name}")], "path": dir })
    }).collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r["created"].as_u64().unwrap_or(0)));
    let at = s["scan"]["at"].as_u64();
    json!({ "on": on(st), "content": st["usageContent"] == true, "scan": { "at": at, "next": at.map(|a| a + WEEK_MS), "prompts": s["scan"]["prompts"], "error": s["scan"]["error"], "busy": busy() },
        "suggestions": sugg, "made": rows, "rules": { "minUses": MIN_USES, "editsPerUse": EDITS_PER_USE, "fixesPerUse": FIXES_PER_USE, "unusedDays": UNUSED_DAYS } })
}

// ---------- drafts ----------

// The SKILL.md between <skill> and </skill> (or the whole answer), checked: front matter with a good name and a
// description. → (name, text)
pub fn parse_skill(answer: &str) -> Result<(String, String), String> {
    let body = answer.split_once("<skill>").and_then(|(_, r)| r.split_once("</skill>")).map_or(answer, |x| x.0);
    let text = body.trim().trim_start_matches("```markdown").trim_start_matches("```md").trim_start_matches("```").trim_end_matches("```").trim().to_string() + "\n";
    if text.len() > MAX_SKILL { return Err(format!("longer than {} KB", MAX_SKILL / 1024)); }
    let fm = skills::front_matter(&text);
    let get = |k: &str| fm.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone()).unwrap_or_default();
    let name = get("name");
    if !good_name(&name) { return Err("the front matter needs a name: lowercase letters, digits and -, at most 40".into()); }
    if get("description").trim().is_empty() { return Err("the front matter needs a description".into()); }
    Ok((name, text))
}

fn example_prompts(home: &Path, ids: &[&str], now: u64) -> String {
    let want: HashSet<&str> = ids.iter().copied().collect();
    let (prompts, _) = read(&home.join(".claude/projects"), now.saturating_sub(2 * WEEK_MS), now.saturating_sub(2 * WEEK_MS), &HashMap::new(), false);
    let mut by: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for p in prompts.iter().rev().filter(|p| want.contains(p.session.as_str())) {
        let i = ids.iter().position(|x| *x == p.session).unwrap_or(0);
        let v = by.entry(i).or_default();
        if v.len() < 3 { v.push(p.text.chars().take(EXAMPLE_CHARS).collect()); }
    }
    by.values().enumerate().map(|(i, v)| format!("--- session {} ---\n{}\n", i + 1, v.join("\n\n"))).collect()
}

const RULES: &str = "Use the skill-creator skill's guidance if you have it, but only draft: do not interview, run evals or write any file.\n\
    The SKILL.md needs front matter with name (lowercase letters, digits and -, at most 40) and description (what it does and when to use it, under 300 characters), then short, concrete steps.\n\
    Keep it generic: leave out names, paths, URLs, ids, secrets and code copied from the examples.\n\
    Print only the file, between <skill> and </skill>.";

// Asks Claude for a draft of `id`: a habit's kind id, or "m:<name>" (an improvement of a skill Bobix made, on its
// evidence). Kept until you approve or dismiss it.
pub fn draft(home: &Path, data: &Path, id: &str, now: u64, run: Run) -> Value {
    let s = load(data);
    let (prompt, target) = if let Some(name) = id.strip_prefix("m:") {
        let m = &s["made"][name];
        if !m.is_object() || !good_name(name) { return err("no such skill"); }
        let e = &m["evidence"];
        if !improve_due(m) { return err("no evidence for a change yet"); }
        let Ok(cur) = fs::read_to_string(personal_dir(home, name).join("SKILL.md")) else { return err("the skill is not in ~/.claude/skills any more") };
        let (_, ev) = read(&home.join(".claude/projects"), now, now.saturating_sub(UNUSED_DAYS * DAY_MS), &HashMap::from([(name.to_string(), m["since"].as_u64().unwrap_or(0))]), true);
        let follow = ev.get(name).map(|e| e.follow.iter().map(|f| format!("- {f}")).collect::<Vec<_>>().join("\n")).unwrap_or_default();
        let uses = e["uses"].as_u64().unwrap_or(1).max(1);
        (format!("Improve this Claude Code skill (one SKILL.md) on the evidence of how it was used. Keep its name \"{name}\".\n{RULES}\n\n\
            Evidence, last 30 days: {uses} uses; per use {:.1} follow-up prompts, {:.1} asking for a fix, {} tokens.\n\
            What the person typed after using it (what the skill missed):\n{follow}\n\nThe skill now:\n<current>\n{cur}</current>\n",
            e["edits"].as_u64().unwrap_or(0) as f64 / uses as f64, e["fixes"].as_u64().unwrap_or(0) as f64 / uses as f64, e["tokens"].as_u64().unwrap_or(0) / uses), Some(name.to_string()))
    } else {
        let Some(k) = s["kinds"].as_array().into_iter().flatten().find(|k| k["id"] == id) else { return err("no such suggestion") };
        let (label, summary) = (k["label"].as_str().unwrap_or(""), k["summary"].as_str().unwrap_or(""));
        let good: Vec<&str> = k["good"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let list = mine(home);
        let sim = similar(label, summary, &list).filter(|x| x.source == "personal" && x.dir == personal_dir(home, &x.name));
        let cur = sim.and_then(|x| fs::read_to_string(x.dir.join("SKILL.md")).ok());
        let weeks = k["weeks"][0].as_u64().unwrap_or(0) + k["weeks"][1].as_u64().unwrap_or(0);
        let mut p = format!("Write a Claude Code skill (one SKILL.md) for a request this developer makes every week.\n{RULES}\n\n\
            The kind of request: {label}. {summary}\nAsked {weeks} times in the last two weeks.\n\
            How they asked, in the sessions that went best (fewest turns):\n{}", example_prompts(home, &good, now));
        if let (Some(x), Some(c)) = (sim, &cur) {
            p += &format!("\nA skill like it exists. Improve it instead of adding a new one: keep its name \"{}\" and make the smallest change that covers this kind of request.\n<current>\n{c}</current>\n", x.name);
        }
        (p, sim.map(|x| x.name.clone()))
    };
    let answer = match run("sonnet", "Skill,Read", &prompt) { Ok(a) => a, Err(e) => return err(e) };
    let (mut name, text) = match parse_skill(&answer) { Ok(x) => x, Err(e) => return err(format!("the draft is not a valid SKILL.md: {e}")) };
    if let Some(t) = &target { name.clone_from(t); }
    let _w = WRITE.lock().unwrap();
    let mut s = load(data);
    s["drafts"][id] = json!({ "name": name, "text": text, "target": target, "at": now });
    match save(data, &s) { Ok(()) => json!({ "ok": true }), Err(e) => err(e) }
}

pub fn dismiss(data: &Path, id: &str) -> Value {
    let _w = WRITE.lock().unwrap();
    let mut s = load(data);
    if let Some(d) = s["drafts"].as_object_mut() { d.remove(id); }
    if !id.starts_with("m:") {
        let mut d: Vec<Value> = s["dismissed"].as_array().cloned().unwrap_or_default();
        if !d.contains(&json!(id)) { d.push(json!(id)); }
        s["dismissed"] = json!(d);
    }
    match save(data, &s) { Ok(()) => json!({ "ok": true }), Err(e) => err(e) }
}

// ---------- approve, versions, undo ----------

fn add_version(data: &Path, s: &mut Value, name: &str, text: &str, why: &str, now: u64, evidence: Value) -> std::io::Result<u64> {
    let v = s["made"][name]["versions"].as_array().into_iter().flatten().filter_map(|x| x["v"].as_u64()).max().map_or(if why == "original" { 0 } else { 1 }, |m| m + 1);
    let f = version_file(data, name, v);
    fs::create_dir_all(f.parent().unwrap())?;
    fs::write(&f, text)?;
    let mut list = s["made"][name]["versions"].as_array().cloned().unwrap_or_default();
    list.push(json!({ "v": v, "at": now, "why": why, "evidence": evidence }));
    s["made"][name]["versions"] = json!(list);
    Ok(v)
}

// A hand edit since Bobix's last write is kept as a version first, so nothing is lost.
fn keep_edit(data: &Path, s: &mut Value, name: &str, disk: &str, now: u64) -> std::io::Result<()> {
    let cur = s["made"][name]["cur"].as_u64().and_then(|v| fs::read_to_string(version_file(data, name, v)).ok());
    if cur.as_deref() != Some(disk) { let v = add_version(data, s, name, disk, "edit", now, Value::Null)?; s["made"][name]["cur"] = json!(v); }
    Ok(())
}

// Installs the draft `id` as you approved it (`text`, edited or not). A new skill goes to ~/.claude/skills/<name>;
// an improvement writes the existing one, after keeping it as a version.
pub fn approve(home: &Path, data: &Path, id: &str, text: &str, now: u64) -> Value {
    let _w = WRITE.lock().unwrap();
    let mut s = load(data);
    let d = s["drafts"][id].clone();
    if !d.is_object() { return err("no such draft"); }
    let (name, text) = match parse_skill(text) { Ok(x) => x, Err(e) => return err(e) };
    let target = d["target"].as_str().map(String::from);
    if target.as_ref().is_some_and(|t| *t != name) { return err(format!("keep the name \"{}\": this improves that skill", target.unwrap())); }
    let dir = personal_dir(home, &name);
    let r = (|| -> std::io::Result<u64> {
        if target.is_none() {
            if fs::symlink_metadata(&dir).is_ok() { return Err(std::io::Error::other(format!("a skill named \"{name}\" is already in ~/.claude/skills: rename it in the draft"))); }
            fs::create_dir_all(&dir)?;
            s["made"][&name] = json!({ "made": true, "kind": s["kinds"].as_array().into_iter().flatten().find(|k| k["id"] == id).map(|k| k["label"].clone()), "created": now });
        } else {
            let disk = fs::read_to_string(dir.join("SKILL.md"))?;
            if !s["made"][&name].is_object() {
                s["made"][&name] = json!({ "made": false, "created": now });
                add_version(data, &mut s, &name, &disk, "original", now, Value::Null)?;
                s["made"][&name]["cur"] = json!(0);
            } else { keep_edit(data, &mut s, &name, &disk, now)?; }
        }
        let ev = if id.starts_with("m:") { s["made"][&name]["evidence"].clone() } else { Value::Null };
        let v = add_version(data, &mut s, &name, &text, if target.is_none() { "new" } else { "improve" }, now, ev)?;
        fs::write(dir.join("SKILL.md"), &text)?;
        s["made"][&name]["cur"] = json!(v);
        s["made"][&name]["since"] = json!(now);
        Ok(v)
    })();
    let v = match r { Ok(v) => v, Err(e) => { if target.is_none() && fs::read_dir(&dir).is_ok_and(|mut r| r.next().is_none()) { let _ = fs::remove_dir(&dir); } return err(e); } };
    s["drafts"].as_object_mut().map(|o| o.remove(id));
    match save(data, &s) { Ok(()) => json!({ "ok": true, "name": name, "v": v, "path": dir }), Err(e) => err(e) }
}

// Writes version `v` of a skill Bobix changed back into ~/.claude/skills.
pub fn restore(home: &Path, data: &Path, name: &str, v: u64, now: u64) -> Value {
    if !good_name(name) { return err("no such skill"); }
    let _w = WRITE.lock().unwrap();
    let mut s = load(data);
    if !s["made"][name].is_object() { return err("no such skill"); }
    let Ok(text) = fs::read_to_string(version_file(data, name, v)) else { return err("no such version") };
    let dir = personal_dir(home, name);
    let Ok(disk) = fs::read_to_string(dir.join("SKILL.md")) else { return err("the skill is not in ~/.claude/skills any more (see Backups)") };
    let r = keep_edit(data, &mut s, name, &disk, now).and_then(|_| fs::write(dir.join("SKILL.md"), &text));
    if let Err(e) = r { return err(e); }
    s["made"][name]["cur"] = json!(v);
    s["made"][name]["since"] = json!(now);
    match save(data, &s) { Ok(()) => json!({ "ok": true, "v": v }), Err(e) => err(e) }
}

// Undo: the version before the current one; for a skill Bobix made at its first version, its removal through the
// skills audit (a backup it can undo). → { ok, v } or { ok, removed: backup id }
pub fn undo(home: &Path, data: &Path, name: &str, now: u64) -> Value {
    let s = load(data);
    let cur = s["made"][name]["cur"].as_u64().unwrap_or(0);
    let prev = s["made"][name]["versions"].as_array().into_iter().flatten().filter_map(|x| x["v"].as_u64()).filter(|v| *v < cur).max();
    match prev { Some(v) => restore(home, data, name, v, now), None => remove(home, data, name) }
}

// Removes a skill Bobix made (unused for a month, or undone at its first version): through the skills audit, which
// keeps a backup with its own Undo.
pub fn remove(home: &Path, data: &Path, name: &str) -> Value {
    if !good_name(name) || load(data)["made"][name]["made"] != true { return err("only a skill Bobix made"); }
    let r = skill_audit::apply(home, None, data, &personal_dir(home, name).to_string_lossy(), "remove");
    if r["ok"] == true { json!({ "ok": true, "removed": r["id"] }) } else { r }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, text: &str) { fs::create_dir_all(p.parent().unwrap()).unwrap(); fs::write(p, text).unwrap(); }
    fn scratch(n: &str) -> PathBuf { let t = std::env::temp_dir().join(format!("bobix-hs-{n}-{}", std::process::id())); let _ = fs::remove_dir_all(&t); t }
    fn ts(ms: u64) -> String {
        let d = crate::habits::day_of(ms);
        let s = (ms % DAY_MS) / 1000;
        format!("{d}T{:02}:{:02}:{:02}.000Z", s / 3600, s / 60 % 60, s % 60)
    }
    fn user(t: u64, text: &str) -> String { json!({ "type": "user", "timestamp": ts(t), "message": { "content": text } }).to_string() + "\n" }
    fn skill_call(t: u64, name: &str) -> String { json!({ "type": "assistant", "timestamp": ts(t), "message": { "id": format!("m{t}"), "content": [{ "type": "tool_use", "name": "Skill", "input": { "skill": name } }], "usage": { "input_tokens": 100, "output_tokens": 50 } } }).to_string() + "\n" }
    fn st() -> Value { json!({ "usageContent": true, "habitScan": true }) }
    const NOW: u64 = 1_762_776_000_000; // 2025-11-10 12:00 UTC

    // Haiku, made up: the release-notes requests are one kind; one item out of range is ignored.
    fn fake(model: &str, _tools: &str, prompt: &str) -> Result<String, String> {
        if model == "haiku" {
            let items: Vec<usize> = prompt.lines().filter(|l| l.contains("release notes")).filter_map(|l| l.strip_prefix('[')?.split(']').next()?.parse().ok()).collect();
            return Ok(format!("Sure:\n{{\"kinds\":[{{\"label\":\"Write release notes for a version\",\"summary\":\"Notes from the merged changes.\",\"items\":{:?}}},{{\"label\":\"x\",\"items\":[999]}}]}}", items));
        }
        assert!(prompt.contains("--- session 1 ---\nwrite the release notes for v1.2"), "the session with the fewest turns comes first: {prompt}");
        Ok("<skill>\n---\nname: release-notes\ndescription: Write release notes for a version from merged changes.\n---\n1. List the merged PRs.\n</skill>".into())
    }

    // One scan at a time (BUSY is the app's), so these tests take turns.
    static ONE: Mutex<()> = Mutex::new(());
    fn world(n: &str) -> (PathBuf, PathBuf, PathBuf, std::sync::MutexGuard<'static, ()>) {
        let g = ONE.lock().unwrap_or_else(|e| e.into_inner());
        let t = scratch(n);
        let (home, data) = (t.join("home"), t.join("data"));
        let p = home.join(".claude/projects/-src-app");
        // two weeks running; the long session is not an example
        write(&p.join("s1.jsonl"), &(user(NOW - 10 * DAY_MS, "write the release notes for v1.1, TOKEN=abc") + &user(NOW - 10 * DAY_MS + 1000, "no, shorter")));
        write(&p.join("s2.jsonl"), &user(NOW - 2 * DAY_MS, "write the release notes for v1.2"));
        write(&p.join("s3.jsonl"), &user(NOW - 3 * DAY_MS, "what does this regex do?"));
        write(&p.join("s1/subagents/a.jsonl"), &user(NOW - DAY_MS, "release notes from a subagent"));
        write(&data.join("usage.json"), &json!({ "sessions": [{ "id": "s1", "turns": 40 }, { "id": "s2", "turns": 6 }] }).to_string());
        (t, home, data, g)
    }

    #[test]
    fn scan_keeps_derived_facts_and_suggests_after_two_weeks() {
        let (t, home, data, _g) = world("scan");
        assert!(due(&st(), &data, NOW) && !due(&json!({ "usageContent": true }), &data, NOW) && !due(&json!({ "habitScan": true }), &data, NOW), "both switches");
        assert_eq!(scan(&home, &data, NOW, &fake)["ok"], true);
        assert!(!due(&st(), &data, NOW + DAY_MS) && due(&st(), &data, NOW + WEEK_MS), "once a week");
        let stored = fs::read_to_string(state_file(&data)).unwrap();
        for never in ["v1.1", "TOKEN", "regex", "shorter", "subagent"] { assert!(!stored.contains(never), "{never} stored"); }
        let v = view(&home, &data, &st(), NOW);
        let s = &v["suggestions"][0];
        assert_eq!((s["id"].as_str(), s["weeks"].clone(), s["action"].as_str()), (Some("write-release-notes-for-a-version"), json!([1, 1]), Some("create")));
        assert_eq!(v["suggestions"].as_array().unwrap().len(), 1);
        // only the last week: not a habit yet
        fs::remove_file(home.join(".claude/projects/-src-app/s1.jsonl")).unwrap();
        scan(&home, &data, NOW, &fake);
        assert_eq!(view(&home, &data, &st(), NOW)["suggestions"], json!([]));
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn draft_approve_versions_and_undo() {
        let (t, home, data, _g) = world("flow");
        scan(&home, &data, NOW, &fake);
        let id = "write-release-notes-for-a-version";
        assert_eq!(draft(&home, &data, id, NOW, &fake)["ok"], true);
        let d = view(&home, &data, &st(), NOW)["suggestions"][0]["draft"].clone();
        assert_eq!(d["name"], "release-notes");
        assert!(!personal_dir(&home, "release-notes").exists(), "nothing installed before approval");
        let edited = d["text"].as_str().unwrap().replace("1. List", "1. First list");
        let r = approve(&home, &data, id, &edited, NOW);
        assert_eq!((r["ok"].clone(), r["v"].clone()), (json!(true), json!(1)));
        let file = personal_dir(&home, "release-notes").join("SKILL.md");
        assert_eq!(fs::read_to_string(&file).unwrap(), edited, "your edits are what gets installed");
        let v = view(&home, &data, &st(), NOW);
        assert_eq!(v["suggestions"], json!([]), "covered by the skill Bobix made");
        assert_eq!((v["made"][0]["name"].as_str(), v["made"][0]["improve"].clone(), v["made"][0]["remove"].clone()), (Some("release-notes"), json!(false), json!(false)));
        assert_eq!(draft(&home, &data, "m:release-notes", NOW, &fake)["ok"], false, "no change without evidence");
        // a hand edit, then a restore: the edit is kept as version 2
        fs::write(&file, "---\nname: release-notes\ndescription: mine\n---\n").unwrap();
        assert_eq!(restore(&home, &data, "release-notes", 1, NOW)["ok"], true);
        assert_eq!(fs::read_to_string(version_file(&data, "release-notes", 2)).unwrap(), "---\nname: release-notes\ndescription: mine\n---\n");
        assert_eq!(fs::read_to_string(&file).unwrap(), edited);
        // undo at the first version: removed through the audit, with a backup it can undo
        let r = undo(&home, &data, "release-notes", NOW);
        assert!(r["ok"] == true && r["removed"].is_string(), "{r}");
        assert!(!file.exists());
        assert_eq!(skill_audit::undo(&data, r["removed"].as_str().unwrap())["ok"], true);
        assert_eq!(fs::read_to_string(&file).unwrap(), edited);
        // a month without a use: suggested for removal, never removed on its own
        assert_eq!(view(&home, &data, &st(), NOW + 31 * DAY_MS)["made"][0]["remove"], true);
        assert!(file.exists());
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn a_similar_skill_is_improved_instead() {
        let (t, home, data, _g) = world("similar");
        let mine = personal_dir(&home, "notes");
        write(&mine.join("SKILL.md"), "---\nname: notes\ndescription: Write release notes for a version.\n---\nold\n");
        scan(&home, &data, NOW, &fake);
        let id = "write-release-notes-for-a-version";
        let s = view(&home, &data, &st(), NOW)["suggestions"][0].clone();
        assert_eq!((s["action"].as_str(), s["similar"]["name"].as_str()), (Some("improve"), Some("notes")));
        assert_eq!(draft(&home, &data, id, NOW, &fake)["ok"], true);
        let d = view(&home, &data, &st(), NOW)["suggestions"][0]["draft"].clone();
        assert_eq!(d["name"], "notes", "keeps the existing name");
        assert_eq!(approve(&home, &data, id, d["text"].as_str().unwrap(), NOW)["ok"], false, "the draft's own name is refused");
        let text = d["text"].as_str().unwrap().replace("name: release-notes", "name: notes");
        assert_eq!(approve(&home, &data, id, &text, NOW)["v"], 1);
        assert_eq!(undo(&home, &data, "notes", NOW)["v"], 0, "undo goes back to yours");
        assert_eq!(fs::read_to_string(mine.join("SKILL.md")).unwrap(), "---\nname: notes\ndescription: Write release notes for a version.\n---\nold\n");
        assert_eq!(remove(&home, &data, "notes")["ok"], false, "not Bobix's to remove");
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn evidence_counts_follow_ups_per_use() {
        let t = scratch("ev");
        let p = t.join("projects/-a");
        let mut text = String::new();
        text += &(skill_call(NOW - 5 * DAY_MS, "release-notes") + &user(NOW - 5 * DAY_MS + 1, "a use of the version before"));
        for i in 0..3 { let b = NOW - (i + 1) * DAY_MS; text += &(skill_call(b, "release-notes") + &user(b + 1, "fix the date format") + &user(b + 2, "shorter") + &user(b + 3, "add links")); }
        text += &(skill_call(NOW - 1000, "other") + &user(NOW - 500, "unrelated"));
        write(&p.join("s.jsonl"), &text);
        let (prompts, ev) = read(&t.join("projects"), NOW - 2 * WEEK_MS, NOW - 30 * DAY_MS, &HashMap::from([("release-notes".to_string(), NOW - 3 * DAY_MS - 1)]), true);
        let e = &ev["release-notes"];
        assert_eq!((e.uses, e.edits, e.fixes, e.tokens, e.follow.len()), (3, 9, 3, 450, 9));
        assert_eq!(prompts.len(), 11);
        assert!(evidence_due(&json!({ "uses": e.uses, "edits": e.edits, "fixes": e.fixes })));
        assert!(!evidence_due(&json!({ "uses": 2, "edits": 9 })), "too few uses");
        assert!(!ev.contains_key("other"), "only skills Bobix made");
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn answers_are_checked() {
        assert!(parse_kinds("nope", 3).is_err());
        let mut k = parse_kinds(r#"{"kinds":[{"label":"Do\nX","items":[1,1,2]},{"label":"Y","items":[1]}]}"#, 3).unwrap();
        k[0].2.sort();
        assert_eq!(k, [("Do X".to_string(), String::new(), vec![0, 1])], "one line; duplicates and one-item kinds dropped");
        assert!(parse_skill("---\nname: Bad Name\ndescription: x\n---\n").is_err());
        assert!(parse_skill("---\nname: ok\n---\n").is_err());
        assert_eq!(parse_skill("<skill>```markdown\n---\nname: ok-1\ndescription: d\n---\nbody\n```</skill>").unwrap().0, "ok-1");
        assert_eq!(scrub("Check console.log, run `npm test`, see /src/app.js, TOKEN=abc, v2.3 and 2026 tickets. Done.", 300), "Check run see and tickets. Done.");
        assert!(good_name("a-1") && !good_name("../x") && !good_name("A") && !good_name(""));
    }

    // The whole flow with your own `claude -p` (real Haiku and Sonnet calls) on a scratch HOME of made-up sessions:
    // scan → suggestion → draft → approve → uses → scan → improve → undo → undo. Each step's view is written to
    // $BOBIX_HS_OUT/<n>-<step>.json. BOBIX_HS_OUT=/tmp/hs cargo test live_flow -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_flow() {
        let out = PathBuf::from(std::env::var("BOBIX_HS_OUT").expect("BOBIX_HS_OUT"));
        let _ = fs::remove_dir_all(&out);
        let (home, data) = (out.join("home"), out.join("data"));
        let bin = crate::find_bin("claude").expect("claude on PATH");
        let run = |m: &str, t: &str, p: &str| { println!("claude -p --model {m} ({} chars)", p.len()); claude(&bin, &data.join("habit-skills/work"), m, t, p, 600) };
        let now = skill_audit::now_ms();
        let p = home.join(".claude/projects/-Users-demo-src-shop");
        let day = |d: u64, h: u64| now - d * DAY_MS + h * 3_600_000;
        let sessions: &[(&str, u64, &[&str])] = &[
            ("a1", 12, &["Write the release notes for shop v2.3 from the merged PRs since v2.2. Group them as Added / Fixed / Changed, one line each, link each PR.", "Shorter lines please"]),
            ("a2", 9, &["release notes for v2.4 please, same format as last time: Added / Fixed / Changed with PR links"]),
            ("a3", 4, &["Draft the release notes for shop v2.5 from what was merged this week (Added/Fixed/Changed, PR links)"]),
            ("a4", 1, &["Can you write release notes for v2.6? Merged PRs since the v2.5 tag, grouped, with links."]),
            ("b1", 10, &["Summarize what I did yesterday in this repo for the standup: commits, PRs opened, reviews."]),
            ("b2", 3, &["Standup summary of yesterday's work in this repo please: commits, PRs, reviews"]),
            ("c1", 2, &["Review PR 412 against our checklist: tests, naming, no console.log, migrations reversible"]),
            ("c2", 1, &["Review PR 418 with the checklist (tests, naming, no console.log, reversible migrations)"]),
            ("d1", 6, &["Why does the cart total round down on the checkout page?"]),
            ("d2", 5, &["Rename the OrderLine component to CartLine everywhere"]),
        ];
        for (id, d, ps) in sessions {
            let text: String = ps.iter().enumerate().map(|(i, t)| user(day(*d, 2 + i as u64), t)).collect();
            write(&p.join(format!("{id}.jsonl")), &text);
        }
        write(&data.join("usage.json"), &json!({ "sessions": [{ "id": "a1", "turns": 30 }, { "id": "a2", "turns": 8 }, { "id": "a3", "turns": 6 }, { "id": "a4", "turns": 12 }] }).to_string());
        write(&personal_dir(&home, "standup-notes").join("SKILL.md"), "---\nname: standup-notes\ndescription: Summarize yesterday's work in this repo for the standup: commits, PRs and reviews.\n---\nList yesterday's commits.\n");
        let st = json!({ "usageContent": true, "habitScan": true });
        let mut n = 0;
        let mut dump = |step: &str, v: &Value| { n += 1; fs::create_dir_all(&out).unwrap(); fs::write(out.join(format!("{n:02}-{step}.json")), serde_json::to_string_pretty(v).unwrap()).unwrap(); println!("{n:02} {step}"); };
        dump("off", &view(&home, &data, &json!({ "usageContent": true }), now));
        let r = scan(&home, &data, now, &run);
        assert_eq!(r["ok"], true, "{r}");
        let v = view(&home, &data, &st, now);
        dump("scanned", &v);
        println!("{}", serde_json::to_string_pretty(&v["suggestions"]).unwrap());
        let stored = fs::read_to_string(state_file(&data)).unwrap();
        for never in ["v2.3", "PR 412", "OrderLine", "console.log"] { assert!(!stored.contains(never), "{never} stored"); }
        let k = v["suggestions"].as_array().unwrap().iter().find(|s| s["action"] == "create").expect("a new-skill suggestion")["id"].as_str().unwrap().to_string();
        assert!(v["suggestions"].as_array().unwrap().iter().all(|s| !s["label"].as_str().unwrap().to_lowercase().contains("review")), "one week only: not a habit");
        let r = draft(&home, &data, &k, now, &run);
        assert_eq!(r["ok"], true, "{r}");
        let v = view(&home, &data, &st, now);
        dump("drafted", &v);
        let d = v["suggestions"].as_array().unwrap().iter().find(|s| s["id"] == k.as_str()).unwrap()["draft"].clone();
        println!("{}", d["text"].as_str().unwrap());
        let name = d["name"].as_str().unwrap().to_string();
        let r = approve(&home, &data, &k, d["text"].as_str().unwrap(), now - DAY_MS); // approved yesterday
        assert_eq!(r["ok"], true, "{r}");
        assert!(personal_dir(&home, &name).join("SKILL.md").is_file());
        dump("approved", &view(&home, &data, &st, now));
        // three uses that needed fixing, then the next weekly scan counts them
        let mut text = String::new();
        for i in 0..3 { let b = now - DAY_MS + (i + 1) * 3_600_000; text += &(skill_call(b, &name) + &user(b + 60_000, "fix it: the PR links are missing") + &user(b + 120_000, "and group the dependency bumps under Changed") + &user(b + 180_000, "shorter")); }
        write(&p.join("e1.jsonl"), &text);
        let later = now + WEEK_MS;
        assert_eq!(scan(&home, &data, later, &run)["ok"], true);
        let v = view(&home, &data, &st, later);
        dump("evidence", &v);
        assert_eq!(v["made"][0]["improve"], true, "{}", v["made"][0]);
        let r = draft(&home, &data, &format!("m:{name}"), later, &run);
        assert_eq!(r["ok"], true, "{r}");
        let v = view(&home, &data, &st, later);
        dump("improve-drafted", &v);
        let t = v["made"][0]["draft"]["text"].as_str().unwrap().to_string();
        println!("{t}");
        assert_eq!(approve(&home, &data, &format!("m:{name}"), &t, later)["v"], 2);
        let v = view(&home, &data, &st, later);
        assert_eq!(v["made"][0]["improve"], false, "the evidence was about version 1");
        dump("improved", &v);
        assert_eq!(undo(&home, &data, &name, later)["v"], 1);
        dump("undone", &view(&home, &data, &st, later));
        assert!(undo(&home, &data, &name, later)["removed"].is_string());
        assert!(!personal_dir(&home, &name).exists());
        dump("removed", &view(&home, &data, &st, later));
        println!("skills folder: {:?}", fs::read_dir(home.join(".claude/skills")).unwrap().flatten().map(|e| e.file_name()).collect::<Vec<_>>());
    }

}
