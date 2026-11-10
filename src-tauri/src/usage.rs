// Usage per session, from the transcripts Claude Code writes itself (`transcript_path` in its hook events, and the
// session's `subagents/` folder next to it). Only the usage fields, the model and tool names are read; prompt text only
// with Settings → usageContent on, and then kept as a task kind and counts, never the text. Each file is read
// incrementally from where the last read stopped. Nothing is written to Claude's files, no credential is read and no
// account or quota API is called. The user's normal is the median of past sessions (usage.json: numbers only).
use serde_json::{json, Map, Value};
use std::{collections::{BTreeMap, BTreeSet, HashMap, VecDeque}, fs, io::{BufRead, BufReader, Seek, SeekFrom}, path::{Path, PathBuf}};

// A gap longer than this between two lines is a break, not work: active time counts at most this much of it.
const IDLE_MIN: u64 = 5;
// The "light work" warning looks at this many latest turns of the session itself.
const RECENT: usize = 10;
// The normal needs this many past sessions of at least MIN_TURNS turns: a session of a few turns is a quick question,
// not a usual working session (on the owner's Mac, sessions under 10 turns cost about $0.01 to $1 and pulled the
// median far below any real working session).
const MIN_PAST: usize = 5;
const MIN_TURNS: u64 = 10;
const PAST_KEEP: usize = 200;
// Past transcripts read once to learn the normal: modified in the last 30 days, quiet for 30 min, the newest 40.
const BACKFILL_DAYS: u64 = 30;
const BACKFILL_MAX: usize = 40;
const BACKFILL_PER_TICK: usize = 4;

// ---------- tokens and prices ----------
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Tok { pub input: u64, pub output: u64, pub cw5: u64, pub cw1h: u64, pub cr: u64, pub turns: u64 }

impl Tok {
    fn of(u: &Value) -> Tok {
        let n = |v: &Value| v.as_u64().unwrap_or(0);
        let cw = n(&u["cache_creation_input_tokens"]);
        let h = n(&u["cache_creation"]["ephemeral_1h_input_tokens"]).min(cw);
        Tok { input: n(&u["input_tokens"]), output: n(&u["output_tokens"]), cw5: cw - h, cw1h: h, cr: n(&u["cache_read_input_tokens"]), turns: 1 }
    }
    fn add(&mut self, o: &Tok) {
        self.input += o.input; self.output += o.output; self.cw5 += o.cw5; self.cw1h += o.cw1h; self.cr += o.cr; self.turns += o.turns;
    }
    fn sub(&mut self, o: &Tok) {
        let s = |a: &mut u64, b: u64| *a = a.saturating_sub(b);
        s(&mut self.input, o.input); s(&mut self.output, o.output); s(&mut self.cw5, o.cw5); s(&mut self.cw1h, o.cw1h); s(&mut self.cr, o.cr); s(&mut self.turns, o.turns);
    }
    // What the model read this turn: the whole context, most of it from the cache.
    fn context(&self) -> u64 { self.input + self.cw5 + self.cw1h + self.cr }
    fn arr(&self) -> [u64; 6] { [self.input, self.output, self.cw5, self.cw1h, self.cr, self.turns] }
    fn from_arr(a: &[u64]) -> Tok { let g = |i: usize| a.get(i).copied().unwrap_or(0); Tok { input: g(0), output: g(1), cw5: g(2), cw1h: g(3), cr: g(4), turns: g(5) } }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Price { pub key: String, pub input: f64, pub output: f64, pub cw5: f64, pub cw1h: f64, pub cr: f64, pub heavy: bool }

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prices { pub date: String, pub list: Vec<Price>, pub error: Option<String> }

// The table a new install starts with, written to the data folder for the person to edit. Not used past that.
pub const SEED: &str = include_str!("../prices.json");

pub fn parse_prices(text: &str) -> Result<Prices, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let num = |m: &Value, k: &str| m[k].as_f64().filter(|x| x.is_finite() && *x >= 0.0).ok_or_else(|| format!("'{k}' must be a number ≥ 0"));
    let list = v["models"].as_array().ok_or("no 'models' list")?.iter().map(|m| Ok(Price {
        key: m["match"].as_str().filter(|s| !s.is_empty()).ok_or("each model needs 'match'")?.to_lowercase(),
        input: num(m, "input")?, output: num(m, "output")?, cw5: num(m, "cacheWrite5m")?, cw1h: num(m, "cacheWrite1h")?, cr: num(m, "cacheRead")?,
        heavy: m["heavy"] == true,
    })).collect::<Result<Vec<_>, String>>()?;
    Ok(Prices { date: v["date"].as_str().unwrap_or("").chars().take(40).collect(), list, error: None })
}

impl Prices {
    pub fn of(&self, model: &str) -> Option<&Price> { let m = model.to_lowercase(); self.list.iter().find(|p| m.contains(&p.key)) }
    fn heavy(&self, model: &str) -> bool { self.of(model).is_some_and(|p| p.heavy) }
    // (all, cache reads) in USD, or None for a model the table doesn't price
    fn cost(&self, model: &str, t: &Tok) -> Option<(f64, f64)> {
        let p = self.of(model)?;
        let cr = t.cr as f64 * p.cr / 1e6;
        Some(((t.input as f64 * p.input + t.output as f64 * p.output + t.cw5 as f64 * p.cw5 + t.cw1h as f64 * p.cw1h) / 1e6 + cr, cr))
    }
}

