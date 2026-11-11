// The skills audit of the Skills screen, and the changes it offers. Each change runs only on the person's click, is
// backed up first in Bobix's own folder (<data>/skill-backups/<id>/) and can be undone from there.
//   audit    every skill Claude loads in a project (skills::all: personal, project, plugin, synced, managed), the size
//            of its description, its uses in the last 30 days and the last one, and duplicates (the same base name in
//            two sets, as acme:build / team:build, or near-identical descriptions). Uses come from Claude Code's transcripts
//            (~/.claude/projects/*/*.jsonl and their subagents/): the Skill tool's calls, and typed /commands only
//            with Settings → usageContent on. Only the skill's name and the time are kept, never any text.
//   disable  moves a personal or project skill's folder into the backup; remove copies it there, then deletes it;
//            undo puts it back as it was. Plugin skills are not changed: the audit says which setting would turn the
//            plugin off. Managed and claude.ai skills are not Bobix's to change.
//   rules    adds the "When to split, which model" block to the project's CLAUDE.md; undo restores the file exactly
//            (or, when it was edited since, takes out only the block).
// Never touches Claude's login, credentials or settings files.
use crate::{skills::{self, Skill}, usage::iso_ms};
use serde_json::{json, Value};
use std::{collections::{BTreeMap, HashMap, HashSet}, fs, io::{BufRead, BufReader}, path::{Path, PathBuf}, sync::Mutex, time::{Duration, SystemTime, UNIX_EPOCH}};

const DAY_MS: u64 = 86_400_000;
const DAYS: u64 = 30;

pub const RULES: &str = "## When to split, which model
- Read-only exploration (search, read, map code): the built-in
  Explore subagent (Haiku).
- Edits and tests: Sonnet.
- A risky change to review, or a design choice: Opus.
- Small task (one area, under ~30 min): one Sonnet session, no split.
- Keep the lead session thin: hand out work, collect results.
- One session per ticket; /clear between unrelated tasks. Past 2×
  your usual length, start a new session from a short summary.";
const OPEN: &str = "<!-- bobix:routing (added by Bobix; take it out from Bobix's Skills screen) -->";
const CLOSE: &str = "<!-- /bobix:routing -->";

pub fn now_ms() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0) }

// ---------- uses, from the transcripts ----------

// Per transcript: (size, modified, slash commands read too) → (skill, ms) of each call. Files are only re-read when
// they changed.
type Hits = Vec<(String, u64)>;
static SEEN: Mutex<Option<HashMap<PathBuf, (u64, SystemTime, bool, Hits)>>> = Mutex::new(None);

pub fn transcripts(projects: &Path, since: SystemTime) -> Vec<(PathBuf, fs::Metadata)> {
    let mut out = vec![];
    let mut add = |p: PathBuf| if let Ok(m) = fs::metadata(&p) {
        if m.is_file() && m.modified().is_ok_and(|t| t >= since) { out.push((p, m)); }
    };
    let jsonl = |d: &Path| fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jsonl")).collect::<Vec<_>>();
    for proj in fs::read_dir(projects).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
        for f in jsonl(&proj) { add(f); }
        for s in fs::read_dir(&proj).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
            for f in jsonl(&s.join("subagents")) { add(f); }
        }
    }
    out
}

