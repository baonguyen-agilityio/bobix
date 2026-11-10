// The war room: one room per problem, hosted by the machine that made it, joined by the people it invites over the
// company tailnet. People chat freely; at the end the host's Claude arranges the chat into the frame (problem → options
// → pros and cons → decision), the host edits that draft and decides. No sync between peers, no session data.
//   host     the room lives in <data>/rooms/<id>/room.json on the creator's machine. Its server is separate from the
//            panel's (which stays on 127.0.0.1 with its token): its own port (BOBIX_ROOM_PORT, else 4748), bound only
//            to this machine's tailnet IPv4, running while the host has a room still open.
//   people   a request is answered only when it comes from a tailnet address, whose `tailscale whois` login the host
//            invited to that room. No password, no account of Bobix's own. Each join, each version someone saw, each
//            message, each record saved and each refusal goes to <data>/rooms/<id>/audit.jsonl.
//   content  only what someone types or pastes into the room: a message is { text }, nothing else, with common secret
//            shapes replaced by "[redacted]" before it leaves the sender and again when the host stores it.
//   AI       one summarizer, the host's own `claude -p`, over the room's (redacted) chat only: it drafts the frame.
//   record   once the host decides, a self-contained HTML decision record is written on the host, and each guest writes
//            the same record from the room's data (never HTML from the host), in <data>/rooms/records/. "Copy for
//            Claude" (host only, after a warning) copies its text: headless `claude -p` can't publish a claude.ai artifact.
use axum::{body::to_bytes, extract::{ConnectInfo, Request, State}, http::{header, HeaderValue, Method, StatusCode}, response::{IntoResponse, Response}, Router};
use serde_json::{json, Value};
use std::{fs, io::{Read, Write}, net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream}, path::{Path, PathBuf}, process::Command,
    sync::{Arc, Mutex, OnceLock}, time::Duration};

pub const DEFAULT_PORT: u16 = 4748;
const MAX_TEXT: usize = 4000;
const MAX_BODY: usize = 16 * 1024;
const MAX_MSGS: usize = 1000;

pub fn port() -> u16 { std::env::var("BOBIX_ROOM_PORT").ok().and_then(|p| p.parse().ok()).filter(|p| *p >= 1024).unwrap_or(DEFAULT_PORT) }
fn err(e: impl ToString) -> Value { json!({ "ok": false, "error": e.to_string() }) }
pub fn now_ms() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0) }

// ---------- the tailnet ----------