// The person's table: written from SEED when missing. A broken file falls back to SEED and says so.
pub fn load_prices(file: &Path) -> Prices {
    if !file.exists() {
        let _ = file.parent().map(fs::create_dir_all);
        let _ = fs::write(file, SEED);
    }
    let seed = || parse_prices(SEED).expect("prices.json in the source is valid");
    match fs::read_to_string(file).map_err(|e| e.to_string()).and_then(|t| parse_prices(&t)) {
        Ok(p) => p,
        Err(e) => Prices { error: Some(e), ..seed() },
    }
}

// ---------- reading a transcript ----------
#[derive(Default, Debug)]
struct File { offset: u64, last_id: String, last: Option<((String, bool), Tok)> }

#[derive(Default, Debug)]
struct Turn { model: String, out: u64, wrote: bool }

#[derive(Default, Debug)]
pub struct Tally {
    file: String,
    main: File,
    subs: BTreeMap<PathBuf, File>,
    models: BTreeMap<(String, bool), Tok>, // (model, in a subagent)
    minutes: BTreeSet<u64>,                 // minutes with a line, main and subagents: active time
    ctx: u64,                               // the latest turn's context
    recent: VecDeque<Turn>,
    run: u32, run_model: String,            // read-only tool calls in a row, and the model making them
    tasks: BTreeMap<String, u32>,           // opt-in: prompts by kind
    last_task: Option<String>,
}

// Tools that only read. Bash counts only with the content opt-in (its command is content).
const READ_ONLY: &[&str] = &["Read", "Grep", "Glob", "LS", "NotebookRead", "WebFetch", "WebSearch"];
const WRITES: &[&str] = &["Edit", "Write", "MultiEdit", "NotebookEdit"];
const NEUTRAL: &[&str] = &["TodoWrite", "ToolSearch"]; // bookkeeping: neither ends nor extends a read-only run

// A shell command that only reads: every part of `a && b | c ; d` starts with one of these (cd aside).
pub fn read_only_command(cmd: &str) -> bool {
    const RO: &[&str] = &["cat", "head", "tail", "grep", "rg", "ls", "find", "wc", "tree", "file", "stat", "du", "less", "jq", "awk", "pwd", "echo", "which"];
    let parts: Vec<&str> = cmd.split(['|', ';', '\n']).flat_map(|p| p.split("&&")).map(str::trim).filter(|p| !p.is_empty()).collect();
    !parts.is_empty() && parts.iter().all(|p| {
        let w: Vec<&str> = p.split_whitespace().collect();
        match w.first().copied() {
            Some("cd") => true,
            Some("sed") => w.contains(&"-n") && !w.iter().any(|x| x.starts_with("-i")),
            Some("git") => w.get(1).is_some_and(|s| ["status", "log", "diff", "show", "branch", "blame", "rev-parse", "ls-files"].contains(s)),
            Some(x) => RO.contains(&x) && !p.contains('>'),
            None => false,
        }
    })
}

// A prompt's kind, from its words (EN and VI): kept instead of the text.
pub fn task_kind(text: &str) -> &'static str {
    let t = text.trim().to_lowercase();
    let has = |ws: &[&str]| ws.iter().any(|w| t.contains(w));
    let first = t.split_whitespace().next().unwrap_or("");
    if (t.ends_with('?') && t.chars().count() < 300) || ["what", "why", "how", "where", "which", "who", "when", "is", "are", "can", "does", "do", "explain"].contains(&first)
        || has(&["giải thích", "tại sao", "là gì", "thế nào", "ở đâu"]) { return "question"; }
    if has(&["fix", "bug", "error", "fail", "broken", "crash", "sửa", "lỗi"]) { return "fix"; }
    if has(&["review", "audit", "check", "kiểm tra"]) { return "review"; }
    if has(&["readme", "docs", "document", "changelog", "tài liệu"]) { return "docs"; }
    if has(&["add", "implement", "create", "build", "refactor", "feature", "thêm", "tạo", "làm"]) { return "build"; }
    "other"
}

// "2026-11-10T15:05:53.123Z" → ms since 1970 (UTC).
pub fn iso_ms(s: &str) -> Option<u64> {
    let n = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let (y, m, d, h, mi, se) = (n(0, 4)?, n(5, 7)?, n(8, 10)?, n(11, 13)?, n(14, 16)?, n(17, 19)?);
    let frac = s.get(19..).filter(|f| f.starts_with('.')).map(|f| f[1..].chars().take_while(char::is_ascii_digit).take(3).collect::<String>()).unwrap_or_default();
    let ms = if frac.is_empty() { 0 } else { frac.parse::<i64>().ok()? * 10i64.pow(3 - frac.len() as u32) };
    let (yy, mm) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * mm + 2) / 5 + d - 1;
    let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468;
    u64::try_from(((days * 24 + h) * 60 + mi) * 60_000 + se * 1000 + ms).ok()
}

enum Read { Same, Grew, Shrank }