// The skill calls in one transcript. Only lines naming the Skill tool (and, with `slash`, typed /commands) are parsed.
fn hits_of(path: &Path, slash: bool) -> Hits {
    let Ok(f) = fs::File::open(path) else { return vec![] };
    let mut out = vec![];
    for line in BufReader::new(f).split(b'\n').map_while(Result::ok) {
        let tool = memfind(&line, "\"name\":\"Skill\"");
        let cmd = slash && memfind(&line, "<command-name>/");
        if !tool && !cmd { continue; }
        let Ok(j) = serde_json::from_slice::<Value>(&line) else { continue };
        let Some(t) = j["timestamp"].as_str().and_then(iso_ms) else { continue };
        if tool && j["type"] == "assistant" {
            for c in j["message"]["content"].as_array().into_iter().flatten() {
                if c["type"] == "tool_use" && c["name"] == "Skill" { if let Some(s) = c["input"]["skill"].as_str() { out.push((s.trim_start_matches('/').to_string(), t)); } }
            }
        }
        if cmd && j["type"] == "user" {
            let text = match &j["message"]["content"] { Value::String(s) => s.clone(), v => v.to_string() };
            if let Some(name) = text.split_once("<command-name>/").and_then(|(_, r)| r.split_once("</command-name>")).map(|x| x.0) {
                if !name.is_empty() && name.len() < 120 && !name.contains(char::is_whitespace) { out.push((name.to_string(), t)); }
            }
        }
    }
    out
}

fn memfind(h: &[u8], n: &str) -> bool { std::str::from_utf8(h).is_ok_and(|s| s.contains(n)) }

// Skill name → (uses, last use in ms) over the last 30 days, from every transcript under `projects`.
pub fn uses(projects: &Path, now: u64, slash: bool) -> HashMap<String, (u32, u64)> {
    let from = now.saturating_sub(DAYS * DAY_MS);
    let files = transcripts(projects, UNIX_EPOCH + Duration::from_millis(from));
    let mut seen = SEEN.lock().unwrap();
    let cache = seen.get_or_insert_with(HashMap::new);
    let mut out: HashMap<String, (u32, u64)> = HashMap::new();
    let mut keep = HashSet::new();
    for (p, m) in files {
        let (len, at) = (m.len(), m.modified().unwrap_or(UNIX_EPOCH));
        let fresh = cache.get(&p).is_some_and(|c| c.0 == len && c.1 == at && c.2 == slash);
        if !fresh { let h = hits_of(&p, slash); cache.insert(p.clone(), (len, at, slash, h)); }
        for (name, t) in &cache[&p].3 {
            if *t < from { continue; }
            let e = out.entry(name.clone()).or_default();
            e.0 += 1;
            e.1 = e.1.max(*t);
        }
        keep.insert(p);
    }
    cache.retain(|p, _| keep.contains(p));
    out
}

// ---------- the audit ----------

fn base(name: &str) -> String { name.rsplit(':').next().unwrap_or(name).to_lowercase() }

pub fn words(s: &str) -> HashSet<String> {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 2).map(String::from).collect()
}

