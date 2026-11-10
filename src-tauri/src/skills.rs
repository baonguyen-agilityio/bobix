// The Claude Code skills a session started in a folder can call, so the Start box can offer one: names and descriptions
// only, read from where Claude Code keeps them (https://code.claude.com/docs/en/skills, /plugins-reference, /settings).
// Read-only here. The skills audit's changes, each on the person's click and undoable, are in skill_audit.rs; no plugin or
// settings file is ever changed.
//   managed   <system dir>/.claude/skills/<skill>/SKILL.md (macOS /Library/Application Support/ClaudeCode, Linux /etc/claude-code,
//             Windows C:\Program Files\ClaudeCode)
//   personal  ~/.claude/skills/<skill>/SKILL.md
//   synced    ~/.claude/skills/synced/<id>/<skill>/SKILL.md (skills enabled on the claude.ai account), /anthropic-skills:<name>
//   project   .claude/skills/<skill>/SKILL.md in the folder and each parent up to the repository root
//   plugin    <installPath>/skills/<skill>/SKILL.md (plus the manifest's "skills" folders, or a SKILL.md at its root) of each
//             plugin in ~/.claude/plugins/installed_plugins.json installed for you or for this folder and enabled
//             (enabledPlugins: user < project < local settings), /<plugin>:<name>. "team" when the plugin is the
//             project's: installed for this folder, or enabled in its shared .claude/settings.json.
use serde::Serialize;
use serde_json::Value;
use std::{fs, io::Read, path::{Path, PathBuf}};

#[derive(Serialize, Clone, PartialEq, Debug, Default)]
pub struct Skill {
    pub name: String, pub description: String, pub source: &'static str,
    // For the audit (skill_audit.rs) only, never sent with the lists above: the skill's folder, the size of its whole
    // description, the plugin's key in installed_plugins.json, and whether only Claude may call it.
    #[serde(skip)] pub dir: PathBuf, #[serde(skip)] pub size: usize, #[serde(skip)] pub plugin: Option<String>, #[serde(skip)] pub hidden: bool,
}

// What to read: `all` keeps the skills only Claude may call and those another place shadows (the audit wants every
// skill Claude loads, duplicates included).
#[derive(Clone, Copy)]
struct Mode { all: bool }

fn push(out: &mut Vec<Skill>, s: Skill, m: Mode) { if m.all || !out.iter().any(|x| x.name == s.name) { out.push(s); } }

// `key: value` lines of a SKILL.md's front matter (between the first two "---" lines). A value may be quoted, or a
// YAML block (">" / "|") on the indented lines after it.
fn front_matter(text: &str) -> Vec<(String, String)> {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") { return vec![]; }
    let body: Vec<&str> = lines.take_while(|l| l.trim() != "---").collect();
    let mut out = vec![];
    let mut i = 0;
    while i < body.len() {
        let l = body[i];
        i += 1;
        if l.starts_with([' ', '\t', '#']) { continue; }
        let Some((k, v)) = l.split_once(':') else { continue };
        let mut v = v.trim().to_string();
        if v.starts_with('>') || v.starts_with('|') {
            let mut parts = vec![];
            while i < body.len() && (body[i].starts_with([' ', '\t']) || body[i].trim().is_empty()) { parts.push(body[i].trim()); i += 1; }
            v = parts.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ");
        } else if v.len() >= 2 && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\''))) {
            v = v[1..v.len() - 1].replace("\\\"", "\"").replace("''", "'");
        }
        out.push((k.trim().to_string(), v));
    }
    out
}