// Reads the complete lines added since `f.offset`; a half-written last line waits for the next read.
fn read_new(path: &Path, f: &mut File, mut on: impl FnMut(&Value, &mut File)) -> Read {
    let Ok(meta) = fs::metadata(path) else { return Read::Same };
    if !meta.is_file() { return Read::Same; } // never open a FIFO / device: that would block
    if meta.len() < f.offset { return Read::Shrank; }
    if meta.len() == f.offset { return Read::Same; }
    let Ok(mut file) = fs::File::open(path) else { return Read::Same };
    if file.seek(SeekFrom::Start(f.offset)).is_err() { return Read::Same; }
    let mut r = BufReader::with_capacity(1 << 20, file);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match r.read_until(b'\n', &mut buf) { Ok(n) if n > 0 && buf.ends_with(b"\n") => f.offset += n as u64, _ => break }
        if let Ok(j) = serde_json::from_slice::<Value>(&buf) { on(&j, f); }
    }
    Read::Grew
}

impl Tally {
    fn line(&mut self, j: &Value, f: &mut File, side: bool, content: bool) {
        let kind = j["type"].as_str().unwrap_or("");
        if kind != "assistant" && kind != "user" { return; }
        if let Some(t) = j["timestamp"].as_str().and_then(iso_ms) { self.minutes.insert(t / 60_000); }
        if kind == "user" {
            if content && !side && j["isMeta"] != true { if let Some(k) = prompt_kind(&j["message"]["content"]) { *self.tasks.entry(k.into()).or_default() += 1; self.last_task = Some(k.into()); } }
            return;
        }
        let m = &j["message"];
        let model = m["model"].as_str().unwrap_or("");
        if model.is_empty() || model.starts_with('<') { return; } // "<synthetic>": Claude Code's own line, no model ran
        let Some(u) = m.get("usage").filter(|u| u.is_object()) else { return };
        let t = Tok::of(u);
        let id = m["id"].as_str().or(j["requestId"].as_str()).unwrap_or("").to_string();
        let key = (model.to_string(), side);
        // One answer is written as one line per content block, each with the same id and usage: replace, don't add.
        let same = !id.is_empty() && id == f.last_id;
        if same { if let Some((k, old)) = &f.last { if let Some(x) = self.models.get_mut(k) { x.sub(old); } } }
        self.models.entry(key.clone()).or_default().add(&t);
        f.last_id = id;
        f.last = Some((key, t));
        if side { return; }
        self.ctx = t.context();
        if !same {
            self.recent.push_back(Turn { model: model.into(), ..Default::default() });
            if self.recent.len() > RECENT { self.recent.pop_front(); }
        }
        let Some(turn) = self.recent.back_mut() else { return };
        turn.out = t.output;
        for c in m["content"].as_array().into_iter().flatten().filter(|c| c["type"] == "tool_use") {
            let name = c["name"].as_str().unwrap_or("");
            let ro = READ_ONLY.contains(&name) || (content && name == "Bash" && c["input"]["command"].as_str().is_some_and(read_only_command));
            if ro { self.run += 1; self.run_model = model.into(); }
            else if !NEUTRAL.contains(&name) { self.run = 0; }
            if WRITES.contains(&name) { turn.wrote = true; }
        }
    }

    // Reads what was added to the transcript and its subagents' files. Returns false when nothing changed.
    fn update(&mut self, file: &str, content: bool) -> bool {
        if self.file != file { *self = Tally { file: file.into(), ..Default::default() }; }
        let path = PathBuf::from(file);
        let mut main = std::mem::take(&mut self.main);
        let r = read_new(&path, &mut main, |j, f| self.line(j, f, false, content));
        self.main = main;
        let mut changed = match r {
            Read::Shrank => { *self = Tally { file: file.into(), ..Default::default() }; return self.update(file, content); } // rewritten: start over
            Read::Grew => true,
            Read::Same => false,
        };
        for sub in subagent_files(&path) {
            let mut f = self.subs.remove(&sub).unwrap_or_default();
            if matches!(read_new(&sub, &mut f, |j, f| self.line(j, f, true, content)), Read::Grew) { changed = true; }
            self.subs.insert(sub, f);
        }
        changed
    }

    fn total(&self) -> Tok { let mut t = Tok::default(); for x in self.models.values() { t.add(x); } t }
    fn main_turns(&self) -> u64 { self.models.iter().filter(|((_, side), _)| !side).map(|(_, t)| t.turns).sum() }

    fn active_ms(&self) -> u64 {
        if self.minutes.is_empty() { return 0; }
        let v: Vec<u64> = self.minutes.iter().copied().collect();
        (1 + v.windows(2).map(|w| (w[1] - w[0]).min(IDLE_MIN)).sum::<u64>()) * 60_000
    }

    fn past(&self, id: &str, end: u64) -> Past {
        let mut models: BTreeMap<String, Tok> = BTreeMap::new();
        for ((m, _), t) in &self.models { models.entry(m.clone()).or_default().add(t); }
        Past { id: id.into(), end, active_ms: self.active_ms(), turns: self.main_turns(), models, tasks: self.tasks.clone() }
    }
}

