// User settings: defaults, validation and a JSON file in the app's data folder. Same file, keys and rules as
// the 0.1 (Electron) app's core/settings.js, so the Electron and Tauri builds read and write the same settings.json.
use crate::js::{js_number, round};
use serde_json::{json, Map, Value};
use std::{fs, path::Path};

pub const UNTIL_ANSWER: f64 = -1.0; // approveWait: no countdown, the request waits until you answer (or Claude moves on)

pub fn defaults() -> Value {
    json!({
        "language": "auto", "port": 4747, "display": "primary", "edge": "bottom", "walkDir": "ltr", "petMode": "edge", "dnd": "0", "dndUntil": 0, "zoneStart": 0, "zoneEnd": 100,
        "size": 56, "opacity": 1, "fadeNearCursor": true, "showIdle": true, "approveWait": -1, "longWait": 10, "sleepHide": 30, "stuckOn": true, "stuckNoPr": 2, "stuckNoReview": 24, "stuckBranch": 4, "stuckIdle": 4, "stuckMaxAge": 14, "stuckSkipped": {},
        "onlyAttention": false, "label": "title", "clickAction": "focus", "notifyWaiting": true, "notifyDone": true,
        "sound": true, "paused": false, "petsVisible": true, "autostart": false, "checkUpdates": true, "guideSeen": false, "tourSeen": [],
        "seenVersion": "", "gitlab": false, "github": false, "role": "", "skillPins": {}, "skillProcess": {}, "workProjects": [], "workKeys": {}, "petImages": {},
        "habitsOn": false, "habitsNo": {}, "habitsHidden": {}, "habitsSkill": {}, "habitsPetDay": "",
    })
}

fn enum_of(k: &str) -> Option<&'static [&'static str]> {
    Some(match k {
        "language" => &["auto", "en", "vi"],
        "edge" => &["bottom", "top"],
        "walkDir" => &["ltr", "rtl"], // the way the pets walk along the edge: left to right, or right to left
        "petMode" => &["edge", "ondemand"], // every pet walks along the edge, or a pet shows only while its session needs you
        "dnd" => &["0", "30", "60"], // "do not disturb": the minutes last chosen (0: off); it lasts until dndUntil
        "label" => &["title", "folder", "both"],
        "clickAction" => &["focus", "panel"],
        "role" => &["", "ba", "qa", "dev"], // what you do, to suggest a skill when you start an issue
        _ => return None,
    })
}

fn range_of(k: &str) -> Option<(f64, f64)> {
    Some(match k {
        "port" => (1024.0, 65535.0),
        "zoneStart" => (0.0, 95.0),
        "zoneEnd" => (5.0, 100.0),
        "size" => (32.0, 128.0),
        "opacity" => (0.2, 1.0),
        "sleepHide" => (0.0, 1440.0),
        "approveWait" => (0.0, 300.0),
        "longWait" => (1.0, 120.0), // minutes after which a request waiting for your answer shows as a long wait
        // stuck work, in hours: a finished session with no PR / MR, a PR / MR nobody reviewed, a branch with commits and
        // no PR / MR, an issue's sessions all resting
        "stuckNoPr" => (1.0, 72.0),
        "stuckNoReview" => (1.0, 168.0),
        "stuckBranch" => (1.0, 72.0),
        "stuckIdle" => (1.0, 72.0),
        "stuckMaxAge" => (1.0, 365.0), // days: stuck work quiet longer than this is old, not a nudge
        "dndUntil" => (0.0, 1e14), // when "do not disturb" ends, in ms since 1970 (0: off)
        _ => return None,
    })
}

fn num_value(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9e15 { json!(n as i64) } else { json!(n) }
}

fn is_hex(s: &str, n: usize) -> bool { s.len() == n && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }

// "<24 hex>.<ext>" with ext in `exts`.
fn hashed_name(s: &str, exts: &[&str]) -> bool {
    s.split_once('.').is_some_and(|(h, e)| is_hex(h, 24) && exts.contains(&e))
}
pub fn pet_file(s: &str) -> bool { hashed_name(s, &["png", "gif"]) }
fn pet_src(s: &str) -> bool { hashed_name(s, &["png", "jpg", "gif", "webp"]) }

// A GitLab project path: "group/name", segments of [\w.-], no "." or ".." segment.
fn project_path(p: &str) -> bool {
    let segs: Vec<&str> = p.split('/').collect();
    p.len() < 300 && segs.len() >= 2
        && segs.iter().all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)) && !s.bytes().all(|b| b == b'.'))
}

