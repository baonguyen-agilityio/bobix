// Feedback (a bug or an idea): a link that opens GitHub's "new issue" form, filled in. The person reads it there and
// submits it themselves, so Bobix needs no token and no server. Issues are public: only listed, non-identifying facts
// go in. Port of src/core/feedback.js.
use crate::{js::{head16, len16}, work::enc};
use fancy_regex::{Captures, Regex};
use serde_json::Value;
use std::sync::LazyLock;

// GitHub answered a 6,500-character link and failed on 6,900 (checked by hand): stay under that.
pub const MAX_URL: usize = 6000;

// Takes what could identify someone out of an error message: home folder, folders of a path, tokens.
pub fn redact(text: &str, home: &str) -> String {
    static TOKEN_Q: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([?&]t=)[^\s&]+").unwrap());
    static KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(?:sk-[A-Za-z0-9_-]{10,}|[a-f0-9]{32,})\b").unwrap());
    static PATH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?:~|[A-Za-z]:)?(?:[\\/][^\s"'`:,()\\/]+){2,}"#).unwrap());
    let out = if home.is_empty() { text.to_string() } else { text.replace(home, "~") };
    let out = TOKEN_Q.replace_all(&out, "${1}[redacted]");
    let out = KEY.replace_all(&out, "[redacted]");
    PATH.replace_all(&out, |c: &Captures| format!(".../{}", c[0].rsplit(['\\', '/']).next().unwrap_or(""))).into_owned()
}

// How JavaScript prints a value in a template string.
fn js(v: &Value) -> String {
    match v { Value::String(s) => s.clone(), Value::Null => "null".into(), v => v.to_string() }
}
fn js_or_undef(v: Option<&Value>) -> String { v.map_or("undefined".into(), js) }
fn truthy(v: &Value) -> bool {
    match v { Value::Null => false, Value::Bool(b) => *b, Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0), Value::String(s) => !s.is_empty(), _ => true }
}

pub struct Info { pub lines: Vec<String>, pub errs: Vec<String> }

// The facts from diagnostics a report needs. A list of fields, not a list of exclusions, so that something added to
// diagnostics later stays out until it is checked. Left out on purpose: process names and paths, environment names.
pub fn summarize(d: &Value, errors: &[String], home: &str) -> Info {
    let (s, h, o) = (&d["settings"], &d["hooks"], &d["overlay"]);
    let set = |k: &str| js_or_undef(s.get(k));
    let or_dash = |v: &Value| if v.is_null() { "-".to_string() } else { js(v) };
    let list = |a: &Value, f: &dyn Fn(&Value) -> String, none: &str| {
        let v: Vec<String> = a.as_array().map(|a| a.iter().map(f).collect()).unwrap_or_default();
        if v.is_empty() { none.to_string() } else { v.join(", ") }
    };
    let lines = vec![
        format!("- Bobix {}, {} {} {}{}{}", js(&d["app"]), js(&d["platform"]), js(&d["release"]), js(&d["arch"]),
            if truthy(&d["wayland"]) { ", Wayland" } else { "" }, if truthy(&d["ozone"]) { format!(", ozone {}", js(&d["ozone"])) } else { String::new() }),
        format!("- Hooks: {}, complete: {}, stale: {}, outdated: {}, port: {}, node: {}{}", if truthy(&h["mode"]) { js(&h["mode"]) } else { "none".into() },
            truthy(&h["complete"]), truthy(&h["stale"]), truthy(&h["outdated"]), or_dash(&h["port"]), truthy(&d["node"]),
            if truthy(&h["error"]) { format!(", error: {}", redact(&js(&h["error"]), home)) } else { String::new() }),
        format!("- Displays: {}", list(&d["displays"], &|x| format!("{}x{}", js(&x["scale"]), if truthy(&x["primary"]) { " (primary)" } else { "" }), "-")),
        format!("- Pets window: ready: {}, visible: {}, always on top: {}, pets: {}", truthy(&o["ready"]), truthy(&o["visible"]), truthy(&o["alwaysOnTop"]), or_dash(&o["page"]["pets"])),
        format!("- Settings: pets visible: {}, show idle: {}, only attention: {}, sleep hide: {}, edge: {}, display: {}, click: {}",
            set("petsVisible"), set("showIdle"), set("onlyAttention"), set("sleepHide"), set("edge"), set("display"), set("clickAction")),
        format!("- Sessions: {}", list(&d["sessions"], &|x| format!("{} {}m", js(&x["state"]), js(&x["sinceMin"])), "none")),
    ];
    let errs = errors[errors.len().saturating_sub(10)..].iter().map(|e| format!("- {}", head16(&redact(e, home), 300))).collect();
    Info { lines, errs }
}