// A prompt's kind, or None for what isn't one (tool results, Claude Code's own tagged messages).
fn prompt_kind(c: &Value) -> Option<&'static str> {
    let text = match c {
        Value::String(s) => s.clone(),
        Value::Array(a) if a.iter().all(|x| x["type"] == "text") => a.iter().filter_map(|x| x["text"].as_str()).collect::<Vec<_>>().join("\n"),
        _ => return None,
    };
    let t = text.trim_start();
    (!t.is_empty() && !t.starts_with('<') && !t.starts_with("[Request interrupted")).then(|| task_kind(t))
}

// <dir>/<session>.jsonl → <dir>/<session>/subagents/*.jsonl
fn subagent_files(transcript: &Path) -> Vec<PathBuf> {
    let dir = transcript.with_extension("").join("subagents");
    let mut v: Vec<PathBuf> = fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jsonl")).collect();
    v.sort();
    v
}

// ---------- past sessions: the normal ----------
#[derive(Clone, Debug, PartialEq)]
pub struct Past { pub id: String, pub end: u64, pub active_ms: u64, pub turns: u64, pub models: BTreeMap<String, Tok>, pub tasks: BTreeMap<String, u32> }

impl Past {
    fn json(&self) -> Value {
        let models: Map<String, Value> = self.models.iter().map(|(m, t)| (m.clone(), json!(t.arr()))).collect();
        json!({ "id": self.id, "end": self.end, "activeMs": self.active_ms, "turns": self.turns, "models": models, "tasks": self.tasks })
    }
    fn from(v: &Value) -> Option<Past> {
        let models = v["models"].as_object()?.iter().filter_map(|(m, a)| Some((m.clone(), Tok::from_arr(&a.as_array()?.iter().filter_map(Value::as_u64).collect::<Vec<_>>())))).collect();
        let tasks = v["tasks"].as_object().map(|o| o.iter().filter_map(|(k, n)| Some((k.clone(), n.as_u64()? as u32))).collect()).unwrap_or_default();
        Some(Past { id: v["id"].as_str()?.into(), end: v["end"].as_u64()?, active_ms: v["activeMs"].as_u64().unwrap_or(0), turns: v["turns"].as_u64().unwrap_or(0), models, tasks })
    }
    fn weight(&self, p: &Prices) -> f64 { self.models.iter().filter_map(|(m, t)| p.cost(m, t)).map(|c| c.0).sum() }
}

// The median API-equivalent weight of past sessions, and how many it is taken over. Weighted by price, not raw
// tokens, so a session on Haiku doesn't count like one on Opus.
pub fn normal(past: &[Past], p: &Prices) -> Option<(f64, usize)> {
    let mut w: Vec<f64> = past.iter().filter(|x| x.turns >= MIN_TURNS).map(|x| x.weight(p)).filter(|w| *w > 0.0).collect();
    if w.len() < MIN_PAST { return None; }
    w.sort_by(|a, b| a.total_cmp(b));
    let n = w.len();
    Some((if n % 2 == 1 { w[n / 2] } else { (w[n / 2 - 1] + w[n / 2]) / 2.0 }, n))
}

// ---------- the view the pages get ----------
fn num(st: &Value, k: &str, def: f64) -> f64 { st[k].as_f64().unwrap_or(def) }

fn warnings(t: &Tally, p: &Prices, st: &Value, times: Option<f64>) -> Vec<Value> {
    if st["usageWarn"] != true { return vec![]; }
    let mut out = vec![];
    let limit = num(st, "warnContext", 300.0) as u64 * 1000;
    let times_max = num(st, "warnNormal", 3.0);
    if t.ctx >= limit { out.push(json!({ "kind": "long", "why": "context", "ctx": t.ctx, "limit": limit })); }
    else if let Some(x) = times.filter(|x| *x >= times_max) { out.push(json!({ "kind": "long", "why": "normal", "times": x, "limit": times_max })); }
    let r = &t.recent;
    let heavy = r.iter().filter(|x| p.heavy(&x.model)).count();
    if r.len() >= RECENT && heavy * 10 >= r.len() * 8 && !r.iter().any(|x| x.wrote) {
        let mut outs: Vec<u64> = r.iter().map(|x| x.out).collect();
        outs.sort();
        let med = outs[outs.len() / 2];
        let model = r.iter().rev().find(|x| p.heavy(&x.model)).map(|x| x.model.clone()).unwrap_or_default();
        if med as f64 <= num(st, "warnLightOut", 200.0) { out.push(json!({ "kind": "light", "why": "output", "out": med, "turns": r.len(), "model": model })); }
        else if st["usageContent"] == true && t.last_task.as_deref() == Some("question") { out.push(json!({ "kind": "light", "why": "question", "turns": r.len(), "model": model })); }
    }
    if t.run as f64 >= num(st, "warnExplore", 8.0) && p.heavy(&t.run_model) { out.push(json!({ "kind": "explore", "n": t.run, "model": t.run_model })); }
    out
}