// Groups of skills that are the same: the same base name in two sets, or near-identical descriptions (most words
// shared). ponytail: compares every pair, fine for a few hundred skills.
fn duplicates(list: &[Skill]) -> Vec<(Vec<usize>, &'static str)> {
    let mut parent: Vec<usize> = (0..list.len()).collect();
    fn root(p: &mut [usize], mut i: usize) -> usize { while p[i] != i { p[i] = p[p[i]]; i = p[i]; } i }
    let w: Vec<HashSet<String>> = list.iter().map(|s| words(&s.description)).collect();
    for i in 0..list.len() {
        for j in i + 1..list.len() {
            let (a, b) = (&w[i], &w[j]);
            let same_text = a.len() >= 6 && b.len() >= 6 && a.intersection(b).count() * 10 >= a.union(b).count() * 9;
            if base(&list[i].name) == base(&list[j].name) || same_text { let (ri, rj) = (root(&mut parent, i), root(&mut parent, j)); parent[rj] = ri; }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..list.len() { let r = root(&mut parent, i); groups.entry(r).or_default().push(i); }
    groups.into_values().filter(|g| g.len() > 1).map(|g| {
        let names: HashSet<String> = g.iter().map(|&i| base(&list[i].name)).collect();
        let why = if names.len() < g.len() { "name" } else { "description" };
        (g, why)
    }).collect()
}

// The names Claude may log a call under: the skill's name and its folder's (the Skill tool uses either).
fn aliases(s: &Skill) -> Vec<String> {
    let mut out = vec![s.name.clone()];
    if let Some(f) = s.dir.file_name().map(|f| f.to_string_lossy().into_owned()) {
        let prefixed = matches!(s.source, "plugin" | "team" | "synced");
        let a = match (prefixed, s.name.rsplit_once(':')) { (true, Some((p, _))) => format!("{p}:{f}"), _ => f };
        if a != s.name { out.push(a); }
    }
    out
}

// What a change to the skill would be. Plugin skills: the setting that would turn the whole plugin off (not done).
fn change(s: &Skill, list: &[Skill]) -> Value {
    match s.source {
        "personal" | "project" => json!({ "can": true }),
        "plugin" | "team" => {
            let key = s.plugin.clone().unwrap_or_default();
            let n = list.iter().filter(|x| x.plugin.as_deref() == Some(key.as_str())).count();
            let file = if s.source == "team" { ".claude/settings.json (the project's, shared with the team)" } else { "~/.claude/settings.json" };
            json!({ "can": false, "why": "plugin", "plan": { "file": file, "set": format!("enabledPlugins[\"{key}\"] = false"), "skills": n,
                "note": "turns the whole plugin off; Bobix does not edit Claude's settings yet" } })
        }
        "managed" => json!({ "can": false, "why": "managed" }),
        _ => json!({ "can": false, "why": "synced" }),
    }
}

// The audit of the skills Claude loads in `dir` (None: wherever you start Claude, your own skills only).
pub fn audit(home: &Path, dir: Option<&Path>, now: u64, slash: bool) -> Value {
    let dir = dir.map(Path::to_path_buf).unwrap_or_else(|| home.join(".claude/no-project"));
    let list = skills::all(home, &dir);
    let used = uses(&home.join(".claude/projects"), now, slash);
    let dups = duplicates(&list);
    let mut dup_of = vec![Value::Null; list.len()];
    for (g, (members, _)) in dups.iter().enumerate() { for &i in members { dup_of[i] = json!(g); } }
    let mut by_source: BTreeMap<&str, u32> = BTreeMap::new();
    let (mut chars, mut unused) = (0usize, 0u32);
    let rows: Vec<Value> = list.iter().enumerate().map(|(i, s)| {
        let (n, last) = aliases(s).iter().filter_map(|a| used.get(a)).fold((0u32, 0u64), |(n, l), (c, t)| (n + c, l.max(*t)));
        *by_source.entry(s.source).or_default() += 1;
        chars += s.name.len() + s.size;
        if n == 0 { unused += 1; }
        json!({ "key": s.dir.to_string_lossy(), "name": s.name, "source": s.source, "plugin": s.plugin, "description": s.description,
            "size": s.size, "hidden": s.hidden, "uses": n, "lastUsed": if last > 0 { json!(last) } else { Value::Null },
            "unused": n == 0, "dup": dup_of[i], "change": change(s, &list) })
    }).collect();
    let groups: Vec<Value> = dups.iter().map(|(g, why)| json!({ "why": why, "names": g.iter().map(|&i| &list[i].name).collect::<Vec<_>>() })).collect();
    json!({ "days": DAYS, "slashCounted": slash, "skills": rows, "duplicates": groups,
        "summary": { "skills": list.len(), "bySource": by_source, "descriptionChars": chars, "tokensPerTurn": chars / 4, "unused": unused, "duplicateGroups": dups.len() } })
}

// ---------- backups, changes and undo ----------

static WRITE: Mutex<()> = Mutex::new(());

pub fn backup_dir(data: &Path) -> PathBuf { data.join("skill-backups") }

fn new_entry(data: &Path) -> std::io::Result<(String, PathBuf)> {
    let mut r = [0u8; 4];
    getrandom::fill(&mut r).map_err(|e| std::io::Error::other(e.to_string()))?;
    let id = format!("{}-{}", now_ms(), r.iter().map(|b| format!("{b:02x}")).collect::<String>());
    let p = backup_dir(data).join(&id);
    fs::create_dir_all(&p)?;
    Ok((id, p))
}

// A copy of `from` at `to`, links kept as links (a skill folder may be a link to a shared kit).
pub fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    let m = fs::symlink_metadata(from)?;
    if m.file_type().is_symlink() {
        let target = fs::read_link(from)?;
        #[cfg(unix)] return std::os::unix::fs::symlink(target, to);
        #[cfg(windows)] return if fs::metadata(from)?.is_dir() { std::os::windows::fs::symlink_dir(target, to) } else { std::os::windows::fs::symlink_file(target, to) };
    }
    if m.is_dir() {
        fs::create_dir(to)?;
        for e in fs::read_dir(from)? { let e = e?; copy_tree(&e.path(), &to.join(e.file_name()))?; }
        Ok(())
    } else { fs::copy(from, to).map(|_| ()) }
}

fn delete(p: &Path) -> std::io::Result<()> {
    if fs::symlink_metadata(p)?.is_dir() { fs::remove_dir_all(p) } else { fs::remove_file(p) }
}

// Moves a folder (or link); across disks, a copy and then a delete.
fn move_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    if fs::rename(from, to).is_ok() { return Ok(()); }
    copy_tree(from, to)?;
    delete(from)
}