// A project's code in session names: [A-Z][A-Z0-9]{0,9}.
fn project_code(c: &str) -> bool {
    let b = c.as_bytes();
    !b.is_empty() && b.len() <= 10 && b[0].is_ascii_uppercase() && b.iter().all(|x| x.is_ascii_uppercase() || x.is_ascii_digit())
}

// A picture entry: a file name (made before pictures were processed) or { src, file, opts }; None when it is neither.
fn pet_entry(e: &Value) -> Option<Value> {
    match e {
        Value::String(s) => pet_file(s).then(|| e.clone()),
        Value::Object(o) => {
            let src = o.get("src")?.as_str().filter(|s| pet_src(s))?;
            let file = o.get("file")?.as_str().filter(|s| pet_file(s))?;
            Some(json!({ "src": src, "file": file, "opts": normalize_opts(o.get("opts"), false) }))
        }
        _ => None,
    }
}

// Keeps only known keys with valid values; anything else falls back to the current value.
pub fn sanitize(input: &Value, base: &Value) -> Value {
    let mut out = base.as_object().cloned().unwrap_or_default();
    let defs = defaults();
    let empty = Map::new();
    let input = input.as_object().unwrap_or(&empty);
    for (k, def) in defs.as_object().unwrap() {
        let Some(v) = input.get(k) else { continue };
        let k = k.as_str();
        if let Some(allowed) = enum_of(k) {
            if v.as_str().is_some_and(|s| allowed.contains(&s)) { out.insert(k.into(), v.clone()); }
        } else if k == "port" {
            // reject, never clamp: a guessed port could be busy
            let n = js_number(v);
            if n.fract() == 0.0 && (1024.0..=65535.0).contains(&n) { out.insert(k.into(), json!(n as i64)); }
        } else if k == "approveWait" && js_number(v) == UNTIL_ANSWER {
            out.insert(k.into(), json!(-1)); // any other negative number is clamped to 0 (off) below
        } else if let Some((lo, hi)) = range_of(k) {
            let n = js_number(v);
            if n.is_finite() {
                let n = if k == "opacity" { n } else { round(n) };
                out.insert(k.into(), num_value(n.max(lo).min(hi)));
            }
        } else if def.is_boolean() {
            if v.is_boolean() { out.insert(k.into(), v.clone()); }
        } else if k == "seenVersion" {
            if let Some(s) = v.as_str() {
                let ok = s.is_empty() || { let p: Vec<&str> = s.split('.').collect(); p.len() == 3 && p.iter().all(|x| (1..=4).contains(&x.len()) && x.bytes().all(|b| b.is_ascii_digit())) };
                if ok { out.insert(k.into(), v.clone()); }
            }
        } else if ["habitsNo", "habitsHidden", "habitsSkill"].contains(&k) {
            // habits: "Không cần" per kind (habitsNo: kind → how many times) and per suggestion (habitsHidden: id → when),
            // and the skill a repository's Start box begins with (habitsSkill: folder → skill name)
            if let Some(o) = v.as_object() {
                let ok_key = |x: &str| (1..=600).contains(&x.len()) && !x.chars().any(char::is_control);
                let m: Map<String, Value> = o.iter().filter(|(x, _)| ok_key(x)).filter_map(|(x, t)| {
                    if k == "habitsSkill" {
                        let n = t.as_str()?.trim_start_matches('/');
                        ((1..=60).contains(&n.len()) && n.chars().all(|c| c.is_ascii_alphanumeric() || "-_:.".contains(c))).then(|| (x.clone(), json!(n)))
                    } else { let n = js_number(t); (n.is_finite() && n >= 0.0).then(|| (x.clone(), num_value(round(n)))) }
                }).take(300).collect();
                out.insert(k.into(), Value::Object(m));
            }
        } else if k == "habitsPetDay" {
            if let Some(d) = v.as_str().filter(|d| d.len() == 10 && d.bytes().enumerate().all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() })) { out.insert(k.into(), json!(d)); }
        } else if k == "tourSeen" {
            // the tour's steps already shown: their keys (tour.js)
            if let Some(a) = v.as_array() {
                let mut seen: Vec<&str> = vec![];
                for x in a.iter().filter_map(|x| x.as_str()).filter(|x| (1..=24).contains(&x.len()) && x.bytes().all(|b| b.is_ascii_lowercase())) {
                    if !seen.contains(&x) { seen.push(x); }
                }
                seen.truncate(64);
                out.insert(k.into(), json!(seen));
            }
        } else if k == "stuckSkipped" {
            // stuck items you skipped: their id → the time of the last activity they had then (ms); one that moves again comes back
            if let Some(o) = v.as_object() {
                let m: Map<String, Value> = o.iter()
                    .filter(|(id, _)| (1..=600).contains(&id.len()) && !id.chars().any(char::is_control))
                    .filter_map(|(id, t)| { let n = js_number(t); (n.is_finite() && n >= 0.0).then(|| (id.clone(), num_value(round(n)))) })
                    .take(300).collect();
                out.insert(k.into(), Value::Object(m));
            }
        } else if k == "workKeys" {
            if let Some(o) = v.as_object() {
                let m: Map<String, Value> = o.iter()
                    .filter(|(p, c)| project_path(p) && c.as_str().is_some_and(project_code))
                    .take(500).map(|(p, c)| (p.to_lowercase(), c.clone())).collect();
                out.insert(k.into(), Value::Object(m));
            }
        } else if k == "skillPins" { // your own skills to offer: name -> the roles it is for (none: any role)
            if let Some(o) = v.as_object() {
                let m: Map<String, Value> = o.iter().filter(|(n, _)| crate::process::skill_name(n)).filter_map(|(n, r)| {
                    let roles: Vec<&str> = r.as_array()?.iter().filter_map(Value::as_str).filter(|x| ["ba", "qa", "dev"].contains(x)).collect();
                    let mut uniq: Vec<&str> = vec![];
                    for x in roles { if !uniq.contains(&x) { uniq.push(x); } }
                    Some((n.clone(), json!(uniq)))
                }).take(300).collect();
                out.insert(k.into(), Value::Object(m));
            }
        } else if k == "skillProcess" { // processes imported by hand, per project, checked again as process files are
            if let Some(o) = v.as_object() {
                let m: Map<String, Value> = o.iter().filter(|(p, _)| project_path(p))
                    .filter_map(|(p, x)| Some((p.to_lowercase(), crate::process::parse(&x.to_string()).ok()?))).take(50).collect();
                out.insert(k.into(), Value::Object(m));
            }
        } else if k == "workProjects" {
            if let Some(a) = v.as_array() {
                let mut seen: Vec<String> = Vec::new();
                for p in a.iter().filter_map(|p| p.as_str()).filter(|p| project_path(p)) {
                    let p = p.to_lowercase();
                    if !seen.contains(&p) { seen.push(p); }
                }
                seen.truncate(500);
                out.insert(k.into(), json!(seen));
            }
        } else if k == "display" {
            if v.as_str().is_some_and(|s| s.encode_utf16().count() < 40) { out.insert(k.into(), v.clone()); }
        } else if k == "petImages" {
            if let Some(o) = v.as_object() {
                let m: Map<String, Value> = o.iter()
                    .filter_map(|(p, e)| Some((p.clone(), pet_entry(e)?)))
                    .filter(|(p, _)| p.encode_utf16().count() < 1024).take(200).collect();
                out.insert(k.into(), Value::Object(m));
            }
        }
    }
    let (zs, ze) = (out["zoneStart"].as_f64().unwrap_or(0.0), out["zoneEnd"].as_f64().unwrap_or(100.0));
    if ze - zs < 5.0 { out.insert("zoneEnd".into(), num_value((zs + 5.0).min(100.0))); }
    Value::Object(out)
}