// The form's address. Ideas carry no technical details. Long bug reports lose the oldest errors first, then the end of
// the description.
pub fn issue_url(repo: &str, idea: bool, title: &str, description: &str, info: Option<&Info>) -> (String, bool) {
    let mut errs: Vec<String> = if idea { vec![] } else { info.map(|i| i.errs.clone()).unwrap_or_default() };
    let lines = info.map(|i| i.lines.clone()).unwrap_or_default();
    let mut text = description.trim().to_string();
    let mut truncated = false;
    let build = |text: &str, errs: &[String]| {
        let t = if text.is_empty() { "_(not described)_" } else { text };
        let body = if idea { format!("## The idea\n{t}") } else {
            let mut b = vec!["## What happened".to_string(), t.into(), String::new(), "## Technical details (added by Bobix)".into()];
            b.extend(lines.iter().cloned());
            if !errs.is_empty() { b.push(String::new()); b.push("## Recent errors".into()); b.extend(errs.iter().cloned()); }
            b.join("\n")
        };
        let heading = format!("[{}] {}", if idea { "Idea" } else { "Bug" }, head16(title.trim(), 200));
        format!("https://github.com/{repo}/issues/new?title={}&body={}", enc(&heading), enc(&body))
    };
    let mut url = build(&text, &errs);
    while url.len() > MAX_URL && !errs.is_empty() { errs.remove(0); truncated = true; url = build(&text, &errs); }
    while url.len() > MAX_URL && !text.is_empty() { text = head16(&text, len16(&text).saturating_sub(100)).to_string(); truncated = true; url = build(&text, &errs); }
    (url, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn decode(s: &str) -> String { percent_encoding::percent_decode_str(s).decode_utf8().unwrap().into_owned() }

    #[test]
    fn feedback_carries_no_folders_tokens_or_names_and_fits_in_a_link() {
        let home = "/Users/Jo Doe";
        assert_eq!(redact("failed /Users/Jo Doe/work/acme-secret/a.js", home), "failed .../a.js");
        assert_eq!(redact("failed /home/sam/acme/a.js", ""), "failed .../a.js");
        assert_eq!(redact("failed C:\\Users\\sam\\acme\\a.js", ""), "failed .../a.js");
        assert_eq!(redact("http://x/y?t=abc&z=1", ""), "http:/.../y?t=[redacted]&z=1");
        assert_eq!(redact(&format!("key {}", "a".repeat(48)), ""), "key [redacted]");
        let d = json!({ "app": "1.0", "platform": "linux", "release": "6", "arch": "x64", "settings": {}, "hooks": { "mode": "node", "error": "bad /Users/Jo Doe/.claude/x.json" },
            "displays": [{ "id": 9, "bounds": { "x": 1 }, "scale": 2, "primary": true }],
            "overlay": { "ready": true, "visible": true, "alwaysOnTop": true, "page": { "pets": 1, "labels": [3] }, "errors": ["secret in overlay"] },
            "sessions": [{ "state": "working", "sinceMin": 2, "title": true, "chain": ["/Users/Jo Doe/acme/run"], "env": ["ACME_SECRET"] }], "somethingNew": "never listed" });
        let info = summarize(&d, &["boom /Users/Jo Doe/acme/a.js".into()], home);
        let text = [info.lines.clone(), info.errs.clone()].concat().join("\n");
        for leak in ["Jo Doe", "acme", "ACME_SECRET", "never listed", "secret in overlay"] { assert!(!text.contains(leak), "{leak}"); }
        assert!(text.contains("Sessions: working 2m"));
        let (short, cut) = issue_url("o/r", false, "Pets & co", "hi", Some(&info));
        assert!(short.starts_with("https://github.com/o/r/issues/new?title=%5BBug%5D%20Pets%20%26%20co&body="));
        assert!(!cut);
        assert!(decode(&short).contains("## Recent errors\n- boom .../a.js"));
        let (idea, _) = issue_url("o/r", true, "Themes", "nice", None);
        assert!(decode(&idea).ends_with("title=[Idea] Themes&body=## The idea\nnice"), "an idea carries no technical details");
        let big = Info { lines: vec!["- l".into()], errs: vec!["- e".repeat(90)] };
        let (long, cut) = issue_url("o/r", false, "t", &"đ".repeat(5000), Some(&big));
        assert!(long.len() <= MAX_URL && cut, "cut to fit");
        assert!(decode(&long).contains("## Technical details"), "the details survive");
    }
}