fn err(e: impl ToString) -> Value { json!({ "ok": false, "error": e.to_string() }) }

// Disables ("disable": moved into the backup) or removes ("remove": copied there, then deleted) the skill whose
// folder is `key`, one the audit of `dir` lists. { ok, id } or { ok: false, error, plan? }.
pub fn apply(home: &Path, dir: Option<&Path>, data: &Path, key: &str, act: &str) -> Value {
    if act != "disable" && act != "remove" { return err("unknown change"); }
    let dir = dir.map(Path::to_path_buf).unwrap_or_else(|| home.join(".claude/no-project"));
    let list = skills::all(home, &dir);
    // Compared as paths: on Windows "\\" and "/" are the same separator.
    let Some(s) = list.iter().find(|s| s.dir == Path::new(key)) else { return err("no such skill here") };
    let c = change(s, &list);
    if c["can"] != true { let mut r = err(format!("not changed: a {} skill", c["why"].as_str().unwrap_or("other"))); r["plan"] = c["plan"].clone(); return r; }
    // Only a folder directly in a skills folder Claude reads (defence in depth: the list gave it).
    if !s.dir.parent().is_some_and(|p| p.ends_with(".claude/skills")) { return err("not in a skills folder"); }
    let _w = WRITE.lock().unwrap();
    let (id, p) = match new_entry(data) { Ok(x) => x, Err(e) => return err(e) };
    let entry = json!({ "id": id, "kind": act, "name": s.name, "source": s.source, "path": s.dir, "at": now_ms() });
    let saved = p.join("skill");
    // Disable: one rename when it can; otherwise (and for Remove) a whole copy first, and only then the delete.
    let r = fs::write(p.join("entry.json"), serde_json::to_vec_pretty(&entry).unwrap_or_default())
        .and_then(|_| if act == "disable" && fs::rename(&s.dir, &saved).is_ok() { Ok(true) } else { copy_tree(&s.dir, &saved).map(|_| false) });
    match r {
        Ok(true) => json!({ "ok": true, "id": id }),
        Ok(false) => match delete(&s.dir) {
            Ok(()) => json!({ "ok": true, "id": id }),
            Err(e) => { let mut r = err(e); r["id"] = json!(id); r } // the original may be half gone: the backup stays
        },
        Err(e) => { let _ = fs::remove_dir_all(&p); err(e) } // the original is untouched
    }
}

fn entry(data: &Path, id: &str) -> Option<(PathBuf, Value)> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') { return None; }
    let p = backup_dir(data).join(id);
    let v = serde_json::from_slice(&fs::read(p.join("entry.json")).ok()?).ok()?;
    Some((p, v))
}