// How long a permission request of a session waits for an answer in Bobix: seconds, infinity (until answered) or 0
// (don't hold it, Claude asks as usual). Never holds what the person may not see: pets hidden, notifications paused,
// or this pet hidden by hand.

pub fn hold_secs(settings: &Value, pet: &str) -> f64 {
    let w = settings["approveWait"].as_f64().unwrap_or(0.0);
    if w == 0.0 || settings["paused"] == true || settings["petsVisible"] != true || pet == "hidden" { return 0.0; }
    if w == UNTIL_ANSWER { f64::INFINITY } else { w }
}

pub fn load(file: &Path) -> Value {
    let d = defaults();
    fs::read_to_string(file).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .map(|v| sanitize(&v, &d)).unwrap_or(d)
}

// Written whole to a temporary file, then renamed over the old one: a crash never leaves half a file.
pub fn save(file: &Path, settings: &Value) -> std::io::Result<()> {
    if let Some(d) = file.parent() { fs::create_dir_all(d)?; }
    let tmp = file.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(settings).unwrap_or_default())?;
    fs::rename(&tmp, file)
}

// ---------- first-run guide and "What's new" (the 0.1 (Electron) app's core/news.js) ----------
// 0.1.4-rc.3 → 0.1.4: a test build shows its release's notes
pub fn base(v: &str) -> &str { v.split('-').next().unwrap_or(v) }