// Tailscale's addresses: 100.64.0.0/10 and fd7a:115c:a1e0::/48.
pub fn is_tailnet(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => { let o = v.octets(); o[0] == 100 && (o[1] & 0xC0) == 64 }
        IpAddr::V6(v) => { let s = v.segments(); s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Who { pub login: String, pub name: String, pub node: String }
impl Who { fn json(&self) -> Value { json!({ "login": self.login, "name": self.name, "node": self.node }) } }

// `tailscale whois --json` → the person. A tagged device (a server, no person) is nobody.
pub fn parse_whois(text: &str) -> Option<Who> {
    let v: Value = serde_json::from_str(text).ok()?;
    let login = v["UserProfile"]["LoginName"].as_str()?.trim().to_lowercase();
    if login.is_empty() || login == "tagged-devices" { return None; }
    let name = v["UserProfile"]["DisplayName"].as_str().unwrap_or(&login).to_string();
    let node = v["Node"]["ComputedName"].as_str().or(v["Node"]["Name"].as_str()).unwrap_or("").split('.').next().unwrap_or("").to_string();
    Some(Who { login, name, node })
}
pub fn whois(bin: &str, addr: &str) -> Option<Who> {
    let out = Command::new(bin).args(["whois", "--json", addr]).stdin(std::process::Stdio::null()).output().ok()?;
    out.status.success().then(|| parse_whois(&String::from_utf8_lossy(&out.stdout))).flatten()
}
// This machine's tailnet IPv4 (`tailscale ip -4`), None when Tailscale is off.
pub fn my_ip(bin: &str) -> Option<Ipv4Addr> {
    let out = Command::new(bin).args(["ip", "-4"]).stdin(std::process::Stdio::null()).output().ok()?;
    String::from_utf8_lossy(&out.stdout).lines().find_map(|l| l.trim().parse::<Ipv4Addr>().ok()).filter(|ip| is_tailnet(IpAddr::V4(*ip)))
}

// ---------- secrets ----------

// ponytail: known token shapes and "key = value" assignments, not a full secret scanner; add shapes as they show up.
fn patterns() -> &'static [(fancy_regex::Regex, &'static str)] {
    static P: OnceLock<Vec<(fancy_regex::Regex, &'static str)>> = OnceLock::new();
    P.get_or_init(|| [
        (r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?(?:-----END [A-Z ]*PRIVATE KEY-----|$)", "[redacted]"),
        (r"\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|glpat-[A-Za-z0-9_\-]{20,}|xox[abprs]-[A-Za-z0-9\-]{10,}|sk-(?:ant-)?[A-Za-z0-9_\-]{20,}|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_\-]{35})", "[redacted]"),
        (r"\beyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}", "[redacted]"),
        (r"(?i)\b(bearer|basic)\s+[A-Za-z0-9._\-~+/]{16,}=*", "$1 [redacted]"),
        (r"(?i)\b([a-z][a-z0-9+.\-]*://)[^\s/:@]+:[^\s/@]+@", "$1[redacted]@"),
        (r#"(?i)\b(password|passwd|pwd|secret|token|api[_-]?key|access[_-]?key|client[_-]?secret|private[_-]?key)(["']?\s*[:=]\s*)(["']?)[^\s"',;]{4,}"#, "$1$2$3[redacted]"),
    ].into_iter().map(|(p, r)| (fancy_regex::Regex::new(p).expect("redaction pattern"), r)).collect())
}
// The text with secrets replaced, cut at MAX_TEXT characters, and how many were replaced.
pub fn redact(text: &str) -> (String, usize) {
    let mut s: String = text.chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t').take(MAX_TEXT).collect();
    let mut n = 0;
    for (re, rep) in patterns() {
        n += re.find_iter(&s).filter(|m| m.is_ok()).count();
        s = re.replace_all(&s, *rep).into_owned();
    }
    (s.trim().to_string(), n)
}

// ---------- the host's rooms ----------

static LOCK: Mutex<()> = Mutex::new(());
fn rooms(data: &Path) -> PathBuf { data.join("rooms") }
fn good_id(id: &str) -> bool { id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) }
fn room_file(data: &Path, id: &str) -> PathBuf { rooms(data).join(id).join("room.json") }
pub fn audit_file(data: &Path, id: &str) -> PathBuf { rooms(data).join(id).join("audit.jsonl") }
pub fn records(data: &Path) -> PathBuf { rooms(data).join("records") }
fn read_json(p: &Path) -> Option<Value> { serde_json::from_slice(&fs::read(p).ok()?).ok() }
pub fn load(data: &Path, id: &str) -> Option<Value> { if good_id(id) { read_json(&room_file(data, id)) } else { None } }
fn save(data: &Path, r: &Value) -> Result<(), String> { crate::settings::save(&room_file(data, r["id"].as_str().unwrap_or("")), r).map_err(|e| e.to_string()) }
fn new_id() -> String { let mut b = [0u8; 8]; let _ = getrandom::fill(&mut b); b.iter().map(|x| format!("{x:02x}")).collect() }
fn one_line(s: &str, max: usize) -> String { s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(max).collect() }
fn login_ok(s: &str) -> bool { s.len() <= 200 && s.contains('@') && !s.chars().any(|c| c.is_whitespace() || c.is_control()) }

pub fn audit(data: &Path, id: &str, ev: &str, who: Option<&Who>, ip: Option<IpAddr>, more: Value) {
    let mut line = json!({ "t": now_ms(), "ev": ev, "who": who.map(|w| w.login.clone()), "node": who.map(|w| w.node.clone()), "ip": ip.map(|i| i.to_string()) });
    if let Value::Object(m) = more { for (k, v) in m { line[k] = v; } }
    let f = audit_file(data, id);
    let _ = fs::create_dir_all(f.parent().unwrap());
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(f) { let _ = writeln!(f, "{line}"); }
}

pub fn create(data: &Path, me: &Who, title: &str, problem: &str, invite: &[String], now: u64) -> Value {
    let title = one_line(&redact(title).0, 120);
    if title.is_empty() { return err("a room needs a title"); }
    let invited: Vec<String> = invite.iter().map(|l| l.trim().to_lowercase()).filter(|l| login_ok(l) && *l != me.login).collect();
    let r = json!({ "id": new_id(), "title": title, "problem": redact(problem).0, "host": me.json(), "invited": invited, "created": now,
        "state": "open", "v": 1, "msgs": [], "draft": null, "decision": null, "seen": {} });
    let _l = LOCK.lock().unwrap();
    match save(data, &r) { Ok(()) => { audit(data, r["id"].as_str().unwrap(), "create", Some(me), None, json!({ "invited": r["invited"] })); json!({ "ok": true, "room": r }) } Err(e) => err(e) }
}

pub fn hosted(data: &Path) -> Vec<Value> {
    let mut v: Vec<Value> = fs::read_dir(rooms(data)).into_iter().flatten().flatten().filter_map(|e| load(data, &e.file_name().to_string_lossy())).collect();
    v.sort_by_key(|r| std::cmp::Reverse(r["created"].as_u64().unwrap_or(0)));
    v
}
pub fn any_open(data: &Path) -> bool { hosted(data).iter().any(|r| r["state"] == "open") }

// A change by the host to an open room: the change, then a new version.
fn change(data: &Path, id: &str, f: impl FnOnce(&mut Value) -> Result<(), String>) -> Value {
    let _l = LOCK.lock().unwrap();
    let Some(mut r) = load(data, id) else { return err("no such room") };
    if r["state"] != "open" { return err("the room is decided"); }
    if let Err(e) = f(&mut r) { return err(e); }
    r["v"] = json!(r["v"].as_u64().unwrap_or(0) + 1);
    match save(data, &r) { Ok(()) => json!({ "ok": true, "room": r }), Err(e) => err(e) }
}

pub fn invite(data: &Path, id: &str, login: &str, on: bool) -> Value {
    let login = login.trim().to_lowercase();
    if !login_ok(&login) { return err("a Tailscale login looks like name@example.com"); }
    let r = change(data, id, |r| {
        if r["host"]["login"] == login.as_str() { return Err("that is you".into()); }
        let mut l: Vec<Value> = r["invited"].as_array().cloned().unwrap_or_default();
        l.retain(|x| x != login.as_str());
        if on { l.push(json!(login)); }
        r["invited"] = json!(l);
        Ok(())
    });
    if r["ok"] == true { audit(data, id, if on { "invite" } else { "uninvite" }, None, None, json!({ "login": login })); }
    r
}
// A message in the chat, from a guest (through the room's server) or the host. Redacted here too, whatever the sender did.
pub fn say(data: &Path, id: &str, who: &Who, text: &str, now: u64) -> Value {
    let (text, n) = redact(text);
    if text.is_empty() { return err("empty"); }
    change(data, id, |r| {
        let msgs = r["msgs"].as_array_mut().ok_or("bad room")?;
        if msgs.len() >= MAX_MSGS { return Err("the room is full".into()); }
        let m = json!({ "id": format!("m{}", msgs.len() + 1), "text": text, "by": who.json(), "at": now, "redacted": n });
        msgs.push(m);
        Ok(())
    })
}

// What a guest sees: everything but who saw which version.
pub fn public(r: &Value) -> Value { let mut v = r.clone(); if let Some(o) = v.as_object_mut() { o.remove("seen"); } v }

fn time(ms: u64) -> String { let s = (ms / 1000) % 86_400; format!("{} {:02}:{:02} UTC", day(ms), s / 3600, s / 60 % 60) }
fn strs(v: &Value) -> Vec<String> { v.as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect() }

// The chat as the summarizer reads it: name, time and text of each message (already redacted).
pub fn transcript(r: &Value) -> String {
    let mut s = format!("Room: {}\nProblem as the host framed it: {}\n\nChat:\n", r["title"].as_str().unwrap_or(""), r["problem"].as_str().unwrap_or(""));
    for m in r["msgs"].as_array().into_iter().flatten() {
        s += &format!("[{}] {}: {}\n", time(m["at"].as_u64().unwrap_or(0)), m["by"]["name"].as_str().unwrap_or("?"), m["text"].as_str().unwrap_or(""));
    }
    s
}

// A draft of the frame, checked: { problem, options: [{ id, text, pros, cons }], decision: { option, why } }, every string
// redacted and cut, at most 12 options of 20 pros and cons each. The decision may be empty until the host picks one.
pub fn clean_draft(d: &Value) -> Result<Value, String> {
    let cut = |v: &Value, max: usize| -> String { redact(v.as_str().unwrap_or("")).0.chars().take(max).collect() };
    let list = |v: &Value| -> Vec<String> { v.as_array().into_iter().flatten().map(|x| cut(x, 500)).filter(|x| !x.is_empty()).take(20).collect() };
    let mut opts = vec![];
    for (i, o) in d["options"].as_array().ok_or("a draft needs options")?.iter().take(12).enumerate() {
        let text = one_line(&cut(&o["text"], 200), 200);
        if text.is_empty() { continue; }
        let id = o["id"].as_str().filter(|x| x.len() <= 8 && x.chars().all(|c| c.is_ascii_alphanumeric())).map(String::from).unwrap_or_else(|| format!("o{}", i + 1));
        opts.push(json!({ "id": id, "text": text, "pros": list(&o["pros"]), "cons": list(&o["cons"]) }));
    }
    let ids: Vec<&str> = opts.iter().filter_map(|o| o["id"].as_str()).collect();
    let mut seen = std::collections::HashSet::new();
    if !ids.iter().all(|i| seen.insert(*i)) { return Err("two options have the same id".into()); }
    let pick = d["decision"]["option"].as_str().filter(|o| ids.contains(o));
    Ok(json!({ "problem": cut(&d["problem"], MAX_TEXT), "options": opts, "decision": { "option": pick, "why": cut(&d["decision"]["why"], MAX_TEXT) } }))
}

// Claude's answer → a draft. Anything around the JSON is ignored; the option it leans to is only a suggestion.
pub fn parse_draft(answer: &str) -> Result<Value, String> {
    let (a, b) = (answer.find('{').ok_or("no JSON in the answer")?, answer.rfind('}').ok_or("no JSON in the answer")?);
    let v: Value = serde_json::from_str(answer.get(a..=b).unwrap_or("")).map_err(|e| format!("not JSON ({e})"))?;
    let mut v = v;
    if let Some(opts) = v["options"].as_array_mut() { for (i, o) in opts.iter_mut().enumerate() { o["id"] = json!(format!("o{}", i + 1)); } }
    let lean = v["lean"].as_u64().map(|n| format!("o{n}"));
    v["decision"] = json!({ "option": lean, "why": v["why"] });
    clean_draft(&v)
}

// The host's Claude arranges the chat into the frame. The host edits the draft before deciding.
pub fn draft(data: &Path, id: &str, run: crate::habit_skills::Run) -> Value {
    let Some(r) = load(data, id) else { return err("no such room") };
    if !r["msgs"].as_array().is_some_and(|a| !a.is_empty()) { return err("nothing to draft from yet"); }
    let prompt = format!("You turn a team's chat into a decision record. Below is the whole chat of the room (secrets already removed). \
        Arrange it into this frame, using only what the chat says, in the chat's language:\n\
        {{\"problem\":\"the problem in one or two sentences\",\"options\":[{{\"text\":\"short name of an option\",\"pros\":[\"…\"],\"cons\":[\"…\"]}}],\
        \"lean\":<the number (1-based) of the option the chat leans to, or null>,\"why\":\"one or two sentences: why that option, as argued in the chat\"}}\n\
        Answer with that JSON only. Keep each pro and con short. Do not invent options or arguments nobody made.\n\n{}", transcript(&r));
    let d = match run("sonnet", "", &prompt).and_then(|a| parse_draft(&a)) { Ok(d) => d, Err(e) => return err(e) };
    change(data, id, |r| { let mut d = d; d["by"] = json!("ai"); d["at"] = json!(now_ms()); r["draft"] = d; Ok(()) })
}

// The host's own edits of the draft.
pub fn save_draft(data: &Path, id: &str, d: &Value) -> Value {
    let d = match clean_draft(d) { Ok(d) => d, Err(e) => return err(e) };
    change(data, id, |r| { let mut d = d; d["by"] = json!("host"); d["at"] = json!(now_ms()); r["draft"] = d; Ok(()) })
}

// The host decides on the draft as edited: the room closes and its record is written.
pub fn decide(data: &Path, id: &str, d: &Value, now: u64) -> Value {
    let d = match clean_draft(d) { Ok(d) => d, Err(e) => return err(e) };
    let Some(opt) = d["decision"]["option"].as_str().map(String::from) else { return err("pick the option you decide on") };
    let r = change(data, id, |r| {
        let mut dd = d.clone();
        dd["by"] = json!("host");
        dd["at"] = json!(now);
        r["draft"] = dd;
        r["decision"] = json!({ "option": opt, "text": d["decision"]["why"], "by": r["host"].clone(), "at": now });
        Ok(())
    });
    if r["ok"] != true { return r; }
    let _l = LOCK.lock().unwrap();
    let Some(mut r) = load(data, id) else { return err("no such room") };
    r["state"] = json!("decided");
    if let Err(e) = save(data, &r) { return err(e); }
    audit(data, id, "decide", None, None, json!({ "option": opt }));
    match save_record(data, &r) { Ok(p) => json!({ "ok": true, "room": r, "record": p }), Err(e) => err(e) }
}

// The record as Markdown, for "Copy for Claude".
pub fn markdown(r: &Value) -> String {
    let d = &r["draft"];
    let mut s = format!("# {}\n\n## Problem\n{}\n\n## Options\n", r["title"].as_str().unwrap_or(""), d["problem"].as_str().or(r["problem"].as_str()).unwrap_or(""));
    for o in d["options"].as_array().into_iter().flatten() {
        let chosen = r["decision"]["option"] == o["id"];
        s += &format!("\n### {}{}\n", o["text"].as_str().unwrap_or(""), if chosen { " (chosen)" } else { "" });
        for p in strs(&o["pros"]) { s += &format!("- pro: {p}\n"); }
        for c in strs(&o["cons"]) { s += &format!("- con: {c}\n"); }
    }
    if r["decision"].is_object() {
        s += &format!("\n## Decision\n{}\n\nDecided by {} on {}.\n", r["decision"]["text"].as_str().unwrap_or(""), r["decision"]["by"]["name"].as_str().unwrap_or(""), time(r["decision"]["at"].as_u64().unwrap_or(0)));
    }
    s
}

// ---------- the decision record ----------

fn esc(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;") }
fn day(ms: u64) -> String { crate::habits::day_of(ms) }

// A self-contained page: no script, no outside file, light and dark. The chat is folded in at the end.
pub fn record_html(r: &Value) -> String {
    let d = &r["draft"];
    let t = |v: &Value| esc(v.as_str().unwrap_or(""));
    let chosen = r["decision"]["option"].clone();
    let li = |v: &Value| strs(v).iter().map(|x| format!("<li>{}</li>", esc(x))).collect::<String>();
    let opts: String = d["options"].as_array().into_iter().flatten().map(|o| {
        let c = o["id"] == chosen;
        format!("<section class=\"opt{}\"><h3>{}{}</h3><div class=pc><div><h4>Pros</h4><ul>{}</ul></div><div><h4>Cons</h4><ul>{}</ul></div></div></section>",
            if c { " chosen" } else { "" }, t(&o["text"]), if c { " <span class=badge>Chosen</span>" } else { "" }, li(&o["pros"]), li(&o["cons"]))
    }).collect();
    let decided = d["options"].as_array().into_iter().flatten().find(|o| o["id"] == chosen).map(|o| t(&o["text"])).unwrap_or_default();
    let msgs = r["msgs"].as_array().cloned().unwrap_or_default();
    let mut people: Vec<String> = vec![t(&r["host"]["name"]) + " (host)"];
    for m in &msgs { let n = t(&m["by"]["name"]); if !people.iter().any(|p| p == &n || p.starts_with(&format!("{n} ("))) { people.push(n); } }
    let chat: String = msgs.iter().map(|m| format!("<li><b>{}</b> <span class=meta>{}</span><br>{}</li>", t(&m["by"]["name"]), time(m["at"].as_u64().unwrap_or(0)), t(&m["text"]).replace('\n', "<br>"))).collect();
    let problem = d["problem"].as_str().filter(|p| !p.is_empty()).or(r["problem"].as_str()).unwrap_or("");
    format!(r#"<!doctype html><html lang=en><head><meta charset=utf-8><meta name=viewport content="width=device-width,initial-scale=1"><title>{title} · decision record</title>
<style>:root{{--bg:#faf9f7;--card:#fff;--ink:#1f1d1a;--ink2:#5f5a53;--line:#e6e2dc;--ok:#15803d;--okbg:#e9f6ee}}
@media (prefers-color-scheme:dark){{:root{{--bg:#16150f;--card:#1f1e19;--ink:#efece6;--ink2:#a8a299;--line:#34322c;--ok:#4ade80;--okbg:#17301f}}}}
*{{box-sizing:border-box}}body{{margin:0;background:var(--bg);color:var(--ink);font:15px/1.55 -apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif}}
main{{max-width:820px;margin:0 auto;padding:40px 20px 64px}}h1{{font-size:28px;margin:0 0 4px}}h2{{font-size:17px;margin:32px 0 10px}}h3{{font-size:16px;margin:0}}h4{{font-size:12px;text-transform:uppercase;letter-spacing:.05em;color:var(--ink2);margin:10px 0 4px}}
.meta{{color:var(--ink2);font-size:13px}}.decision{{border:1px solid var(--ok);background:var(--okbg);border-radius:12px;padding:16px 18px}}.decision b{{font-size:18px}}
.opt{{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:14px 16px;margin:0 0 10px}}.opt.chosen{{border-color:var(--ok)}}.badge{{font-size:12px;color:var(--ok);border:1px solid var(--ok);border-radius:99px;padding:1px 8px;margin-left:6px;vertical-align:2px}}
.pc{{display:grid;grid-template-columns:1fr 1fr;gap:16px}}ul{{margin:0;padding-left:18px}}.problem{{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:14px 16px}}
details{{margin-top:32px}}summary{{cursor:pointer;font-weight:600}}.chat{{list-style:none;padding:0}}.chat li{{padding:8px 0;border-bottom:1px solid var(--line)}}
footer{{margin-top:40px;color:var(--ink2);font-size:12px;border-top:1px solid var(--line);padding-top:12px}}@media (max-width:600px){{.pc{{grid-template-columns:1fr}}}}</style></head>
<body><main><p class=meta>Decision record · {created}</p><h1>{title}</h1><p class=meta>Hosted by {host} · {people}</p>
<h2>Decision</h2><div class=decision><b>{decided}</b><p>{why}</p><p class=meta>Decided by {by} · {at}</p></div>
<h2>Problem</h2><div class=problem>{problem}</div><h2>Options</h2>{opts}
<details><summary>The discussion ({n} messages)</summary><ul class=chat>{chat}</ul></details>
<footer>Made in a Bobix war room over the team's tailnet. The host's Claude arranged the chat into this frame and the host edited it. Everything here was typed by the people named; common secret shapes were removed before it was shared.</footer></main></body></html>
"#, title = t(&r["title"]), created = day(r["created"].as_u64().unwrap_or(0)), host = t(&r["host"]["name"]), people = people.join(", "), decided = decided,
        why = t(&r["decision"]["text"]).replace('\n', "<br>"), by = t(&r["decision"]["by"]["name"]), at = time(r["decision"]["at"].as_u64().unwrap_or(0)),
        problem = esc(problem).replace('\n', "<br>"), opts = opts, n = msgs.len(), chat = chat)
}
fn slug(s: &str) -> String { s.to_lowercase().split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join("-").chars().take(48).collect() }
pub fn record_path(data: &Path, r: &Value) -> PathBuf { records(data).join(format!("{}-{}.html", slug(r["title"].as_str().unwrap_or("room")), r["id"].as_str().unwrap_or(""))) }
pub fn save_record(data: &Path, r: &Value) -> std::io::Result<PathBuf> {
    let p = record_path(data, r);
    fs::create_dir_all(records(data))?;
    fs::write(&p, record_html(r))?;
    Ok(p)
}

// ---------- the room's server (host) ----------

// `peer_ok` is is_tailnet in the app; only the two-process test swaps in loopback (macOS's firewall blocks an unsigned
// test binary listening on the tailnet address).
pub struct Host { pub data: PathBuf, pub host: String, pub whois: Box<dyn Fn(&str) -> Option<Who> + Send + Sync>, pub peer_ok: fn(IpAddr) -> bool }

// Who may see the room: a tailnet address whose person the host invited.
pub fn admit(r: &Value, tailnet: bool, who: Option<&Who>) -> Result<Who, &'static str> {
    if !tailnet { return Err("not a tailnet address"); }
    let w = who.ok_or("unknown on the tailnet")?;
    let invited = r["invited"].as_array().into_iter().flatten().any(|l| l.as_str() == Some(w.login.as_str()));
    if invited { Ok(w.clone()) } else { Err("not invited") }
}

fn reply(code: StatusCode, v: Value) -> Response {
    let mut r = (code, v.to_string()).into_response();
    r.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

async fn handle(State(h): State<Arc<Host>>, ConnectInfo(peer): ConnectInfo<SocketAddr>, req: Request) -> Response {
    if !(h.peer_ok)(peer.ip()) { return reply(StatusCode::FORBIDDEN, err("not a tailnet address")); }
    // A web page can't send this header cross-origin without a preflight that is never answered; the Host check stops DNS rebinding.
    let hd = req.headers();
    if hd.get("x-bobix-room").and_then(|v| v.to_str().ok()) != Some("1") || hd.get(header::HOST).and_then(|v| v.to_str().ok()) != Some(h.host.as_str()) {
        return reply(StatusCode::FORBIDDEN, err("not a Bobix room request"));
    }
    let (method, path) = (req.method().clone(), req.uri().path().to_string());
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let (id, what) = match parts.as_slice() { ["r", id] => (id.to_string(), ""), ["r", id, w] => (id.to_string(), *w), _ => return reply(StatusCode::NOT_FOUND, err("not found")) };
    let Some(room) = load(&h.data, &id) else { return reply(StatusCode::NOT_FOUND, err("no such room")) };
    let hh = h.clone();
    let who = tokio::task::spawn_blocking(move || (hh.whois)(&peer.to_string())).await.ok().flatten();
    let w = match admit(&room, (h.peer_ok)(peer.ip()), who.as_ref()) {
        Ok(w) => w,
        Err(why) => { audit(&h.data, &id, "refused", who.as_ref(), Some(peer.ip()), json!({ "why": why })); return reply(StatusCode::FORBIDDEN, err(why)); }
    };
    match (method, what) {
        (Method::GET, "") => {
            let _l = LOCK.lock().unwrap();
            let mut r = load(&h.data, &id).unwrap_or(room);
            let v = r["v"].clone();
            if r["seen"][&w.login] != v {
                audit(&h.data, &id, if r["seen"][&w.login].is_null() { "join" } else { "saw" }, Some(&w), Some(peer.ip()), json!({ "v": v }));
                r["seen"][&w.login] = v;
                let _ = save(&h.data, &r);
            }
            reply(StatusCode::OK, json!({ "ok": true, "room": public(&r), "you": w.json() }))
        }
        (Method::POST, "say") => {
            let Ok(b) = to_bytes(req.into_body(), MAX_BODY).await else { return reply(StatusCode::PAYLOAD_TOO_LARGE, err("too large")) };
            let Some(c) = serde_json::from_slice::<Value>(&b).ok().filter(message_ok) else { return reply(StatusCode::BAD_REQUEST, err("a message is { text }")) };
            let r = say(&h.data, &id, &w, c["text"].as_str().unwrap_or(""), now_ms());
            audit(&h.data, &id, "say", Some(&w), Some(peer.ip()), json!({ "ok": r["ok"] }));
            reply(if r["ok"] == true { StatusCode::OK } else { StatusCode::BAD_REQUEST }, if r["ok"] == true { json!({ "ok": true, "room": public(&r["room"]) }) } else { r })
        }
        (Method::GET, "record") => {
            if room["state"] != "decided" { return reply(StatusCode::CONFLICT, err("not decided yet")); }
            // The guest writes the record from the room it already has; this only marks who saved it.
            audit(&h.data, &id, "record", Some(&w), Some(peer.ip()), json!({ "v": room["v"] }));
            reply(StatusCode::OK, json!({ "ok": true }))
        }
        _ => reply(StatusCode::METHOD_NOT_ALLOWED, err("not allowed")),
    }
}

// Exactly { text }, a string: nothing else can ride along.
pub fn message_ok(c: &Value) -> bool { c.as_object().is_some_and(|o| o.len() == 1 && o.get("text").is_some_and(Value::is_string)) }

static SERVER: Mutex<Option<(SocketAddr, tokio::sync::oneshot::Sender<()>)>> = Mutex::new(None);
pub fn running() -> Option<SocketAddr> { SERVER.lock().unwrap().as_ref().map(|s| s.0) }

// Binds `ip:port` (a tailnet address only) and serves until `stop`.
pub fn serve(ip: IpAddr, port: u16, host: Arc<Host>) -> std::io::Result<SocketAddr> {
    if !is_tailnet(ip) && !cfg!(test) { return Err(std::io::Error::other("not a tailnet address")); }
    let mut s = SERVER.lock().unwrap();
    if let Some((a, _)) = s.as_ref() { return Ok(*a); }
    let l = std::net::TcpListener::bind((ip, port))?;
    l.set_nonblocking(true)?;
    let addr = l.local_addr()?;
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let app = Router::new().fallback(handle).with_state(host);
    tauri::async_runtime::spawn(async move {
        match tokio::net::TcpListener::from_std(l) {
            Ok(l) => { if let Err(e) = axum::serve(l, app.into_make_service_with_connect_info::<SocketAddr>()).with_graceful_shutdown(async { let _ = rx.await; }).await { eprintln!("[bobix] room: {e}"); } }
            Err(e) => eprintln!("[bobix] room: {e}"),
        }
    });
    *s = Some((addr, tx));
    Ok(addr)
}
pub fn stop() { if let Some((_, tx)) = SERVER.lock().unwrap().take() { let _ = tx.send(()); } }

// ---------- a guest's side ----------

// "100.101.102.103:4748/0123456789abcdef" → (address, room id). Only a tailnet address.
pub fn parse_addr(s: &str) -> Option<(SocketAddr, String)> {
    let s = s.trim().trim_start_matches("http://").trim_end_matches('/');
    let (a, id) = s.split_once('/')?;
    let addr: SocketAddr = a.parse().ok()?;
    // loopback only in a test build (the two-process test); the app takes tailnet addresses only
    ((is_tailnet(addr.ip()) || (cfg!(test) && addr.ip().is_loopback())) && good_id(id)).then(|| (addr, id.to_string()))
}

// One HTTP/1.1 request to the host's room. → (status, body)
pub fn http(addr: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> Result<(u16, String), String> {
    let mut c = TcpStream::connect_timeout(&addr, Duration::from_secs(4)).map_err(|e| format!("can't reach the host: {e}"))?;
    let _ = c.set_read_timeout(Some(Duration::from_secs(10)));
    let b = body.map(Value::to_string).unwrap_or_default();
    let req = format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\nx-bobix-room: 1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}", b.len());
    c.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    c.take(4 * 1024 * 1024).read_to_end(&mut out).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out);
    let (head, rest) = text.split_once("\r\n\r\n").ok_or("bad answer")?;
    let code = head.split_whitespace().nth(1).and_then(|c| c.parse().ok()).ok_or("bad answer")?;
    let body = if head.to_lowercase().contains("transfer-encoding: chunked") { unchunk(rest) } else { rest.to_string() };
    Ok((code, body))
}
fn unchunk(s: &str) -> String {
    let (mut out, mut rest) = (String::new(), s);
    while let Some((size, r)) = rest.split_once("\r\n") {
        let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
        if n == 0 || r.len() < n { break; }
        out += &r[..n];
        rest = r[n..].trim_start_matches("\r\n");
    }
    out
}
fn answer(r: Result<(u16, String), String>) -> Value {
    match r {
        Ok((200, b)) => serde_json::from_str(&b).unwrap_or_else(|_| err("bad answer")),
        Ok((code, b)) => { let mut v: Value = serde_json::from_str(&b).unwrap_or_else(|_| err(format!("the host answered {code}"))); v["status"] = json!(code); v }
        Err(e) => err(e),
    }
}

// What a guest sends: only the text typed or pasted, redacted first.
pub fn message(text: &str) -> Value { json!({ "text": redact(text).0 }) }

fn joined_file(data: &Path) -> PathBuf { rooms(data).join("joined.json") }
pub fn joined(data: &Path) -> Vec<Value> { read_json(&joined_file(data)).and_then(|v| v.as_array().cloned()).unwrap_or_default() }
fn save_joined(data: &Path, f: impl FnOnce(&mut Vec<Value>)) {
    let _l = LOCK.lock().unwrap();
    let mut j = joined(data);
    f(&mut j);
    let _ = crate::settings::save(&joined_file(data), &json!(j));
}

// A guest reads the room: remembered in the list; once it is decided, its record is fetched and saved here.
pub fn remote_get(data: &Path, addr: &str) -> Value {
    let Some((a, id)) = parse_addr(addr) else { return err("a room address looks like 100.x.y.z:4748/<room id>") };
    let v = answer(http(a, "GET", &format!("/r/{id}"), None));
    if v["ok"] != true { return v; }
    let r = &v["room"];
    let mut rec = joined(data).iter().find(|j| j["addr"] == addr).and_then(|j| j["record"].as_str().map(String::from));
    if r["state"] == "decided" && rec.is_none() && r["id"] == id.as_str() {
        match save_record(data, r) {
            Ok(p) => { rec = Some(p.to_string_lossy().into()); let _ = http(a, "GET", &format!("/r/{id}/record"), None); } // tells the host's audit log
            Err(e) => eprintln!("[bobix] room record: {e}"),
        }
    }
    let entry = json!({ "addr": addr, "id": id, "title": r["title"], "host": r["host"]["name"], "state": r["state"], "seen": now_ms(), "record": rec });
    save_joined(data, |j| { j.retain(|x| x["addr"] != addr); j.insert(0, entry.clone()); });
    let mut v = v;
    v["record"] = entry["record"].clone();
    v
}
pub fn remote_say(addr: &str, text: &str) -> Value {
    let Some((a, id)) = parse_addr(addr) else { return err("bad room address") };
    answer(http(a, "POST", &format!("/r/{id}/say"), Some(&message(text))))
}
pub fn leave(data: &Path, addr: &str) -> Value { save_joined(data, |j| j.retain(|x| x["addr"] != addr)); json!({ "ok": true }) }

// The audit lines of a hosted room, newest first.
pub fn audit_lines(data: &Path, id: &str) -> Vec<Value> {
    if !good_id(id) { return vec![]; }
    let mut v: Vec<Value> = fs::read_to_string(audit_file(data, id)).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    v.reverse();
    v.truncate(300);
    v
}


#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(n: &str) -> PathBuf { let t = std::env::temp_dir().join(format!("bobix-room-{n}-{}", std::process::id())); let _ = fs::remove_dir_all(&t); t }
    fn who(l: &str) -> Who { Who { login: l.into(), name: l.split('@').next().unwrap().into(), node: "laptop".into() } }
    const T: IpAddr = IpAddr::V4(Ipv4Addr::new(100, 101, 102, 103));

    #[test]
    fn tailnet_addresses_only() {
        for (ip, ok) in [("100.64.0.1", true), ("100.127.255.254", true), ("100.63.0.1", false), ("100.128.0.1", false), ("127.0.0.1", false), ("192.168.1.9", false), ("0.0.0.0", false),
            ("fd7a:115c:a1e0::1", true), ("fd7a:115c:a1e1::1", false), ("::1", false)] {
            assert_eq!(is_tailnet(ip.parse().unwrap()), ok, "{ip}");
        }
        assert!(parse_addr("100.101.102.103:4748/0123456789abcdef").is_some());
        assert!(parse_addr("192.168.1.9:4748/0123456789abcdef").is_none(), "only a tailnet host");
        assert!(parse_addr("100.101.102.103:4748/../etc").is_none());
    }

    #[test]
    fn whois_names_a_person_never_a_tagged_device() {
        let w = parse_whois(r#"{"Node":{"ComputedName":"alex-mbp","Name":"alex-mbp.tail1234.ts.net."},"UserProfile":{"LoginName":"Alex@Example.com","DisplayName":"Alex Doe"}}"#).unwrap();
        assert_eq!(w, Who { login: "alex@example.com".into(), name: "Alex Doe".into(), node: "alex-mbp".into() });
        assert!(parse_whois(r#"{"Node":{},"UserProfile":{"LoginName":"tagged-devices"}}"#).is_none());
        assert!(parse_whois("not json").is_none());
    }

    #[test]
    fn only_invited_people_on_the_tailnet_get_in() {
        let r = json!({ "invited": ["alex@example.com"] });
        assert_eq!(admit(&r, is_tailnet(T), Some(&who("alex@example.com"))).unwrap().login, "alex@example.com");
        assert_eq!(admit(&r, is_tailnet(T), Some(&who("sam@example.com"))), Err("not invited"));
        assert_eq!(admit(&r, is_tailnet(T), None), Err("unknown on the tailnet"));
        assert_eq!(admit(&r, is_tailnet("192.168.1.9".parse().unwrap()), Some(&who("alex@example.com"))), Err("not a tailnet address"));
        assert_eq!(admit(&r, is_tailnet("127.0.0.1".parse().unwrap()), Some(&who("alex@example.com"))), Err("not a tailnet address"));
    }

    #[test]
    fn secrets_are_redacted() {
        let fake_gh = format!("ghp_{}", "a1B2c3D4e5F6g7H8i9J0k1L2m3N4");
        let text = format!("token={fake_gh} and password: hunter22, url https://bob:s3cretpw@db.example.com/x, Authorization: Bearer abcdefghijklmnop1234, \
            key AKIAABCDEFGHIJKLMNOP, jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36P\n-----BEGIN RSA PRIVATE KEY-----\nMIIBOgIBAAJBAKj34GkxFhD90vcNLYLInFEX6Ppy1tPf9Cnzj4p4WGeKLs1Pt8Qu\n-----END RSA PRIVATE KEY-----\nKafka keeps 7 days of events.");
        let (out, n) = redact(&text);
        for s in [fake_gh.as_str(), "hunter22", "s3cretpw", "abcdefghijklmnop1234", "AKIAABCDEFGHIJKLMNOP", "SflKxwRJSMeKKF2QT4fwpMeJf36P", "MIIBOgIBAAJBAKj34"] { assert!(!out.contains(s), "{s} kept: {out}"); }
        assert!(out.contains("Kafka keeps 7 days of events.") && out.contains("https://[redacted]@db.example.com") && n >= 7, "{n} {out}");
        assert_eq!(redact("Pick Postgres: one less thing to run.").0, "Pick Postgres: one less thing to run.", "plain text stays");
        assert_eq!(redact(&"x".repeat(9000)).0.len(), MAX_TEXT);
    }

    #[test]
    fn a_guest_sends_only_what_was_typed() {
        let m = message("fast, token=abcdef123456");
        assert_eq!(m, json!({ "text": "fast, token=[redacted]" }));
        assert!(message_ok(&m));
        for bad in [json!({ "text": "x", "transcript": "…" }), json!({ "kind": "note", "text": "x" }), json!({}), json!({ "text": 1 }), json!(["x"])] { assert!(!message_ok(&bad), "{bad}"); }
        let src = include_str!("room.rs");
        let code = &src[..src.find("#[cfg(test)]").unwrap()];
        for never in ["transcript_path", "usage::", "state::", "habits::read", "sessions"] { assert!(!code.contains(never), "the room code must not touch {never}"); }
    }

    #[test]
    fn drafts_are_checked() {
        assert!(parse_draft("no json").is_err());
        let d = parse_draft(r#"Here: {"problem":"Lost events","options":[{"text":"Kafka","pros":["replay"],"cons":["a cluster; password=hunter22"]},{"text":"  ","pros":[]},{"text":"Outbox","pros":[],"cons":[]}],"lean":2,"why":"No new service"}"#).unwrap();
        assert_eq!(d["options"].as_array().unwrap().len(), 2, "an empty option is dropped");
        assert_eq!(d["options"][0]["cons"][0], "a cluster; password=[redacted]");
        assert_eq!(d["decision"]["option"], json!(null), "lean 2 was the empty one, so no pick");
        let e = clean_draft(&json!({ "problem": "p", "options": [{ "id": "o1", "text": "A" }, { "id": "o1", "text": "B" }] }));
        assert!(e.is_err(), "duplicate ids");
        let ok = clean_draft(&json!({ "problem": "p", "options": [{ "id": "o1", "text": "A", "pros": ["x"] }], "decision": { "option": "o9", "why": "w" } })).unwrap();
        assert_eq!(ok["decision"]["option"], json!(null), "an unknown option is not a pick");
    }

    #[test]
    fn host_flow_and_audit() {
        let data = scratch("flow");
        let (me, alex) = (who("host@example.com"), who("alex@example.com"));
        let r = create(&data, &me, "Pick a queue for order events", "Orders are lost when the worker restarts.", &["alex@example.com".into(), "host@example.com".into(), "bad".into()], 1);
        let id = r["room"]["id"].as_str().unwrap().to_string();
        assert_eq!(r["room"]["invited"], json!(["alex@example.com"]), "not yourself, not a bad login");
        assert_eq!(say(&data, &id, &alex, "Kafka gives replays; password=topsecret1", 2)["room"]["msgs"][0]["text"], "Kafka gives replays; password=[redacted]");
        say(&data, &id, &me, "An outbox table means no new service", 3);
        assert_eq!(say(&data, &id, &alex, "   ", 3)["ok"], false);
        let dr = draft(&data, &id, &|m: &str, tools: &str, p: &str| {
            assert!(m == "sonnet" && tools.is_empty() && p.contains("alex: Kafka gives replays") && !p.contains("topsecret1"));
            Ok(r#"{"problem":"Orders are lost on restart","options":[{"text":"Kafka","pros":["replays"],"cons":[]},{"text":"Outbox","pros":["no new service"],"cons":[]}],"lean":2,"why":"Simpler. ghp_abcdefghijklmnopqrstuvwxyz"}"#.into())
        });
        assert_eq!(dr["room"]["draft"]["decision"]["why"], "Simpler. [redacted]");
        assert_eq!(dr["room"]["draft"]["decision"]["option"], "o2");
        let mut edited = dr["room"]["draft"].clone();
        edited["decision"]["why"] = json!("Simpler, and Kafka can come later.");
        edited["options"][1]["cons"] = json!(["polling load on the database"]);
        assert_eq!(decide(&data, &id, &json!({ "options": [{ "text": "A" }] }), 4)["ok"], false, "no pick, no decision");
        let d = decide(&data, &id, &edited, 4);
        assert_eq!(d["ok"], true, "{d}");
        assert_eq!(d["room"]["decision"]["by"]["login"], "host@example.com");
        let html = fs::read_to_string(d["record"].as_str().unwrap()).unwrap();
        for s in ["Pick a queue for order events", "Simpler, and Kafka can come later.", "polling load on the database", "Chosen", "Kafka gives replays", "Decided by host"] { assert!(html.contains(s), "{s}"); }
        assert!(!html.contains("<script") && !html.contains("topsecret1"));
        assert_eq!(say(&data, &id, &alex, "late", 5)["ok"], false, "decided rooms are closed");
        let ev: Vec<String> = audit_lines(&data, &id).iter().map(|l| l["ev"].as_str().unwrap().to_string()).collect();
        assert_eq!(ev, ["decide", "create"]);
        assert!(markdown(&d["room"]).contains("### Outbox (chosen)"));
        let _ = fs::remove_dir_all(&data);
    }

    // The real server on a loopback port: a non-tailnet peer is refused before anything else.
    #[test]
    fn the_server_refuses_non_tailnet_peers() {
        let data = scratch("srv");
        let me = who("host@example.com");
        let id = create(&data, &me, "Room", "", &["alex@example.com".into()], 1)["room"]["id"].as_str().unwrap().to_string();
        let a = serve("127.0.0.1".parse().unwrap(), 0, Arc::new(Host { data: data.clone(), host: "x".into(), whois: Box::new(|_| Some(who("alex@example.com"))), peer_ok: is_tailnet })).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let (code, body) = http(a, "GET", &format!("/r/{id}"), None).unwrap();
        stop();
        assert_eq!((code, body.contains("not a tailnet address")), (403, true));
        assert!(!body.contains("Room\""), "nothing of the room");
        let _ = fs::remove_dir_all(&data);
    }

    // Two Bobix backends in two processes, each with its own BOBIX_HOME and data folder: create → refused by a room that
    // doesn't invite the guest → join → chat → the host's real `claude -p` drafts the frame → the host edits it → decide
    // → the same record saved on both. Identity is the real `tailscale whois` of this machine (both sides are the same
    // person on one machine); the transport is loopback, because macOS's firewall holds an unsigned test binary
    // listening on the tailnet address until someone clicks Allow. Tailscale is only asked (ip, whois), never changed.
    // BOBIX_ROOM_E2E=<dir> cargo test room::tests::two_instances -- --ignored --nocapture
    #[test]
    #[ignore]
    fn two_instances() {
        let dir = PathBuf::from(std::env::var("BOBIX_ROOM_E2E").expect("BOBIX_ROOM_E2E"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let exe = std::env::current_exe().unwrap();
        let spawn = |role: &str| std::process::Command::new(&exe).args(["--ignored", "--exact", "room::tests::role", "--nocapture"])
            .env("ROOM_ROLE", role).env("BOBIX_ROOM_E2E", &dir).env("BOBIX_DATA_DIR", dir.join(role).join("data")).env("BOBIX_HOME", dir.join(role).join("home"))
            .spawn().unwrap();
        let (mut h, mut g) = (spawn("host"), spawn("guest"));
        let (hs, gs) = (h.wait().unwrap(), g.wait().unwrap());
        assert!(hs.success() && gs.success(), "host {hs}, guest {gs}");
        let rec = |r: &str| fs::read_dir(dir.join(r).join("data/rooms/records")).unwrap().flatten().map(|e| e.path()).next().unwrap();
        let (a, b) = (fs::read_to_string(rec("host")).unwrap(), fs::read_to_string(rec("guest")).unwrap());
        assert_eq!(a, b, "the same record on both machines");
        assert!(a.contains("Kafka can come later") && !a.contains("abcdef123456"));
        println!("records: {} and {}", rec("host").display(), rec("guest").display());
    }

    #[test]
    #[ignore]
    fn role() {
        let Ok(role) = std::env::var("ROOM_ROLE") else { return };
        let dir = PathBuf::from(std::env::var("BOBIX_ROOM_E2E").unwrap());
        let data = crate::paths::data_dir();
        let ts = "/usr/local/bin/tailscale";
        let wait = |f: &str| { for _ in 0..900 { if dir.join(f).exists() { return fs::read_to_string(dir.join(f)).unwrap(); } std::thread::sleep(Duration::from_millis(200)); } panic!("waited for {f}") };
        if role == "host" {
            let ip = my_ip(ts).expect("tailscale up");
            let me = whois(ts, &ip.to_string()).expect("whois of this machine");
            // on one machine the guest is the same person: invite it by that login (a second room invites someone else)
            let a = create(&data, &me, "Pick a queue for order events", "Order events are lost when the worker restarts.", &[], now_ms());
            let mut r = a["room"].clone();
            r["invited"] = json!([me.login.clone()]);
            save(&data, &r).unwrap();
            let id = r["id"].as_str().unwrap().to_string();
            let other = create(&data, &me, "Someone else's room", "", &["someone@example.com".into()], now_ms())["room"]["id"].as_str().unwrap().to_string();
            let port = 4790;
            let ip_s = ip.to_string();
            let host = Arc::new(Host { data: data.clone(), host: format!("127.0.0.1:{port}"), whois: Box::new(move |_| whois(ts, &ip_s)), peer_ok: |ip| ip.is_loopback() });
            serve(IpAddr::V4(Ipv4Addr::LOCALHOST), port, host).unwrap();
            fs::write(dir.join("addr"), format!("127.0.0.1:{port}/{id}\n127.0.0.1:{port}/{other}")).unwrap();
            say(&data, &id, &me, "What about an outbox table in Postgres? No new service to run.", now_ms());
            wait("guest-said");
            say(&data, &id, &me, "Polling every second is fine at 300 events a second.", now_ms());
            let bin = crate::find_bin("claude").expect("claude");
            let run = |m: &str, t: &str, p: &str| crate::habit_skills::claude(&bin, &data.join("rooms/work"), m, t, p, 300);
            let d = draft(&data, &id, &run);
            assert_eq!(d["ok"], true, "{d}");
            let mut edited = d["room"]["draft"].clone();
            println!("draft: {}", serde_json::to_string_pretty(&edited).unwrap());
            let outbox = edited["options"].as_array().unwrap().iter().find(|o| o["text"].as_str().unwrap().to_lowercase().contains("outbox")).expect("an outbox option")["id"].clone();
            edited["decision"] = json!({ "option": outbox, "why": "One less service to run; Kafka can come later if replays matter." });
            let r = decide(&data, &id, &edited, now_ms());
            assert_eq!(r["ok"], true, "{r}");
            fs::write(dir.join("decided"), "").unwrap();
            wait("guest-done");
            stop();
            let ev: Vec<String> = audit_lines(&data, &id).iter().rev().map(|l| l["ev"].as_str().unwrap().to_string()).collect();
            println!("audit: {ev:?}");
            for e in ["create", "join", "say", "saw", "decide", "record"] { assert!(ev.contains(&e.to_string()), "{e} in {ev:?}"); }
            assert!(audit_lines(&data, &other).iter().any(|l| l["ev"] == "refused" && l["why"] == "not invited"));
        } else {
            let addrs = wait("addr");
            let (addr, other) = addrs.split_once('\n').unwrap();
            let no = remote_get(&data, other);
            assert_eq!((no["ok"].clone(), no["status"].clone(), no["error"].clone()), (json!(false), json!(403), json!("not invited")), "{no}");
            assert_eq!(remote_get(&data, addr)["ok"], true);
            assert_eq!(remote_say(addr, "Kafka gives us replays for free when a consumer has a bug.")["ok"], true);
            let c = remote_say(addr, "But it is a new cluster to run; the staging password=abcdef123456 is already shared too widely.");
            assert!(c["room"]["msgs"].as_array().unwrap().last().unwrap()["text"].as_str().unwrap().contains("password=[redacted]"));
            fs::write(dir.join("guest-said"), "").unwrap();
            wait("decided");
            let v = remote_get(&data, addr);
            let rec = v["record"].as_str().expect("record saved here");
            assert!(Path::new(rec).starts_with(&data));
            fs::write(dir.join("guest-done"), rec).unwrap();
        }
    }
}