// The changes Bobix can undo, newest first.
pub fn backups(data: &Path) -> Value {
    let mut ids: Vec<String> = fs::read_dir(backup_dir(data)).into_iter().flatten().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    ids.sort_by(|a, b| b.cmp(a));
    json!(ids.iter().filter_map(|id| entry(data, id).map(|(_, v)| v)).collect::<Vec<_>>())
}

// Undoes one change: the skill's folder back where it was, or CLAUDE.md as it was. The backup goes once undone.
pub fn undo(data: &Path, id: &str) -> Value {
    let _w = WRITE.lock().unwrap();
    let Some((p, e)) = entry(data, id) else { return err("no such backup") };
    let Some(path) = e["path"].as_str().map(PathBuf::from) else { return err("bad backup") };
    let r = if e["kind"] == "rules" { undo_rules(&p, &path) } else {
        if fs::symlink_metadata(&path).is_ok() { return err("a skill is already there; move it away first"); }
        fs::create_dir_all(path.parent().unwrap_or(Path::new("/"))).and_then(|_| move_tree(&p.join("skill"), &path)).map(|_| json!({ "ok": true }))
    };
    match r { Ok(v) => { let _ = fs::remove_dir_all(&p); v } Err(e) => err(e) }
}

// ---------- the rule block in CLAUDE.md ----------

// The project's CLAUDE.md: the one at the repository root, else .claude/CLAUDE.md, else a new one at the root.
pub fn claude_md(dir: &Path) -> PathBuf {
    let root = skills::repo_root(dir);
    let (a, b) = (root.join("CLAUDE.md"), root.join(".claude/CLAUDE.md"));
    if !a.is_file() && b.is_file() { b } else { a }
}

fn block() -> String { format!("{OPEN}\n{RULES}\n{CLOSE}\n") }

// Whether the block is in the project's CLAUDE.md, the exact text Bobix would add, and the backup that undoes it.
pub fn rules_status(dir: &Path, data: &Path) -> Value {
    let file = claude_md(dir);
    let present = fs::read_to_string(&file).is_ok_and(|t| t.contains(OPEN));
    let undo = backups(data).as_array().into_iter().flatten().find(|e| e["kind"] == "rules" && e["path"] == json!(file)).map(|e| e["id"].clone());
    json!({ "file": file, "present": present, "text": RULES, "block": block(), "undo": undo })
}

// Adds the block at the end of the project's CLAUDE.md (made when there is none), after a backup.
pub fn rules_add(dir: &Path, data: &Path) -> Value {
    let _w = WRITE.lock().unwrap();
    let file = claude_md(dir);
    let before = match fs::read(&file) { Ok(b) => Some(b), Err(e) if e.kind() == std::io::ErrorKind::NotFound => None, Err(e) => return err(e) };
    if before.as_ref().is_some_and(|b| String::from_utf8_lossy(b).contains(OPEN)) { return err("the block is already there"); }
    let mut after = before.clone().unwrap_or_default();
    if !after.is_empty() { if !after.ends_with(b"\n") { after.push(b'\n'); } after.push(b'\n'); }
    after.extend_from_slice(block().as_bytes());
    let r = (|| -> std::io::Result<String> {
        let (id, p) = new_entry(data)?;
        if let Some(b) = &before { fs::write(p.join("before"), b)?; }
        fs::write(p.join("after"), &after)?;
        fs::write(p.join("entry.json"), serde_json::to_vec_pretty(&json!({ "id": id, "kind": "rules", "name": "CLAUDE.md", "path": file, "existed": before.is_some(), "at": now_ms() }))?)?;
        if let Err(e) = fs::write(&file, &after) { let _ = fs::remove_dir_all(&p); return Err(e); }
        Ok(id)
    })();
    match r { Ok(id) => json!({ "ok": true, "id": id }), Err(e) => err(e) }
}