// Once, the first time this version starts with these settings. `existing`: hooks were already installed, so this
// person used an older Bobix: skip the guide, keep the tour of what is new. A new install sees the guide; the tour's
// steps of today (`tour`, its step keys) count as seen, and only steps added later show.
pub fn first_start(settings: &Value, version: &str, existing: bool, tour: &[String]) -> Option<Value> {
    if settings["seenVersion"].as_str().is_some_and(|v| !v.is_empty()) { return None; }
    Some(if existing { json!({ "guideSeen": true }) } else { json!({ "seenVersion": base(version), "tourSeen": tour }) })
}

// The version whose notes the panel shows, or None.
pub fn news_for(settings: &Value, version: &str, notes: &str) -> Option<String> {
    (base(version) == notes && settings["seenVersion"] != notes).then(|| notes.to_string())
}

// ---------- pet picture options (the 0.1 (Electron) app's core/pixelate.js normalizeOpts / parseColor) ----------
const SIZES: [f64; 3] = [32.0, 48.0, 64.0];

// Untrusted options (IPC, settings.json) -> options pixelate can use. removeBg defaults to whether the picture has no
// alpha. outlineColor is left out for the automatic dark outline; it is kept while the outline is off.
pub fn normalize_opts(input: Option<&Value>, plain_bg: bool) -> Value {
    let empty = Map::new();
    let o = input.and_then(|v| v.as_object()).unwrap_or(&empty);
    let num = |k: &str| o.get(k).and_then(|v| v.as_f64()).filter(|n| n.is_finite());
    let int = |k: &str, lo: f64, hi: f64, def: f64| num(k).map(|n| round(n).max(lo).min(hi)).unwrap_or(def);
    let boolean = |k: &str, def: bool| o.get(k).and_then(|v| v.as_bool()).unwrap_or(def);
    let size = num("size").map(|n| SIZES.iter().copied().fold(SIZES[0], |a, b| if (b - n).abs() < (a - n).abs() { b } else { a })).unwrap_or(48.0);
    let colors = int("colors", 4.0, 32.0, 16.0);
    let outline = boolean("outline", true);
    let accents = int("accents", 0.0, 8.0, 3.0).min(colors - if outline { 1.0 } else { 0.0 } - 2.0);
    let mut out = json!({
        "size": num_value(size), "colors": num_value(colors), "accents": num_value(accents), "clean": boolean("clean", true),
        "outline": outline, "removeBg": boolean("removeBg", plain_bg), "whole": boolean("whole", false),
    });
    if let Some(c) = o.get("outlineColor").and_then(parse_color) { out["outlineColor"] = json!(c); }
    out
}