// One skill folder → its command (front matter `name`, else the folder's name) and description; None when it has no
// SKILL.md or only Claude may call it (user-invocable: false). Only the first 16 KB are read.
fn read_skill(dir: &Path, prefix: Option<&str>, source: &'static str, m: Mode) -> Option<Skill> {
    let mut buf = Vec::new();
    fs::File::open(dir.join("SKILL.md")).ok()?.take(16 * 1024).read_to_end(&mut buf).ok()?;
    let fm = front_matter(&String::from_utf8_lossy(&buf));
    let get = |k: &str| fm.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone());
    let hidden = get("user-invocable").as_deref() == Some("false");
    if hidden && !m.all { return None; }
    let name = get("name").filter(|n| !n.is_empty()).or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()))?;
    let name = match prefix { Some(p) => format!("{p}:{name}"), None => name };
    if name.chars().any(|c| c.is_whitespace() || c == '/') { return None; }
    let full = get("description").unwrap_or_default();
    let description: String = full.chars().take(300).collect();
    Some(Skill { name, description, source, dir: dir.to_path_buf(), size: full.chars().count(), plugin: None, hidden })
}

// Every skill folder in `root` (each `<skill>/SKILL.md`), by name.
fn read_dir(root: &Path, prefix: Option<&str>, source: &'static str, out: &mut Vec<Skill>, m: Mode) {
    let Ok(rd) = fs::read_dir(root) else { return };
    let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    for d in dirs {
        if let Some(s) = read_skill(&d, prefix, source, m) { push(out, s, m); }
    }
}

fn json(p: &Path) -> Value { fs::read_to_string(p).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null) }

pub fn managed_dir() -> PathBuf {
    if cfg!(target_os = "macos") { PathBuf::from("/Library/Application Support/ClaudeCode") }
    else if cfg!(windows) { PathBuf::from(r"C:\Program Files\ClaudeCode") }
    else { PathBuf::from("/etc/claude-code") }
}

// The folder and its parents up to the repository root (the first one holding .git), nearest first.
fn up_to_repo_root(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    for d in dir.ancestors() {
        out.push(d.to_path_buf());
        if d.join(".git").exists() { return out; }
    }
    vec![dir.to_path_buf()] // not in git: the folder only
}

// The skills of the plugins installed for you, or for this folder, that are enabled.
fn plugin_skills(claude: &Path, dir: &Path, out: &mut Vec<Skill>, m: Mode) {
    let installed = json(&claude.join("plugins/installed_plugins.json"));
    let shared = json(&dir.join(".claude/settings.json"));
    let mut enabled = serde_json::Map::new();
    for f in [claude.join("settings.json"), dir.join(".claude/settings.json"), dir.join(".claude/settings.local.json")] {
        if let Some(m) = json(&f)["enabledPlugins"].as_object() { enabled.extend(m.clone()); }
    }
    let Some(plugins) = installed["plugins"].as_object() else { return };
    let mut keys: Vec<&String> = plugins.keys().collect();
    keys.sort();
    for key in keys {
        let here = |e: &&Value| e["scope"] == "user" || e["projectPath"].as_str().is_some_and(|p| Path::new(p) == dir);
        let Some(entry) = plugins[key].as_array().and_then(|a| a.iter().find(here)) else { continue };
        let Some(root) = entry["installPath"].as_str().map(PathBuf::from) else { continue };
        let manifest = json(&root.join(".claude-plugin/plugin.json"));
        let on = enabled.get(key.as_str()).and_then(Value::as_bool).unwrap_or(manifest["defaultEnabled"] != false);
        if !on { continue; }
        let name = manifest["name"].as_str().map(String::from).unwrap_or_else(|| key.split('@').next().unwrap_or(key).to_string());
        let extra: Vec<String> = match &manifest["skills"] { Value::String(s) => vec![s.clone()], Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(), _ => vec![] };
        let has_dir = root.join("skills").is_dir();
        let src = if entry["scope"] != "user" || shared["enabledPlugins"].get(key.as_str()).is_some() { "team" } else { "plugin" };
        let first = out.len();
        read_dir(&root.join("skills"), Some(&name), src, out, m);
        for x in &extra {
            let p = root.join(x.trim_start_matches("./"));
            if !p.starts_with(&root) || x.contains("..") { continue; } // only inside the plugin, as Claude Code allows
            if p.join("SKILL.md").is_file() { if let Some(s) = read_skill(&p, Some(&name), src, m) { push(out, s, m); } }
            else { read_dir(&p, Some(&name), src, out, m); }
        }
        if !has_dir && extra.is_empty() && root.join("SKILL.md").is_file() {
            if let Some(mut s) = read_skill(&root, None, src, m) { s.name = format!("{name}:{}", s.name); out.push(s); }
        }
        for s in &mut out[first..] { s.plugin = Some(key.clone()); }
    }
}