// As it was when the file is as Bobix left it; edited since: only the block comes out, the edits stay.
fn undo_rules(p: &Path, file: &Path) -> std::io::Result<Value> {
    let now = match fs::read(file) { Ok(b) => Some(b), Err(e) if e.kind() == std::io::ErrorKind::NotFound => None, Err(e) => return Err(e) };
    if now.as_deref() == Some(fs::read(p.join("after"))?.as_slice()) {
        match fs::read(p.join("before")) {
            Ok(b) => fs::write(file, b)?,
            Err(_) => fs::remove_file(file)?, // Bobix made the file
        }
        return Ok(json!({ "ok": true, "exact": true }));
    }
    let Some(now) = now else { return Ok(json!({ "ok": true, "exact": false })) }; // already gone
    let t = String::from_utf8_lossy(&now).into_owned();
    let (Some(a), Some(b)) = (t.find(OPEN), t.find(CLOSE)) else { return Ok(json!({ "ok": true, "exact": false })) };
    if b < a { return Ok(json!({ "ok": false, "error": "the block's markers are out of order; edit CLAUDE.md by hand" })); }
    let mut end = b + CLOSE.len();
    if t[end..].starts_with('\n') { end += 1; }
    let mut start = a;
    if t[..start].ends_with("\n\n") { start -= 1; } // the blank line Bobix put before it
    fs::write(file, format!("{}{}", &t[..start], &t[end..]))?;
    Ok(json!({ "ok": true, "exact": false }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, text: &str) { fs::create_dir_all(p.parent().unwrap()).unwrap(); fs::write(p, text).unwrap(); }

    fn scratch(n: &str) -> PathBuf {
        let t = std::env::temp_dir().join(format!("bobix-audit-{n}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        t
    }

    // Every file under `p` with its bytes (and links with their targets), for a byte-for-byte comparison.
    fn snapshot(p: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut out = BTreeMap::new();
        fn walk(root: &Path, p: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
            let m = fs::symlink_metadata(p).unwrap();
            if m.file_type().is_symlink() { out.insert(format!("{rel} ->"), fs::read_link(p).unwrap().to_string_lossy().as_bytes().to_vec()); }
            else if m.is_dir() { out.insert(format!("{rel}/"), vec![]); for e in fs::read_dir(p).unwrap() { walk(root, &e.unwrap().path(), out); } }
            else { out.insert(rel, fs::read(p).unwrap()); }
        }
        walk(p, p, &mut out);
        out
    }

    fn tool_line(skill: &str, ts: &str) -> String {
        json!({ "type": "assistant", "timestamp": ts, "message": { "content": [{ "type": "tool_use", "name": "Skill", "input": { "skill": skill, "args": "secret prompt" } }] } }).to_string() + "\n"
    }

    #[test]
    fn audit_counts_uses_flags_unused_and_duplicates() {
        let t = scratch("list");
        let (home, repo) = (t.join("home"), t.join("repo"));
        let c = home.join(".claude");
        write(&c.join("skills/acme-build/SKILL.md"), "---\nname: acme:build\ndescription: Implement features, plans, and fixes with a structured workflow\n---\n");
        write(&c.join("skills/team-build/SKILL.md"), "---\nname: team:build\ndescription: Something else entirely here\n---\n");
        write(&c.join("skills/plan/SKILL.md"), "---\nname: plan\ndescription: Plan implementations and design architectures with detailed phases\n---\n");
        write(&c.join("skills/roadmap/SKILL.md"), "---\nname: roadmap\ndescription: Plan implementations and design architectures, with detailed phases\n---\n");
        write(&c.join("skills/quiet/SKILL.md"), "---\nname: quiet\nuser-invocable: false\ndescription: Only Claude\n---\n");
        fs::create_dir_all(repo.join(".git")).unwrap();
        write(&repo.join(".claude/skills/plan/SKILL.md"), "---\nname: plan\ndescription: the repo's own\n---\n");
        let now = iso_ms("2026-11-10T12:00:00.000Z").unwrap();
        let p = c.join("projects/-repo");
        let mut text = tool_line("acme-build", "2026-11-09T10:00:00.000Z") + &tool_line("acme:build", "2026-11-10T09:00:00.000Z") + &tool_line("plan", "2026-09-01T09:00:00.000Z");
        text += &(json!({ "type": "user", "timestamp": "2026-11-08T10:00:00.000Z", "message": { "content": "<command-name>/roadmap</command-name>\n<command-args>my plan</command-args>" } }).to_string() + "\n");
        write(&p.join("s1.jsonl"), &text);
        write(&p.join("s1/subagents/a.jsonl"), &tool_line("quiet", "2026-11-10T08:00:00.000Z"));
        let a = audit(&home, Some(&repo), now, false);
        let row = |n: &str, src: &str| a["skills"].as_array().unwrap().iter().find(|r| r["name"] == n && r["source"] == src).cloned().unwrap();
        assert_eq!(row("acme:build", "personal")["uses"], 2, "by name and by folder");
        assert_eq!(row("acme:build", "personal")["lastUsed"], iso_ms("2026-11-10T09:00:00.000Z").unwrap());
        assert_eq!(row("plan", "personal")["unused"], true, "a use older than 30 days doesn't count");
        assert_eq!(row("roadmap", "personal")["uses"], 0, "typed /commands only with usageContent on");
        assert_eq!(row("quiet", "personal")["uses"], 1, "subagents' calls count; skills only Claude calls are listed");
        assert_eq!(row("plan", "project")["source"], "project", "a shadowed duplicate is listed too");
        let groups: Vec<(String, Vec<String>)> = a["duplicates"].as_array().unwrap().iter()
            .map(|g| (g["why"].as_str().unwrap().into(), g["names"].as_array().unwrap().iter().map(|n| n.as_str().unwrap().to_string()).collect())).collect();
        assert!(groups.contains(&("name".into(), vec!["acme:build".into(), "team:build".into()])), "{groups:?}");
        assert!(groups.iter().any(|(_, n)| n.contains(&"roadmap".to_string()) && n.contains(&"plan".to_string())), "near-identical descriptions: {groups:?}");
        assert_eq!(a["summary"]["skills"], 6);
        let on = audit(&home, Some(&repo), now, true);
        assert_eq!(on["skills"].as_array().unwrap().iter().find(|r| r["name"] == "roadmap").unwrap()["uses"], 1);
        assert!(!serde_json::to_string(&on).unwrap().contains("secret prompt") && !on.to_string().contains("my plan"), "no transcript text kept");
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn remove_then_undo_restores_the_files_byte_for_byte() {
        let t = scratch("undo");
        let (home, data) = (t.join("home"), t.join("data"));
        let s = home.join(".claude/skills/acme-build");
        write(&s.join("SKILL.md"), "---\nname: acme:build\ndescription: Cook\n---\nbody\n");
        fs::create_dir_all(s.join("scripts")).unwrap();
        fs::write(s.join("scripts/run.bin"), (0u8..=255).collect::<Vec<u8>>()).unwrap();
        #[cfg(unix)] std::os::unix::fs::symlink("../SKILL.md", s.join("scripts/link")).unwrap();
        let before = snapshot(&s);
        for act in ["remove", "disable"] {
            let r = apply(&home, None, &data, &s.to_string_lossy(), act);
            assert_eq!(r["ok"], true, "{act}: {r}");
            assert!(fs::symlink_metadata(&s).is_err(), "{act}: gone from Claude's folder");
            assert_eq!(backups(&data)[0]["kind"], act);
            let u = undo(&data, r["id"].as_str().unwrap());
            assert_eq!(u["ok"], true, "{u}");
            assert_eq!(snapshot(&s), before, "{act} then undo: the same files, bytes and links");
            assert_eq!(backups(&data), json!([]), "the backup goes once undone");
        }
        // The cross-disk path of a move: a copy, then a delete.
        let (copy, back) = (t.join("copy"), t.join("back"));
        copy_tree(&s, &copy).unwrap();
        move_tree(&copy, &back).unwrap();
        assert_eq!(snapshot(&back), before);
        assert_eq!(undo(&data, "../etc")["ok"], false);
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn plugin_and_managed_skills_are_not_changed() {
        let t = scratch("plugin");
        let (home, data) = (t.join("home"), t.join("data"));
        let root = t.join("plugins/kit");
        write(&root.join(".claude-plugin/plugin.json"), r#"{ "name": "kit" }"#);
        write(&root.join("skills/a/SKILL.md"), "---\nname: a\n---\n");
        write(&root.join("skills/b/SKILL.md"), "---\nname: b\n---\n");
        write(&home.join(".claude/plugins/installed_plugins.json"), &json!({ "plugins": { "kit@m": [{ "scope": "user", "installPath": root }] } }).to_string());
        let r = apply(&home, None, &data, &root.join("skills/a").to_string_lossy(), "remove");
        assert_eq!(r["ok"], false);
        assert_eq!(r["plan"]["set"], "enabledPlugins[\"kit@m\"] = false");
        assert_eq!(r["plan"]["skills"], 2);
        assert!(root.join("skills/a/SKILL.md").is_file(), "untouched");
        assert_eq!(apply(&home, None, &data, "/etc", "remove")["ok"], false, "only a skill the audit lists");
        assert!(!backup_dir(&data).exists() || backups(&data) == json!([]));
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn rule_block_added_and_undone_exactly() {
        let t = scratch("rules");
        let (repo, data) = (t.join("repo"), t.join("data"));
        fs::create_dir_all(repo.join(".git")).unwrap();
        let file = repo.join("CLAUDE.md");
        // No CLAUDE.md: made, then taken away again.
        let r = rules_add(&repo.join("sub"), &data);
        assert_eq!(r["ok"], true, "{r}");
        assert!(fs::read_to_string(&file).unwrap().contains(RULES));
        assert_eq!(rules_add(&repo, &data)["ok"], false, "once");
        assert_eq!(rules_status(&repo, &data)["undo"], r["id"]);
        assert_eq!(undo(&data, r["id"].as_str().unwrap())["exact"], true);
        assert!(!file.exists());
        // An existing one without a final newline: back byte for byte.
        let orig = b"# Project\r\nKeep this.\xef\xbb\xbf".to_vec();
        fs::write(&file, &orig).unwrap();
        let r = rules_add(&repo, &data);
        assert_eq!(rules_status(&repo, &data)["present"], true);
        assert_eq!(undo(&data, r["id"].as_str().unwrap())["exact"], true);
        assert_eq!(fs::read(&file).unwrap(), orig);
        // Edited since: only the block comes out, the edit stays.
        fs::write(&file, "# Project\n").unwrap();
        let r = rules_add(&repo, &data);
        let mut t2 = fs::read_to_string(&file).unwrap();
        t2.push_str("\nMy note.\n");
        fs::write(&file, &t2).unwrap();
        assert_eq!(undo(&data, r["id"].as_str().unwrap())["exact"], false);
        assert_eq!(fs::read_to_string(&file).unwrap(), "# Project\n\nMy note.\n");
        let _ = fs::remove_dir_all(&t);
    }

    // The audit of this machine (read-only): counts only. BOBIX_SLASH=1 counts typed /commands too. BOBIX_SKILLS_DIR=/path/to/repo cargo test live_audit -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_audit() {
        let dir = std::env::var("BOBIX_SKILLS_DIR").unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").into());
        let t0 = std::time::Instant::now();
        let slash = std::env::var_os("BOBIX_SLASH").is_some();
        let a = audit(&crate::paths::home(), Some(Path::new(&dir)), now_ms(), slash);
        println!("first {:?}: {}", t0.elapsed(), a["summary"]);
        let t1 = std::time::Instant::now();
        let _ = audit(&crate::paths::home(), Some(Path::new(&dir)), now_ms(), slash);
        println!("again {:?}", t1.elapsed());
        let used = a["skills"].as_array().unwrap().iter().filter(|r| r["uses"].as_u64() > Some(0)).count();
        println!("used: {used}, groups: {}", a["duplicates"].as_array().unwrap().len());
    }
}