// A colour as a person types it -> "#rrggbb", or None: #rgb, #rgba, #rrggbb, #rrggbbaa, rgb()/rgba() and hsl()/hsla()
// with commas or spaces, numbers or percentages. Alpha must be a number but is dropped.
pub fn parse_color(v: &Value) -> Option<String> {
    let v = v.as_str()?;
    if v.encode_utf16().count() > 64 { return None; }
    let s = v.trim().to_lowercase();
    if let Some(h) = s.strip_prefix('#') {
        if matches!(h.len(), 3 | 4 | 6 | 8) && h.bytes().all(|b| b.is_ascii_hexdigit()) {
            let full: String = if h.len() < 6 { h.chars().flat_map(|c| [c, c]).collect() } else { h.into() };
            return Some(format!("#{}", &full[..6]));
        }
        return None;
    }
    let (name, rest) = s.split_once('(')?;
    let hsl = match name { "rgb" | "rgba" => false, "hsl" | "hsla" => true, _ => return None };
    let inner = rest.strip_suffix(')')?;
    if inner.contains(['(', ')']) { return None; }
    let parts: Vec<&str> = split_args(inner.trim());
    if parts.len() < 3 || parts.len() > 4 { return None; }
    let mut n = Vec::new();
    let mut unit = Vec::new();
    for p in &parts {
        let (x, u) = parse_num_unit(p)?;
        n.push(x);
        unit.push(u);
    }
    if !n.iter().all(|x| x.is_finite()) { return None; }
    let hue = |u: &str| -> Option<f64> { Some(match u { "" | "deg" => 1.0, "grad" => 0.9, "rad" => 180.0 / std::f64::consts::PI, "turn" => 360.0, _ => return None }) };
    if hsl && hue(unit[0]).is_none() { return None; }
    if unit[if hsl { 1 } else { 0 }..].iter().any(|u| !u.is_empty() && *u != "%") { return None; }
    let byte = |x: f64| round(x.max(0.0).min(255.0)) as u8;
    let rgb: Vec<u8> = if hsl {
        // CSS Color 4's hsl-to-rgb
        let h = ((n[0] * hue(unit[0]).unwrap()) % 360.0 + 360.0) % 360.0;
        let (sat, l) = (n[1].max(0.0).min(100.0) / 100.0, n[2].max(0.0).min(100.0) / 100.0);
        let a = sat * l.min(1.0 - l);
        [0.0, 8.0, 4.0].iter().map(|k| { let k = (k + h / 30.0) % 12.0; byte((l - a * (k - 3.0).min(9.0 - k).min(1.0).max(-1.0)) * 255.0) }).collect()
    } else {
        (0..3).map(|i| byte(if unit[i] == "%" { n[i] * 255.0 / 100.0 } else { n[i] })).collect()
    };
    Some(format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]))
}

// JS: str.split(/\s*[,/]\s*|\s+/)
fn split_args(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let (mut start, mut i) = (0, 0);
    while i < b.len() {
        if b[i].is_ascii_whitespace() || b[i] == b',' || b[i] == b'/' {
            let mut j = i;
            while j < b.len() && b[j].is_ascii_whitespace() { j += 1; }
            if j < b.len() && (b[j] == b',' || b[j] == b'/') {
                j += 1;
                while j < b.len() && b[j].is_ascii_whitespace() { j += 1; }
            }
            out.push(&s[start..i]);
            start = j;
            i = j;
        } else { i += 1; }
    }
    out.push(&s[start..]);
    out
}