// None when the transcript has no usage yet: the pages then show nothing.
pub fn view(t: &Tally, p: &Prices, st: &Value, normal: Option<f64>) -> Option<Value> {
    let total = t.total();
    if total.turns == 0 { return None; }
    let (mut cost, mut cr_cost, mut unpriced) = (0.0, 0.0, false);
    for ((m, _), x) in &t.models { match p.cost(m, x) { Some((c, r)) => { cost += c; cr_cost += r; } None => unpriced = true } }
    let times = normal.filter(|n| *n > 0.0 && cost > 0.0).map(|n| cost / n);
    let input_all = total.context();
    let models: Vec<Value> = t.models.iter().map(|((m, side), x)| json!({ "model": m, "side": side, "turns": x.turns })).collect();
    let mut v = json!({
        "turns": total.turns, "mainTurns": t.main_turns(),
        "tokens": { "input": total.input, "output": total.output, "cacheWrite": total.cw5 + total.cw1h, "cacheRead": total.cr },
        "models": models, "activeMs": t.active_ms(), "ctx": t.ctx,
        "reread": if input_all > 0 { total.cr as f64 / input_all as f64 } else { 0.0 },
        "rereadCost": if cost > 0.0 { Some(cr_cost / cost) } else { None },
        "times": times,
        "warnings": warnings(t, p, st, times),
    });
    if st["usageCost"] == true && cost > 0.0 { v["cost"] = json!({ "usd": cost, "date": p.date, "partial": unpriced }); }
    if st["usageContent"] == true && !t.tasks.is_empty() { v["tasks"] = json!(t.tasks); }
    Some(v)
}

// ---------- the store: live tallies, past sessions, the price table ----------
pub struct Store {
    dir: PathBuf,
    tallies: HashMap<String, Tally>,
    past: Option<Vec<Past>>, // read on first use
    prices: Prices,
    prices_mark: Option<std::time::SystemTime>,
    queue: Option<Vec<(String, PathBuf, u64)>>, // past transcripts still to read for the normal: (id, file, modified)
}

pub fn prices_file(dir: &Path) -> PathBuf { dir.join("prices.json") }
fn past_file(dir: &Path) -> PathBuf { dir.join("usage.json") }

fn mtime_ms(p: &Path) -> Option<u64> { fs::metadata(p).ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as u64) }

impl Store {
    pub fn new(dir: PathBuf) -> Store { Store { dir, tallies: HashMap::new(), past: None, prices: Prices::default(), prices_mark: None, queue: None } }