// The skills a Claude Code session started in `dir` can call with "/<name>", for `home` (the person's home folder).
// Precedence as Claude Code's: managed over personal over project for the same name; plugin skills are prefixed.
pub fn list(home: &Path, dir: &Path) -> Vec<Skill> { collect(home, dir, Mode { all: false }) }

// Every skill Claude loads in `dir`, with the ones only Claude may call and the shadowed duplicates (for the audit).
pub fn all(home: &Path, dir: &Path) -> Vec<Skill> { collect(home, dir, Mode { all: true }) }

fn collect(home: &Path, dir: &Path, m: Mode) -> Vec<Skill> {
    let claude = home.join(".claude");
    let mut out = vec![];
    read_dir(&managed_dir().join(".claude/skills"), None, "managed", &mut out, m);
    read_dir(&claude.join("skills"), None, "personal", &mut out, m); // skills/synced has no SKILL.md of its own
    for d in up_to_repo_root(dir) { read_dir(&d.join(".claude/skills"), None, "project", &mut out, m); }
    plugin_skills(&claude, dir, &mut out, m);
    if let Ok(rd) = fs::read_dir(claude.join("skills/synced")) {
        let mut ids: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        ids.sort();
        for id in ids { read_dir(&id, Some("anthropic-skills"), "synced", &mut out, m); }
    }
    out
}

// The repository root of `dir` (the nearest folder up holding .git), else `dir`.
pub fn repo_root(dir: &Path) -> PathBuf { up_to_repo_root(dir).pop().unwrap_or_else(|| dir.to_path_buf()) }