// /^([+-]?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?)(%|deg|g?rad|turn)?$/
fn parse_num_unit(p: &str) -> Option<(f64, &str)> {
    let unit_at = p.find(|c: char| c == '%' || ('a'..='z').contains(&c) && c != 'e').unwrap_or(p.len());
    let (num, unit) = p.split_at(unit_at);
    if !matches!(unit, "" | "%" | "deg" | "rad" | "grad" | "turn") { return None; }
    let b = num.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') { i += 1; }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() { i += 1; }
    let ints = i - int_start;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let f = i;
        while i < b.len() && b[i].is_ascii_digit() { i += 1; }
        if ints == 0 && i == f { return None; }
    } else if ints == 0 { return None; }
    if i < b.len() && b[i] == b'e' {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') { i += 1; }
        let e = i;
        while i < b.len() && b[i].is_ascii_digit() { i += 1; }
        if i == e { return None; }
    }
    if i != b.len() { return None; }
    Some((num.parse::<f64>().ok()?, unit))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: Value) -> Value { sanitize(&v, &defaults()) }

    #[test]
    fn settings_sanitize() {
        let o = s(json!({ "size": 9999, "edge": "left", "opacity": 0, "zoneStart": 90, "zoneEnd": 91, "evil": 1, "sound": "yes" }));
        assert_eq!(o["size"], 128);
        assert_eq!(o["edge"], "bottom");
        assert_eq!(s(json!({ "walkDir": "up" }))["walkDir"], "ltr");
        assert_eq!(s(json!({ "walkDir": "rtl" }))["walkDir"], "rtl");
        assert_eq!(s(json!({ "petMode": "float" }))["petMode"], "edge");
        assert_eq!(s(json!({ "petMode": "ondemand" }))["petMode"], "ondemand");
        assert_eq!(s(json!({ "dnd": "45" }))["dnd"], "0");
        assert_eq!(s(json!({ "dnd": "30", "dndUntil": 1.8e12 }))["dndUntil"], 1800000000000i64);
        assert_eq!(s(json!({ "dndUntil": -5 }))["dndUntil"], 0);
        assert_eq!(o["opacity"], json!(0.2));
        assert_eq!(o["zoneEnd"], 95);
        assert!(o.get("evil").is_none());
        assert_eq!(o["sound"], true);
        assert_eq!(s(json!({ "sleepHide": "60" }))["sleepHide"], 60);
        assert_eq!(s(json!({ "sleepHide": -5 }))["sleepHide"], 0);
        assert_eq!(defaults()["habitsOn"], false, "habits: off until you turn them on");
        let h = s(json!({ "habitsOn": true, "habitsNo": { "allow": 2, "x": "y" }, "habitsHidden": { "morning:/src/a#1": 1762650000000_i64 },
            "habitsSkill": { "/src/shop": "/fix", "/src/a": "rm -rf", "/src/b": "ak:cook" }, "habitsPetDay": "2026-11-09" }));
        assert_eq!((&h["habitsOn"], &h["habitsNo"], &h["habitsSkill"], &h["habitsPetDay"]), (&json!(true), &json!({ "allow": 2 }), &json!({ "/src/shop": "fix", "/src/b": "ak:cook" }), &json!("2026-11-09")));
        assert_eq!(h["habitsHidden"], json!({ "morning:/src/a#1": 1762650000000_i64 }));
        assert_eq!(s(json!({ "habitsPetDay": "today" }))["habitsPetDay"], "");
        assert_eq!((defaults()["longWait"].clone(), s(json!({ "longWait": "25" }))["longWait"].clone(), s(json!({ "longWait": 0 }))["longWait"].clone(), s(json!({ "longWait": 999 }))["longWait"].clone()), (json!(10), json!(25), json!(1), json!(120)));
        assert_eq!([("stuckOn", json!(true)), ("stuckNoPr", json!(2)), ("stuckNoReview", json!(24)), ("stuckBranch", json!(4)), ("stuckIdle", json!(4))].map(|(k, v)| defaults()[k] == v), [true; 5]);
        assert_eq!((defaults()["stuckMaxAge"].clone(), s(json!({ "stuckMaxAge": 0 }))["stuckMaxAge"].clone(), defaults()["stuckSkipped"].clone()), (json!(14), json!(1), json!({})));
        assert_eq!(s(json!({ "stuckSkipped": { "noreview:https://h/g/a/pull/9": 1762650000000_i64, "bad": "x", "": 5, "branch:g/a:x": -1, "idle:g/a#3": "1762650000000" } }))["stuckSkipped"],
            json!({ "noreview:https://h/g/a/pull/9": 1762650000000_i64, "idle:g/a#3": 1762650000000_i64 }));
        assert_eq!(s(json!({ "stuckSkipped": [1] }))["stuckSkipped"], json!({}), "not a map: kept as it was");
        assert_eq!((s(json!({ "stuckNoReview": 500 }))["stuckNoReview"].clone(), s(json!({ "stuckIdle": "0" }))["stuckIdle"].clone(), s(json!({ "stuckOn": "no" }))["stuckOn"].clone()), (json!(168), json!(1), json!(true)));
    }

    #[test]
    fn skill_pins_and_imported_processes_are_checked() {
        let pins = s(json!({ "skillPins": { "ak:cook": ["dev", "qa", "dev", "admin"], "ak:fix": [], "bad name": ["dev"], "x;rm": [], "ok": "dev" } }));
        assert_eq!(pins["skillPins"], json!({ "ak:cook": ["dev", "qa"], "ak:fix": [] }), "skill names and roles only");
        let good = json!({ "version": 1, "rules": [{ "when": { "label": "bug" }, "skill": "fix-ticket" }] });
        let p = s(json!({ "skillProcess": { "Acme/Shop": good, "acme/bad": { "version": 1, "rules": [{ "when": { "label": "bug" }, "skill": "x y" }] }, "not a path": good } }));
        assert_eq!(p["skillProcess"].as_object().unwrap().keys().collect::<Vec<_>>(), ["acme/shop"], "only valid processes, for project paths");
        assert_eq!(p["skillProcess"]["acme/shop"]["rules"][0]["when"]["label"], json!(["bug"]));
    }

    #[test]
    fn until_answer_is_validated() {
        assert_eq!(defaults()["approveWait"], -1);
        assert_eq!(s(json!({}))["approveWait"], -1);
        assert_eq!(s(json!({ "approveWait": 45 }))["approveWait"], 45);
        assert_eq!(s(json!({ "approveWait": "-1" }))["approveWait"], -1);
        assert_eq!(s(json!({ "approveWait": -5 }))["approveWait"], 0);
        assert_eq!(s(json!({ "approveWait": 9999 }))["approveWait"], 300);
        let mut base = defaults();
        base["approveWait"] = json!(20);
        assert_eq!(sanitize(&json!({ "approveWait": "x" }), &base)["approveWait"], 20);
    }

    #[test]
    fn hold_only_where_seen() {
        let mut on = defaults();
        on["approveWait"] = json!(60);
        assert_eq!(hold_secs(&on, "auto"), 60.0);
        on["approveWait"] = json!(-1);
        assert_eq!(hold_secs(&on, "auto"), f64::INFINITY);
        assert_eq!(hold_secs(&on, "hidden"), 0.0);
        assert_eq!(hold_secs(&on, "shown"), f64::INFINITY);
        on["paused"] = json!(true);
        assert_eq!(hold_secs(&on, "auto"), 0.0);
        on["paused"] = json!(false);
        on["petsVisible"] = json!(false);
        assert_eq!(hold_secs(&on, "auto"), 0.0);
        on["approveWait"] = json!(0);
        on["petsVisible"] = json!(true);
        assert_eq!(hold_secs(&on, "auto"), 0.0);
    }

    #[test]
    fn work_settings() {
        assert_eq!(defaults()["gitlab"], false);
        assert_eq!(defaults()["github"], false);
        assert!(s(json!({ "gitlabUrl": "https://x" })).get("gitlabUrl").is_none());
        assert_eq!(s(json!({ "workProjects": ["Team/Shop-App", "team/shop-app", "g/sub/p", "nopath", "../x", 3, "a/b c"] }))["workProjects"], json!(["team/shop-app", "g/sub/p"]));
        assert_eq!(s(json!({ "workProjects": "team/x" }))["workProjects"], json!([]));
        assert_eq!(s(json!({ "workKeys": { "Team/App": "BAK", "x": "BAK", "a/b": "bad" } }))["workKeys"], json!({ "team/app": "BAK" }));
    }

    #[test]
    fn pet_images_kept_and_dropped() {
        let (a, b) = ("0123456789abcdef01234567", "fedcba9876543210fedcba98");
        let opts = normalize_opts(None, false);
        let mut big = opts.clone();
        big["size"] = json!(64);
        let good = json!({ "src": format!("{a}.webp"), "file": format!("{b}.gif"), "opts": big });
        let out = s(json!({ "petImages": {
            "/legacy": format!("{a}.png"), "/new": good,
            "/badname": "../../etc/passwd", "/badfile": { "src": format!("{a}.png"), "file": "../x.png", "opts": opts },
            "/badsrc": { "src": format!("{a}.bmp"), "file": format!("{b}.png"), "opts": opts },
            "/traversal": { "src": format!("../{a}.png"), "file": format!("{b}.png"), "opts": opts }, "/nums": 5, "/null": null,
            "/gifresult-as-jpg": { "src": format!("{a}.png"), "file": format!("{b}.jpg"), "opts": opts },
        } }))["petImages"].clone();
        let mut keys: Vec<&String> = out.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(keys, ["/legacy", "/new"]);
        assert_eq!(out["/new"], good);
        let e = s(json!({ "petImages": { "/p": { "src": format!("{a}.png"), "file": format!("{a}.png"), "opts": { "size": 9999, "colors": 1000, "accents": -4, "clean": "yes", "outline": 0, "removeBg": 1, "whole": 1 } } } }));
        assert_eq!(e["petImages"]["/p"]["opts"], json!({ "size": 64, "colors": 32, "accents": 0, "clean": true, "outline": true, "removeBg": false, "whole": false }));
    }

    #[test]
    fn normalize_opts_clamps() {
        let d = json!({ "size": 48, "colors": 16, "accents": 3, "clean": true, "outline": true, "removeBg": false, "whole": false });
        assert_eq!(normalize_opts(None, false), d);
        assert_eq!(normalize_opts(Some(&json!({})), true)["removeBg"], true);
        assert_eq!(normalize_opts(Some(&json!({ "removeBg": false })), true)["removeBg"], false);
        assert_eq!(normalize_opts(Some(&json!({ "size": 41 })), false)["size"], 48);
        assert_eq!(normalize_opts(Some(&json!({ "size": "64" })), false)["size"], 48);
        assert_eq!(normalize_opts(Some(&json!({ "colors": 4, "accents": 8 })), false)["accents"], 1);
        assert_eq!(normalize_opts(Some(&json!({ "colors": 4, "accents": 8, "outline": false })), false)["accents"], 2);
    }

    #[test]
    fn outline_colours() {
        let good = [
            ("#f0a", "#ff00aa"), ("#F0A8", "#ff00aa"), ("#ff00AA", "#ff00aa"), ("#ff00aa80", "#ff00aa"), ("  #AbC  ", "#aabbcc"),
            ("rgb(255, 0, 128)", "#ff0080"), ("rgb(255 0 128)", "#ff0080"), ("RGBA(255,0,128,0.5)", "#ff0080"), ("rgb(255 0 128 / 50%)", "#ff0080"),
            ("rgb(100% 0% 50%)", "#ff0080"), ("rgb(300 -5 12.6)", "#ff000d"),
            ("hsl(200, 80%, 40%)", "#1481b8"), ("hsl(200 80% 40%)", "#1481b8"), ("hsla(200deg, 80%, 40%, .3)", "#1481b8"), ("hsl(200 80 40)", "#1481b8"),
            ("hsl(0.5turn 100% 50%)", "#00ffff"), ("hsl(-120 100% 50%)", "#0000ff"), ("hsl(400grad 100% 50% / 0)", "#ff0000"), ("hsl(0 0% 100%)", "#ffffff"),
        ];
        for (v, want) in good { assert_eq!(parse_color(&json!(v)).as_deref(), Some(want), "{v}"); }
        let long = format!("#{}", "a".repeat(70));
        let bad = ["red", "#ff", "#ggg", "f0a", "#f0a8c", "rgb(1 2)", "rgb(1 2 3 4 5)", "rgb(1,,2,3)", "rgb(1 2 3x)", "rgb(10deg 0 0)",
            "hsl(10% 50% 50%)", "rgb(1e999 0 0)", "rgb((1 2 3))", "rgb(1 2 3) x", "cmyk(1 2 3)", &long, ""];
        for v in bad { assert_eq!(parse_color(&json!(v)), None, "{v}"); }
        assert_eq!(parse_color(&json!(5)), None);
        assert!(normalize_opts(Some(&json!({})), false).get("outlineColor").is_none());
        let off = normalize_opts(Some(&json!({ "outline": false, "outlineColor": "rgb(255 0 128)" })), false);
        assert_eq!(off["outlineColor"], "#ff0080");
        assert_eq!(normalize_opts(Some(&off), false), off);
    }

    #[test]
    fn whats_new_and_the_guide() {
        let s = |v: &str| serde_json::json!({ "seenVersion": v });
        let tour = ["pets".to_string(), "board".to_string()];
        assert_eq!(super::first_start(&s(""), "0.1.4-rc.3", false, &tour), Some(serde_json::json!({ "seenVersion": "0.1.4", "tourSeen": ["pets", "board"] })), "a new install: today's steps count as seen");
        assert_eq!(super::first_start(&s(""), "0.1.4", true, &tour), Some(serde_json::json!({ "guideSeen": true })));
        assert_eq!(super::first_start(&s("0.1.3"), "0.1.4", true, &tour), None);
        let t = |v: serde_json::Value| super::sanitize(&serde_json::json!({ "tourSeen": v }), &super::defaults())["tourSeen"].clone();
        assert_eq!(t(serde_json::json!(["pets", "board", "pets", "Bad", "", 3, "stuck"])), serde_json::json!(["pets", "board", "stuck"]));
        assert_eq!(t(serde_json::json!("x")), serde_json::json!([]));
        assert!(crate::i18n::TOUR.len() >= 8 && crate::i18n::TOUR[0] == "pets", "{:?}", *crate::i18n::TOUR);
        assert_eq!(super::news_for(&s("0.1.3"), "0.1.4-rc.1", "0.1.4").as_deref(), Some("0.1.4"));
        assert_eq!(super::news_for(&s("0.1.4"), "0.1.4", "0.1.4"), None);
        assert_eq!(super::news_for(&s("0.1.3"), "0.1.5", "0.1.4"), None);
        let f = std::env::temp_dir().join(format!("bobix-settings-{}/settings.json", std::process::id()));
        let d = super::defaults();
        super::save(&f, &d).unwrap();
        assert_eq!(super::load(&f), d);
        assert!(std::fs::read_to_string(&f).unwrap().starts_with("{\n  \"language\": \"auto\","), "JSON.stringify(settings, null, 2)");
        let _ = std::fs::remove_dir_all(f.parent().unwrap());
    }
}