    fn past(&mut self) -> &mut Vec<Past> {
        let f = past_file(&self.dir);
        self.past.get_or_insert_with(|| fs::read_to_string(f).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v["sessions"].as_array().map(|a| a.iter().filter_map(Past::from).collect())).unwrap_or_default())
    }

    fn keep(&mut self, p: Past) {
        let past = self.past();
        past.retain(|x| x.id != p.id);
        past.push(p);
        past.sort_by_key(|x| x.end);
        if past.len() > PAST_KEEP { past.drain(..past.len() - PAST_KEEP); }
    }

    fn save_past(&mut self) {
        let v = json!({ "sessions": self.past().iter().map(Past::json).collect::<Vec<_>>() });
        if let Err(e) = crate::settings::save(&past_file(&self.dir), &v) { eprintln!("[bobix] usage: {e}"); }
    }

    // Past transcripts next to the live ones, not counted yet: read a few per tick until done (once per app start).
    fn backfill(&mut self, live: &[(String, String)], now: u64, content: bool) -> bool {
        if self.queue.is_none() {
            let Some(root) = live.iter().find_map(|(_, f)| Path::new(f).parent()?.parent().map(Path::to_path_buf)) else { return false };
            let known: Vec<String> = self.past().iter().map(|p| p.id.clone()).collect();
            let mut q: Vec<(String, PathBuf, u64)> = fs::read_dir(root).into_iter().flatten().flatten().filter(|e| e.path().is_dir())
                .flat_map(|d| fs::read_dir(d.path()).into_iter().flatten().flatten())
                .filter_map(|e| { let p = e.path(); let id = p.file_stem()?.to_str()?.to_string(); (p.extension()? == "jsonl").then_some(())?; let m = mtime_ms(&p)?; Some((id, p, m)) })
                .filter(|(id, _, m)| now.saturating_sub(*m) < BACKFILL_DAYS * 86_400_000 && now.saturating_sub(*m) > 30 * 60_000
                    && !known.contains(id) && !live.iter().any(|(l, _)| l == id))
                .collect();
            q.sort_by_key(|x| std::cmp::Reverse(x.2));
            q.truncate(BACKFILL_MAX);
            self.queue = Some(q);
        }
        let batch: Vec<_> = { let q = self.queue.as_mut().unwrap(); let n = q.len().min(BACKFILL_PER_TICK); q.drain(..n).collect() };
        if batch.is_empty() { return false; }
        for (id, file, end) in batch {
            let mut t = Tally::default();
            t.update(&file.to_string_lossy(), content);
            if t.main_turns() > 0 { self.keep(t.past(&id, end)); }
        }
        true
    }

    pub fn prices(&mut self) -> &Prices {
        let f = prices_file(&self.dir);
        let mark = fs::metadata(&f).and_then(|m| m.modified()).ok();
        if mark.is_none() || mark != self.prices_mark { self.prices = load_prices(&f); self.prices_mark = fs::metadata(&f).and_then(|m| m.modified()).ok(); }
        &self.prices
    }

    // live: (session id, transcript). Returns each live session's view (None: no usage data).
    pub fn tick(&mut self, live: &[(String, String)], st: &Value, now: u64) -> HashMap<String, Option<Value>> {
        let content = st["usageContent"] == true;
        self.prices();
        let ended: Vec<String> = self.tallies.keys().filter(|id| !live.iter().any(|(l, _)| l == *id)).cloned().collect();
        let mut save = false;
        for id in ended {
            let t = self.tallies.remove(&id).unwrap();
            if t.main_turns() > 0 { self.keep(t.past(&id, now)); save = true; }
        }
        for (id, file) in live { self.tallies.entry(id.clone()).or_default().update(file, content); }
        if self.backfill(live, now, content) { save = true; }
        if save { self.save_past(); }
        let p = self.prices.clone();
        let n = normal(self.past(), &p).map(|x| x.0);
        live.iter().map(|(id, _)| (id.clone(), self.tallies.get(id).and_then(|t| view(t, &p, st, n)))).collect()
    }

    // For the pages: the price table's date and state, and what the normal is taken over (no dollars).
    pub fn meta(&mut self) -> Value {
        let p = self.prices().clone();
        let n = normal(self.past(), &p).map(|x| x.1).unwrap_or(0);
        json!({ "date": p.date, "error": p.error, "normalOf": n, "minPast": MIN_PAST })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf { let d = std::env::temp_dir().join(format!("bobix-usage-{name}-{}", std::process::id())); let _ = fs::remove_dir_all(&d); fs::create_dir_all(&d).unwrap(); d }
    fn seed() -> Prices { parse_prices(SEED).unwrap() }
    fn asst(id: &str, model: &str, ts: &str, u: Value, tools: &[(&str, Value)]) -> String {
        let mut content = vec![json!({ "type": "text", "text": "secret answer" })];
        content.extend(tools.iter().map(|(n, i)| json!({ "type": "tool_use", "name": n, "input": i })));
        json!({ "type": "assistant", "timestamp": ts, "message": { "id": id, "model": model, "usage": u, "content": content } }).to_string() + "\n"
    }
    fn u(i: u64, o: u64, cw: u64, h1: u64, cr: u64) -> Value { json!({ "input_tokens": i, "output_tokens": o, "cache_creation_input_tokens": cw, "cache_read_input_tokens": cr, "cache_creation": { "ephemeral_1h_input_tokens": h1 } }) }
    fn st() -> Value { crate::settings::defaults() }

    #[test]
    fn seed_prices_and_matching() {
        let p = seed();
        assert_eq!(p.date, "2026-10-06");
        assert_eq!(p.of("claude-opus-5-5").unwrap().input, 4.0, "opus-5-5 before opus-5");
        assert_eq!(p.of("claude-opus-5").unwrap().input, 5.0);
        assert_eq!(p.of("claude-fable-5-1").unwrap().cr, 0.25);
        assert_eq!(p.of("claude-haiku-4-5-20251001").unwrap().input, 1.0);
        assert!(p.of("gpt-9").is_none());
        assert!(p.heavy("claude-opus-5-5") && !p.heavy("claude-sonnet-5-5") && !p.heavy("claude-haiku-5-5"));
        // 1M of each kind on Opus 5.5: 4 + 20 + 5 + 8 + 0.2
        let (c, r) = p.cost("claude-opus-5-5", &Tok { input: 1_000_000, output: 1_000_000, cw5: 1_000_000, cw1h: 1_000_000, cr: 1_000_000, turns: 1 }).unwrap();
        assert!((c - 37.2).abs() < 1e-9 && (r - 0.2).abs() < 1e-9);
        assert!(parse_prices("{\"models\":[{\"match\":\"x\",\"input\":-1}]}").is_err());
        let d = tmp("prices");
        let f = prices_file(&d);
        assert_eq!(load_prices(&f), seed(), "written from the seed when missing");
        fs::write(&f, "{ broken").unwrap();
        let b = load_prices(&f);
        assert!(b.error.is_some() && b.list == seed().list, "a broken table falls back to the seed and says so");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn reads_incrementally_dedupes_and_counts_subagents() {
        let d = tmp("tally");
        let f = d.join("s1.jsonl");
        let mut text = String::new();
        text += &json!({ "type": "user", "timestamp": "2026-11-10T10:00:00.000Z", "message": { "content": "fix the login bug" } }).to_string(); text += "\n";
        // one answer, two lines (text + tool_use), same id and usage: counted once
        text += &asst("m1", "claude-opus-5-5", "2026-11-10T10:00:05.000Z", u(10, 100, 1000, 1000, 0), &[]);
        text += &asst("m1", "claude-opus-5-5", "2026-11-10T10:00:06.000Z", u(10, 100, 1000, 1000, 0), &[("Read", json!({}))]);
        text += &json!({ "type": "assistant", "message": { "model": "<synthetic>", "usage": u(0, 0, 0, 0, 0) } }).to_string(); text += "\n";
        fs::write(&f, &text).unwrap();
        let mut t = Tally::default();
        assert!(t.update(&f.to_string_lossy(), false));
        assert_eq!(t.total(), Tok { input: 10, output: 100, cw5: 0, cw1h: 1000, cr: 0, turns: 1 });
        assert!(t.tasks.is_empty(), "content opt-in off: no prompt read");
        // half a line is left for the next read
        let more = asst("m2", "claude-opus-5-5", "2026-11-10T10:20:00.000Z", u(5, 50, 0, 0, 2000), &[]);
        let (a, b) = more.split_at(30);
        fs::OpenOptions::new().append(true).open(&f).unwrap().write_all_str(a);
        assert!(t.update(&f.to_string_lossy(), false));
        assert_eq!(t.total().turns, 1);
        fs::OpenOptions::new().append(true).open(&f).unwrap().write_all_str(b);
        assert!(t.update(&f.to_string_lossy(), false));
        assert_eq!((t.total().turns, t.total().cr, t.ctx), (2, 2000, 2005));
        assert!(!t.update(&f.to_string_lossy(), false), "nothing new: nothing read");
        // a subagent in <session>/subagents/
        fs::create_dir_all(d.join("s1/subagents")).unwrap();
        fs::write(d.join("s1/subagents/agent-a.jsonl"), asst("x1", "claude-haiku-5-5", "2026-11-10T10:21:00.000Z", u(1, 2, 0, 0, 3), &[])).unwrap();
        assert!(t.update(&f.to_string_lossy(), false));
        assert_eq!(t.models[&("claude-haiku-5-5".to_string(), true)].turns, 1);
        assert_eq!(t.main_turns(), 2);
        // active: 10:00, 10:20, 10:21 → 1 + min(20, 5) + 1 minutes
        assert_eq!(t.active_ms(), 7 * 60_000);
        let v = view(&t, &seed(), &st(), None).unwrap();
        assert_eq!(v["tokens"], json!({ "input": 16, "output": 152, "cacheWrite": 1000, "cacheRead": 2003 }));
        assert!(v["cost"].is_object() && v["cost"]["date"] == "2026-10-06", "cost shown by default, with the table's date");
        assert!(!v.to_string().contains("secret") && !v.to_string().contains("login"), "no text reaches the view");
        let mut off = st();
        off["usageCost"] = json!(false);
        assert!(view(&t, &seed(), &off, None).unwrap().get("cost").is_none(), "Settings switch hides cost");
        // a rewritten (shorter) transcript starts over
        fs::write(&f, asst("m9", "claude-sonnet-5-5", "2026-11-10T11:00:00.000Z", u(1, 1, 0, 0, 0), &[])).unwrap();
        t.subs.clear();
        let _ = fs::remove_dir_all(d.join("s1"));
        t.update(&f.to_string_lossy(), false);
        assert_eq!(t.total().turns, 1);
        assert!(view(&Tally::default(), &seed(), &st(), None).is_none(), "no usage: nothing to show");
        let _ = fs::remove_dir_all(d);
    }

    trait W { fn write_all_str(&mut self, s: &str); }
    impl W for fs::File { fn write_all_str(&mut self, s: &str) { std::io::Write::write_all(self, s.as_bytes()).unwrap(); } }

    #[test]
    fn three_warnings() {
        let d = tmp("warn");
        let f = d.join("w.jsonl");
        let mut text = String::new();
        for i in 0..10 { text += &asst(&format!("m{i}"), "claude-opus-5-5", "2026-11-10T10:00:00Z", u(1, 120, 0, 0, 50_000), &[]); }
        fs::write(&f, &text).unwrap();
        let mut t = Tally::default();
        t.update(&f.to_string_lossy(), false);
        let kinds = |t: &Tally, s: &Value, times| warnings(t, &seed(), s, times).iter().map(|w| w["kind"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        assert_eq!(kinds(&t, &st(), None), ["light"], "10 Opus turns of short answers, no edits");
        assert_eq!(kinds(&t, &st(), Some(3.5)), ["long", "light"], "3.5× the normal");
        let mut s = st();
        s["usageWarn"] = json!(false);
        assert!(kinds(&t, &s, Some(9.0)).is_empty(), "warnings off");
        // an edit in the window: not light work; a long read-only run on Opus: explore; a big context: long
        let mut more = asst("e1", "claude-opus-5-5", "2026-11-10T10:01:00Z", u(1, 120, 0, 0, 320_000), &[("Edit", json!({}))]);
        for i in 0..4 { more += &asst(&format!("r{i}"), "claude-opus-5-5", "2026-11-10T10:02:00Z", u(1, 50, 0, 0, 320_000), &[("Grep", json!({})), ("Read", json!({})), ("TodoWrite", json!({}))]); }
        fs::OpenOptions::new().append(true).open(&f).unwrap().write_all_str(&more);
        t.update(&f.to_string_lossy(), false);
        let w = warnings(&t, &seed(), &st(), None);
        assert_eq!(w.iter().map(|w| w["kind"].as_str().unwrap()).collect::<Vec<_>>(), ["long", "explore"]);
        assert_eq!((w[0]["why"].as_str(), w[1]["n"].as_u64()), (Some("context"), Some(8)));
        // Bash counts as reading only with the content opt-in
        fs::OpenOptions::new().append(true).open(&f).unwrap().write_all_str(&asst("b1", "claude-opus-5-5", "2026-11-10T10:03:00Z", u(1, 50, 0, 0, 1), &[("Bash", json!({ "command": "git log -5 | head" }))]));
        let mut t2 = Tally::default();
        t2.update(&f.to_string_lossy(), true);
        assert_eq!(t2.run, 9);
        t.update(&f.to_string_lossy(), false);
        assert_eq!(t.run, 0);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn content_opt_in_keeps_kinds_not_text() {
        let d = tmp("content");
        let f = d.join("c.jsonl");
        let user = |c: Value| json!({ "type": "user", "message": { "content": c } }).to_string() + "\n";
        let text = user(json!("Why does the build fail?")) + &user(json!([{ "type": "text", "text": "add a dark mode toggle" }]))
            + &user(json!([{ "type": "tool_result", "content": "fix fix fix" }])) + &user(json!("<task-notification>x</task-notification>"))
            + &asst("m1", "claude-opus-5-5", "2026-11-10T10:00:00Z", u(1, 1, 0, 0, 0), &[]);
        fs::write(&f, text).unwrap();
        let mut t = Tally::default();
        t.update(&f.to_string_lossy(), true);
        assert_eq!(t.tasks, BTreeMap::from([("build".to_string(), 1), ("question".to_string(), 1)]));
        let mut s = st();
        s["usageContent"] = json!(true);
        let v = view(&t, &seed(), &s, None).unwrap().to_string();
        assert!(v.contains("\"tasks\":{\"build\":1,\"question\":1}") && !v.contains("dark mode") && !v.contains("build fail"));
        assert_eq!([task_kind("sửa lỗi đăng nhập"), task_kind("review PR 12"), task_kind("update the README"), task_kind("hello")], ["fix", "review", "docs", "other"]);
        assert!(read_only_command("cd src && grep -rn foo . | head -5") && read_only_command("sed -n 1,20p a.rs") && read_only_command("git status"));
        assert!(!read_only_command("sed -i s/a/b/ x") && !read_only_command("cat a > b") && !read_only_command("npm test") && !read_only_command("git push") && !read_only_command(""));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn normal_is_a_median_of_past_sessions() {
        let p = seed();
        let past = |id: &str, out: u64, turns: u64| Past { id: id.into(), end: 1, active_ms: 0, turns, models: BTreeMap::from([("claude-opus-5-5".into(), Tok { output: out, turns, ..Default::default() })]), tasks: Default::default() };
        let mut v: Vec<Past> = (1..=4).map(|i| past(&format!("p{i}"), i * 1_000_000, 10)).collect();
        assert_eq!(normal(&v, &p), None, "too few sessions yet");
        v.push(past("tiny", 9_000_000, 9)); // under MIN_TURNS: left out
        assert_eq!(normal(&v, &p), None);
        v.push(past("p5", 5_000_000, 10));
        assert_eq!(normal(&v, &p), Some((60.0, 5)), "median of $20, $40, $60, $80, $100");
        assert_eq!(Past::from(&v[0].json()), Some(v[0].clone()));
        assert_eq!(iso_ms("1970-01-01T00:01:00.5Z"), Some(60_500));
        assert_eq!(iso_ms("2026-11-10T15:05:53.123Z"), Some(1_794_323_153_123));
        assert_eq!(iso_ms("nope"), None);
    }

    #[test]
    fn store_records_ended_sessions_and_backfills() {
        let d = tmp("store");
        let root = d.join("projects");
        let proj = root.join("-src-app");
        fs::create_dir_all(&proj).unwrap();
        // five old sessions to learn the normal from, one live
        for i in 0..5 { let f = proj.join(format!("old{i}.jsonl")); fs::write(&f, (0..10).map(|k| asst(&format!("o{i}{k}"), "claude-opus-5-5", "2026-11-01T10:00:00Z", u(0, 300_000, 0, 0, 0), &[])).collect::<String>()).unwrap(); }
        let live = proj.join("live.jsonl");
        fs::write(&live, (0..10).map(|k| asst(&format!("l{k}"), "claude-opus-5-5", "2026-11-10T10:00:00Z", u(0, 600_000, 0, 0, 0), &[])).collect::<String>()).unwrap();
        let data = d.join("data");
        let mut s = Store::new(data.clone());
        let now = mtime_ms(&live).unwrap() + 3_600_000;
        let ids = vec![("live".to_string(), live.to_string_lossy().into_owned())];
        let mut v = HashMap::new();
        for _ in 0..3 { v = s.tick(&ids, &st(), now); }
        assert_eq!(s.past().len(), 5, "past transcripts read a few per tick");
        let lv = v["live"].as_ref().unwrap();
        assert_eq!(lv["times"], json!(2.0), "twice the usual session");
        assert_eq!(s.meta()["normalOf"], 5);
        assert!(prices_file(&data).exists(), "the editable table is written next to the settings");
        // the live session ends: kept as numbers only
        s.tick(&[], &st(), now);
        let saved = fs::read_to_string(data.join("usage.json")).unwrap();
        assert!(saved.contains("\"id\": \"live\"") && !saved.contains("secret"));
        assert_eq!(Store::new(data).past().len(), 6, "read back after a restart");
        let _ = fs::remove_dir_all(d);
    }
}