// Your own skills, wherever you start Claude: personal, synced from claude.ai, and plugins installed for you (for
// Settings → My skills, where you pin the ones to offer).
pub fn mine(home: &Path) -> Vec<Skill> {
    let none = home.join(".claude/no-project"); // a folder with no project skills and no project settings
    list(home, &none).into_iter().filter(|s| matches!(s.source, "personal" | "synced" | "plugin")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, text: &str) { fs::create_dir_all(p.parent().unwrap()).unwrap(); fs::write(p, text).unwrap(); }

    #[test]
    fn front_matter_values() {
        let fm = front_matter("---\nname: ak:cook\ndescription: \"Fix bugs: \\\"fast\\\"\"\nwhen_to_use: >\n  Use it\n  for bugs.\nuser-invocable: false\n---\n# Body\nname: not this\n");
        assert_eq!(fm, [("name".into(), "ak:cook".into()), ("description".into(), "Fix bugs: \"fast\"".into()), ("when_to_use".into(), "Use it for bugs.".into()), ("user-invocable".into(), "false".into())]);
        assert!(front_matter("# no front matter\nname: x").is_empty());
    }

    #[test]
    fn skills_of_a_folder_from_every_place_claude_keeps_them() {
        let t = std::env::temp_dir().join(format!("bobix-skills-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        let (home, repo) = (t.join("home"), t.join("src/app"));
        let c = home.join(".claude");
        write(&c.join("skills/plan/SKILL.md"), "---\nname: plan\ndescription: Plan a feature\n---\n");
        write(&c.join("skills/hidden/SKILL.md"), "---\nuser-invocable: false\ndescription: only Claude\n---\n");
        write(&c.join("skills/no-front/SKILL.md"), "Just text.\n");
        write(&c.join("skills/synced/abc/docx/SKILL.md"), "---\nname: docx\ndescription: Word files\n---\n");
        fs::create_dir_all(repo.join(".git")).unwrap();
        write(&repo.join(".claude/skills/fix-ticket/SKILL.md"), "---\nname: fix-ticket\ndescription: >\n  Fix one ticket\n  end to end.\n---\n");
        write(&repo.join(".claude/skills/plan/SKILL.md"), "---\nname: plan\ndescription: the repo's plan\n---\n");
        write(&t.join("src/.claude/skills/outside/SKILL.md"), "---\nname: outside\n---\n"); // above the repository root
        let p = |n: &str| t.join("plugins").join(n);
        write(&p("kit/.claude-plugin/plugin.json"), r#"{ "name": "ba-kit" }"#);
        write(&p("kit/skills/write-stories/SKILL.md"), "---\nname: write-stories\ndescription: Stories from a brief\n---\n");
        write(&p("extra/.claude-plugin/plugin.json"), r#"{ "name": "ux", "skills": ["./.claude/skills/ux"] }"#);
        write(&p("extra/.claude/skills/ux/SKILL.md"), "---\nname: ux\n---\n");
        write(&p("off/skills/x/SKILL.md"), "---\nname: x\n---\n");
        write(&p("other/skills/y/SKILL.md"), "---\nname: y\n---\n");
        let installed = serde_json::json!({ "version": 2, "plugins": {
            "kit@m": [{ "scope": "local", "projectPath": repo, "installPath": p("kit") }],
            "extra@m": [{ "scope": "user", "installPath": p("extra") }],
            "off@m": [{ "scope": "user", "installPath": p("off") }],
            "other@m": [{ "scope": "local", "projectPath": t.join("elsewhere"), "installPath": p("other") }] } });
        write(&c.join("plugins/installed_plugins.json"), &installed.to_string());
        write(&c.join("settings.json"), r#"{ "enabledPlugins": { "off@m": true } }"#);
        write(&repo.join(".claude/settings.local.json"), r#"{ "enabledPlugins": { "off@m": false } }"#);
        let got: Vec<(String, &str, String)> = list(&home, &repo).into_iter().map(|s| (s.name, s.source, s.description)).collect();
        let want: Vec<(String, &str, String)> = [("no-front", "personal", ""), ("plan", "personal", "Plan a feature"), ("fix-ticket", "project", "Fix one ticket end to end."),
            ("ux:ux", "plugin", ""), ("ba-kit:write-stories", "team", "Stories from a brief"), ("anthropic-skills:docx", "synced", "Word files")]
            .into_iter().map(|(a, b, c)| (a.to_string(), b, c.to_string())).collect();
        assert_eq!(got, want, "personal wins over project; hidden, disabled, other folders' and outside-the-repo skills left out; a plugin installed for the folder is the team's");
        let mine: Vec<String> = mine(&home).into_iter().map(|s| s.name).collect();
        assert_eq!(mine, ["no-front", "plan", "ux:ux", "off:x", "anthropic-skills:docx"], "yours only: no repository's or team's (off@m is off only in that repository)");
        let _ = fs::remove_dir_all(&t);
    }

    // Prints the counts of what this machine's Claude Code has for a folder (read-only); BOBIX_SKILLS_JSON=<file> also
    // writes the list there. BOBIX_SKILLS_DIR=/path/to/repo cargo test live_skills -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_skills() {
        let dir = std::env::var("BOBIX_SKILLS_DIR").unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").into());
        let all = list(&crate::paths::home(), Path::new(&dir));
        for src in ["managed", "personal", "project", "plugin", "synced"] { println!("{src}: {}", all.iter().filter(|s| s.source == src).count()); }
        if let Ok(f) = std::env::var("BOBIX_SKILLS_JSON") { fs::write(f, serde_json::to_string(&all).unwrap()).unwrap(); }
    }
}
