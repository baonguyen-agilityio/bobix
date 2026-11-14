// Permission requests held in Bobix until someone answers them on a pet, the panel or a notification. Port of
// waitForAnswer / answer / releaseUnseen in the 0.1 (Electron) app's main.js.
use crate::{server::Reply, settings, state::Session, Bobix};
use fancy_regex::Regex;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::{Arc, LazyLock}, time::Duration};
use tokio::sync::oneshot;

// plan: a ticket flow's plan (ExitPlanMode), held until answered; approving it puts that one session in auto mode.
// raw: the request's text as it arrives (what a phone is shown, escaped by phone_view); until: when it stops waiting, if it does.
// released: someone is at the computer, so the phones get no more pushes for it; it can still be answered.
pub struct Pending { pub sid: String, pub plan: bool, tx: oneshot::Sender<Option<Value>>, pub raw: String, pub cut: bool, pub risky: bool, pub until: Option<u64>, pub released: bool,
    // held for the phones only (the pets were hidden or paused, nobody at the computer): a settings change does not hand it back
    pub for_phones: bool, pub held_at: u64 }

impl Pending {
    #[cfg(test)]
    pub fn for_test(sid: &str, plan: bool, tx: oneshot::Sender<Option<Value>>, raw: &str, cut: bool) -> Pending {
        Pending { sid: sid.into(), plan, tx, raw: raw.into(), cut, risky: false, until: None, released: false, for_phones: false, held_at: 0 }
    }
}

// Who answered: the computer (a page, a pet, a notification) or a phone.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum By { Pc, Phone }
// How a request ended. Answered(by, allowed); Expired: nobody answered in time, or Claude moved on; Released(why):
// handed back to Claude's own prompt ("settings": the pets were hidden or paused, "restart": Bobix is restarting);
// Computer: it was released because someone was at the computer, and then its hook closed (answered in Claude's own prompt).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum End { Answered(By, bool), Expired, Released(&'static str), Computer }
// A closed request, kept CLOSED_KEEP_MS so a late answer is told why it is late, not just "no".
pub struct Closed { pub plan: bool, pub end: End, pub at: u64 }
const CLOSED_KEEP_MS: u64 = 30 * 60_000; // the contract is 20 minutes at least
// The record of closed requests as JSON, for the file that survives a restart (phones.rs).
pub fn closed_json(m: &HashMap<String, Closed>) -> Value {
    json!(m.iter().map(|(id, k)| {
        let (kind, by, allowed, why) = match k.end {
            End::Answered(by, a) => ("answered", Some(by == By::Phone), a, ""), End::Expired => ("expired", None, false, ""),
            End::Released(w) => ("released", None, false, w), End::Computer => ("computer", None, false, ""),
        };
        json!({ "id": id, "plan": k.plan, "kind": kind, "phone": by, "allowed": allowed, "why": why, "at": k.at })
    }).collect::<Vec<_>>())
}
pub fn closed_from_json(v: &Value) -> HashMap<String, Closed> {
    v.as_array().into_iter().flatten().filter_map(|x| {
        let end = match x["kind"].as_str()? {
            "answered" => End::Answered(if x["phone"] == true { By::Phone } else { By::Pc }, x["allowed"] == true),
            "expired" => End::Expired, "computer" => End::Computer,
            _ => End::Released(match x["why"].as_str() { Some("settings") => "settings", _ => "restart" }),
        };
        Some((x["id"].as_str()?.to_string(), Closed { plan: x["plan"] == true, end, at: x["at"].as_u64()? }))
    }).collect()
}
// Bobix is closing: what is still held goes back to Claude's own prompt, and a phone that asks later is told why.
pub fn release_all(c: &mut crate::Core, why: &'static str, now: u64) {
    let ids: Vec<String> = c.pending.keys().cloned().collect();
    for id in ids { close(c, &id, None, End::Released(why), now); }
}

// What an answer gets back.
#[derive(Debug, PartialEq, Eq)]
pub enum Answered { Ok, LatePc, LatePhone, Expired, Released(&'static str), WrongKind, Unknown, Invalid }

// Allow or Deny, this request only: Bobix never sets standing permission rules (Claude's own auto mode does that).
fn decision(choice: &str) -> Option<Value> {
    Some(match choice {
        "allow" => json!({ "behavior": "allow" }),
        "deny" => json!({ "behavior": "deny", "message": "Denied from Bobix." }),
        _ => return None,
    })
}
// A ticket flow's plan: approved, the session runs on in Claude's own auto mode (this session only, as Claude's "Yes, and
// use auto mode"; nothing is written to any settings); sent back, Claude gets what to change. hook.js adds the plan
// itself (updatedInput), which Claude needs to take an approval.
fn plan_decision(approve: bool, change: &str) -> Value {
    if approve { return json!({ "behavior": "allow", "updatedPermissions": [{ "type": "setMode", "mode": "auto", "destination": "session" }] }); }
    let t = change.trim();
    json!({ "behavior": "deny", "message": if t.is_empty() { "Not approved from Bobix. Revise the plan and its assumptions, then ask again.".to_string() } else { format!("Change the plan, then ask again: {}", crate::js::head16(t, 2000)) } })
}
// What the hook prints for Claude Code.
fn output(d: Value) -> Value { json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": d } }) }

// Deletes or overwrites: shell removals, moves and copies over files, "> file", in-place edits, hard resets and force
// pushes, SQL drops; the Write tool (replaces a whole file); a tool named for deleting. Wide on purpose: a false alarm
// only adds a warning to the request.
pub fn risky(tool: &str, input: &Value) -> bool {
    static CMD: LazyLock<Regex> = LazyLock::new(|| Regex::new(concat!(
        r"(?i)(?:^|[\s;&|(`$])(?:rm|rmdir|unlink|shred|mv|cp|dd|truncate|mkfs\S*|wipefs|del|erase|rd|",
        r"remove-item|ri|move-item|copy-item|set-content|out-file|clear-content|tee|rsync)(?:\s|$)",
        r"|(?:^|[^>&0-9])>(?![>&])(?!\s*/dev/null\b)",
        r"|\bgit\s+(?:reset\s+--hard|clean|checkout\s+(?:--|\.)|restore|push\b[^;&|]*(?:--force|\s-f\b)|branch\s+-[dD]\b|stash\s+(?:drop|clear)|rm\b)",
        r"|--force\b|\bsed\s+(?:-\w*i|--in-place)|\bperl\s+-\w*i|\bfind\b[^;&|]*\s-(?:delete|exec\s+rm)\b",
        r"|\b(?:drop|truncate)\s+(?:table|database|schema)\b|\bdelete\s+from\b")).unwrap());
    static NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)delete|remove|drop|destroy|overwrite|truncate|purge|wipe|erase").unwrap());
    match tool {
        "Write" | "NotebookEdit" => true,
        "Bash" | "PowerShell" => input["command"].as_str().is_none_or(|c| CMD.is_match(c).unwrap_or(true)),
        t => NAME.is_match(t).unwrap_or(true),
    }
}

// Control and format characters change what a screen shows without being seen: bidirectional overrides, zero-width
// characters, terminal escapes. A newline, a tab and a CRLF are text. (Cf, plus U+2028/2029.)
fn odd_char(c: char, next: Option<char>) -> bool {
    match c {
        '\n' | '\t' => false,
        '\r' => next != Some('\n'),
        c => c.is_control() || matches!(c as u32, 0xAD | 0x600..=0x605 | 0x61C | 0x6DD | 0x70F | 0x8E2 | 0x180E | 0x200B..=0x200F | 0x2028..=0x202E
            | 0x2060..=0x2064 | 0x2066..=0x206F | 0xFEFF | 0xFFF9..=0xFFFB | 0x110BD | 0x110CD | 0x13430..=0x1343F | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A | 0xE0001 | 0xE0020..=0xE007F),
    }
}
fn odd_str(s: &str) -> bool { let mut it = s.chars().peekable(); while let Some(c) = it.next() { if odd_char(c, it.peek().copied()) { return true; } } false }
// Any string of the value, or any key, holds such a character.
pub fn odd(v: &Value) -> bool {
    match v {
        Value::String(s) => odd_str(s),
        Value::Array(a) => a.iter().any(odd),
        Value::Object(m) => m.iter().any(|(k, v)| odd_str(k) || odd(v)),
        _ => false,
    }
}
// The same text with each such character as a visible escape (\u202E), so the text shown is the text that runs.
pub fn escape_hidden(s: &str) -> String { escaped(s) }
fn escaped(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if !odd_char(c, it.peek().copied()) { out.push(c); } else if (c as u32) < 0x10000 { out.push_str(&format!("\\u{:04X}", c as u32)); } else { out.push_str(&format!("\\u{{{:X}}}", c as u32)); }
    }
    out
}

// The request's text, whole: the main part (the command, file, address…) first, then every other key as "key: value".
// A string of a different kind (an object, a number) is written as JSON.
fn full_text(e: &Value) -> String { escaped(&raw_text(e)) }
pub fn raw_text(e: &Value) -> String {
    let i = &e["tool_input"];
    let Some(m) = i.as_object() else { return String::new() };
    let text = |v: &Value| v.as_str().map(String::from).unwrap_or_else(|| v.to_string());
    let main = ["command", "file_path", "url", "pattern", "query", "plan"].into_iter().find(|k| m.get(*k).and_then(Value::as_str).is_some_and(|s| !s.is_empty()));
    let rest: Vec<String> = m.iter().filter(|(k, _)| Some(k.as_str()) != main).map(|(k, v)| format!("{k}: {}", text(v))).collect();
    let all = match main { Some(k) => std::iter::once(text(&m[k])).chain((!rest.is_empty()).then(|| rest.join("\n"))).collect::<Vec<_>>().join("\n\n"), None => rest.join("\n") };
    all
}

fn new_id() -> String {
    let mut b = [0u8; 16];
    let _ = getrandom::fill(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// Holds session `s`'s request when the person can see it (settings::hold_secs). Called with the core locked.
// plan: a ticket flow's plan, held until you answer whatever the pets show (it is the flow's one gate).
pub fn wait(b: &Arc<Bobix>, pending: &mut HashMap<String, Pending>, closed: &mut HashMap<String, Closed>, phones: &crate::phones::State, settings: &Value, s: &mut Session, e: &Value, plan: bool) -> Option<Reply> {
    let mut secs = if plan { f64::INFINITY } else { settings::hold_secs(settings, &s.pet) };
    // nothing is shown to the person here (pets hidden or paused), but a phone is paired and nobody is at the computer: hold it for the phones
    let for_phones = secs == 0.0 && !plan && phones.holds_for_phones(settings, &*crate::presence::source());
    if for_phones { secs = f64::INFINITY; }
    if secs == 0.0 { return None; }
    // one open request per session
    let now = crate::state::now();
    if let Some((oid, old)) = s.answer.as_ref().and_then(|a| a["id"].as_str()).and_then(|id| pending.remove(id).map(|p| (id.to_string(), p))) {
        let _ = old.tx.send(None);
        closed.insert(oid, Closed { plan: old.plan, end: End::Expired, at: now }); // Claude asked again: the old one is moot
    }
    let id = new_id();
    let cut = e["bobix_cut"] == true; // the hook dropped part of the input
    let risky = cut || risky(e["tool_name"].as_str().unwrap_or(""), &e["tool_input"]);
    let (tx, rx) = oneshot::channel();
    let (gone_tx, gone_rx) = oneshot::channel::<()>();
    let forever = secs.is_infinite();
    // until: null = no countdown. risky: it deletes or overwrites, or the input was cut (the panel warns). odd: hidden characters
    // (shown as escapes). full: the request's text, whole.
    s.answer = Some(json!({ "id": id, "until": if forever { Value::Null } else { json!(now + (secs * 1000.0) as u64) },
        "risky": risky, "cut": cut, "odd": odd(&e["tool_input"]), "full": full_text(e), "plan": plan }));
    pending.insert(id.clone(), Pending { sid: s.id.clone(), plan, tx, raw: raw_text(e), cut, risky, until: (!forever).then(|| now + (secs * 1000.0) as u64), released: false, for_phones, held_at: now });
    // Nobody answered in time, or Claude moved on (Esc, session closed: the hook's request closes).
    let (b1, id1) = (b.clone(), id.clone());
    tauri::async_runtime::spawn(async move {
        // gone: the hook's request closed (Claude moved on, or it was answered in Claude's own prompt)
        let gone = if forever { let _ = gone_rx.await; true } else {
            tokio::select! { _ = gone_rx => true, _ = tokio::time::sleep(Duration::from_secs_f64(secs)) => false }
        };
        // a request released because someone is at the computer, whose hook closes, was answered there
        let at_computer = gone && b1.core.lock().unwrap().pending.get(&id1).is_some_and(|p| p.released);
        done(&b1, &id1, None, if at_computer { End::Computer } else { End::Expired });
    });
    Some(Reply::Answer { secs: if forever { "forever".into() } else { (secs as u64).to_string() }, rx, gone: gone_tx })
}

// Ends a held request in the core: with a decision for Claude, or None (Claude asks in its own UI). The first call closes
// it and is the only one that returns true; the record says how, for whoever comes late. Called with the core locked.
fn close(c: &mut crate::Core, id: &str, decision: Option<Value>, end: End, now: u64) -> bool {
    let Some(p) = c.pending.remove(id) else { return false };
    if p.plan && decision.is_some() { let ok = decision.as_ref().is_some_and(|d| d["behavior"] == "allow"); c.tickets.answered(&p.sid, ok, now); c.tickets.save(); }
    if let Some(s) = c.sessions.get_mut(&p.sid) {
        if s.answer.as_ref().is_some_and(|a| a["id"] == id) { s.answer = None; }
        if decision.is_some() { s.ask.clear(); s.ask_tool.clear(); s.state = "working".into(); s.since = now; }
    }
    let _ = p.tx.send(decision.map(output));
    c.closed.retain(|_, k| now.saturating_sub(k.at) < CLOSED_KEEP_MS);
    c.closed.insert(id.to_string(), Closed { plan: p.plan, end, at: now });
    c.phones.closed_events.push((id.to_string(), p.sid.clone(), p.plan)); // the phones told of it are told it is gone
    true
}

pub fn done(b: &Arc<Bobix>, id: &str, decision: Option<Value>, end: End) -> bool {
    let closed = close(&mut b.core.lock().unwrap(), id, decision, end, crate::state::now());
    if closed { b.broadcast(); }
    closed
}

// What a late or wrong answer to `id` is told, from the record.
fn late(c: &crate::Core, id: &str, expect: Option<bool>) -> Answered {
    match c.closed.get(id) {
        None => Answered::Unknown,
        Some(k) if expect.is_some_and(|plan| plan != k.plan) => Answered::WrongKind,
        Some(k) => match k.end { End::Answered(By::Pc, _) => Answered::LatePc, End::Answered(By::Phone, _) => Answered::LatePhone,
            End::Computer => Answered::LatePc, End::Expired => Answered::Expired, End::Released(why) => Answered::Released(why) },
    }
}

// An answer: allow | deny. On a ticket flow's plan, allow approves it (auto mode) and deny sends it back; `change`: what to
// change, from the Work screen. `expect`: Some(false) only a plain request, Some(true) only a plan; a page that shows what
// it asks about passes None. The first answer wins; the others are told why they are late.
pub fn respond(b: &Arc<Bobix>, id: &str, choice: &str, change: &str, by: By, expect: Option<bool>) -> Answered {
    let r = respond_core(&mut b.core.lock().unwrap(), id, choice, change, by, expect, crate::state::now());
    if r == Answered::Ok { b.broadcast(); }
    r
}
// The same with the core already locked (a command from a phone arrives with it locked).
pub fn respond_core(c: &mut crate::Core, id: &str, choice: &str, change: &str, by: By, expect: Option<bool>, now: u64) -> Answered {
    if !matches!(choice, "allow" | "deny") { return Answered::Invalid; }
    let plan = match c.pending.get(id) {
        Some(p) if expect.is_some_and(|e| e != p.plan) => return Answered::WrongKind,
        Some(p) => p.plan,
        None => return late(c, id, expect),
    };
    let d = if plan { plan_decision(choice == "allow", change) } else { decision(choice).unwrap() };
    if !close(c, id, Some(d), End::Answered(by, choice == "allow"), now) { return late(c, id, expect); }
    Answered::Ok
}

// An answer from a page or a notification (the computer).
pub fn answer(b: &Arc<Bobix>, id: &str, choice: &str) -> bool { answer_with(b, id, choice, "") }
pub fn answer_with(b: &Arc<Bobix>, id: &str, choice: &str, change: &str) -> bool { respond(b, id, choice, change, By::Pc, None) == Answered::Ok }

// A held request whose pet is now hidden (or the pets paused / off) goes back to Claude, so it asks in its own UI.
pub fn release_unseen(b: &Arc<Bobix>) {
    let ids = unseen_ids(&b.core.lock().unwrap());
    for id in ids { done(b, &id, None, End::Released("settings")); }
}
// The held requests a settings change hands back: those whose pet is no longer shown, except a plan, one held only for
// the phones, and one already sent to a phone.
fn unseen_ids(c: &crate::Core) -> Vec<String> {
    c.sessions.values().filter(|s| s.answer.as_ref().is_some_and(|a| a["plan"] != true) && settings::hold_secs(&c.settings, &s.pet) == 0.0)
        .filter_map(|s| s.answer.as_ref()?["id"].as_str().map(String::from)).filter(|id| !c.pending.get(id).is_some_and(|p| p.for_phones) && !c.phones.was_sent(id)).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn decisions_match_the_electron_build() {
        assert_eq!(super::decision("allow").unwrap(), serde_json::json!({ "behavior": "allow" }));
        assert_eq!(super::decision("deny").unwrap()["message"], "Denied from Bobix.");
        assert!(super::decision("always").is_none(), "no standing rules");
        assert!(super::decision("maybe").is_none());
        let ok = super::plan_decision(true, "");
        assert_eq!(ok["updatedPermissions"], serde_json::json!([{ "type": "setMode", "mode": "auto", "destination": "session" }]), "this session's mode only");
        assert!(ok.get("updatedInput").is_none(), "hook.js gives the whole plan back");
        assert_eq!(super::plan_decision(false, " use ints ")["message"], "Change the plan, then ask again: use ints");
        assert_eq!(super::plan_decision(false, "")["behavior"], "deny");
        let e = serde_json::json!({ "tool_input": { "command": "npm test", "description": "d" } });
        assert_eq!(super::full_text(&e), "npm test\n\ndescription: d", "the main part, then every other key");
        assert_eq!(super::full_text(&serde_json::json!({ "tool_input": { "command": "ls" } })), "ls");
        assert_eq!(super::full_text(&serde_json::json!({ "tool_input": { "a": 1 } })), "a: 1");
        assert_eq!(super::full_text(&serde_json::json!({ "tool_input": { "plan": "1. add it" } })), "1. add it", "a plan reads as itself");
        assert_eq!(super::new_id().len(), 32);
    }

    #[test]
    fn deleting_or_overwriting_is_flagged() {
        let bash = |c: &str| serde_json::json!({ "command": c, "description": "x" });
        for c in ["rm -rf ./logs", "rm x", "sudo rm -r /tmp/a", "find . -name '*.log' -delete", "git reset --hard HEAD~1", "git push --force origin main",
            "git push -f", "git clean -fdx", "git checkout -- src", "echo hi > out.txt", "cat a >file", "mv a b", "cp a b", "dd if=/dev/zero of=x",
            "sed -i 's/a/b/' f", "psql -c 'DROP TABLE users'", "npm test && rm -rf x", "ls | xargs rm", "Remove-Item -Recurse x", "tee out.txt", "x --force"] {
            assert!(super::risky("Bash", &bash(c)), "{c}");
        }
        for c in ["npm test", "cargo test --workspace", "git status", "ls -la 2>/dev/null", "echo hi >> log.txt", "npm run build 2>&1", "git push origin feat/x", "npm install"] {
            assert!(!super::risky("Bash", &bash(c)), "{c}");
        }
        assert!(super::risky("Write", &serde_json::json!({ "file_path": "/a" })) && super::risky("mcp__db__delete_rows", &serde_json::json!({})));
        assert!(!super::risky("WebFetch", &serde_json::json!({ "url": "https://x.y" })) && !super::risky("Edit", &serde_json::json!({ "file_path": "/a" })));
    }

    use super::*;
    use serde_json::json;

    #[test]
    fn the_whole_input_is_shown_not_cut() {
        let long = "x".repeat(5000);
        assert_eq!(full_text(&json!({ "tool_input": { "command": long } })).len(), 5000, "a long command is whole");
        let w = full_text(&json!({ "tool_input": { "file_path": "/a.txt", "content": "line1\nline2" } }));
        assert_eq!(w, "/a.txt\n\ncontent: line1\nline2", "a Write shows its content");
        let mcp = full_text(&json!({ "tool_input": { "table": "users", "where": { "id": 1 } } }));
        assert_eq!(mcp, "table: users\nwhere: {\"id\":1}", "an MCP call shows its arguments");
        assert!(risky("Bash", &json!({ "command": format!("{} && rm -rf x", "a".repeat(2000)) })), "a dangerous part far from the start");
    }

    #[test]
    fn hidden_characters_are_marked_and_shown_escaped() {
        assert!(odd(&json!({ "command": "ls \u{202E}fdp.sh" })), "bidirectional override");
        assert!(odd(&json!({ "a": ["x", { "b": "zero\u{200B}width" }] })) && odd(&json!({ "k\u{1b}[2J": "v" })), "nested, and in a key; terminal escape");
        assert!(odd(&json!({ "c": "a\rb" })) && odd(&json!({ "c": "bell\u{7}" })), "a bare CR, a control");
        assert!(!odd(&json!({ "content": "line1\nline2\r\n\tindented ✓ é 日本語 😀" })), "ordinary text, newlines and tabs are text");
        let e = json!({ "tool_input": { "command": "echo \u{202E}ok\u{1F600}\u{E0041}" } });
        assert_eq!(full_text(&e), "echo \\u202Eok😀\\u{E0041}", "each odd character is a visible escape, emoji stay");
    }

    fn pending(c: &mut crate::Core, id: &str, plan: bool) -> oneshot::Receiver<Option<Value>> {
        let (tx, rx) = oneshot::channel();
        c.pending.insert(id.into(), Pending { sid: "s1".into(), plan, tx, raw: String::new(), cut: false, risky: false, until: None, released: false, for_phones: false, held_at: 0 });
        rx
    }

    #[test]
    fn the_first_answer_closes_a_request_and_the_rest_are_told_why() {
        let mut c = crate::Core::default();
        let mut rx = pending(&mut c, "a", false);
        assert!(close(&mut c, "a", decision("allow"), End::Answered(By::Phone, true), 1000));
        assert!(!close(&mut c, "a", decision("deny"), End::Answered(By::Pc, false), 1001), "a second call changes nothing");
        assert_eq!(rx.try_recv().unwrap().unwrap()["hookSpecificOutput"]["decision"]["behavior"], "allow", "Claude got the first");
        assert!(matches!(c.closed["a"].end, End::Answered(By::Phone, true)));
        assert_eq!(late(&c, "a", None), Answered::LatePhone);
        let _ = pending(&mut c, "b", false);
        close(&mut c, "b", None, End::Answered(By::Pc, true), 1002);
        assert_eq!(late(&c, "b", None), Answered::LatePc);
        assert_eq!(late(&c, "nope", None), Answered::Unknown);
    }

    #[test]
    fn giving_up_or_handing_back_is_recorded() {
        let mut c = crate::Core::default();
        let _ = pending(&mut c, "a", false);
        let _ = pending(&mut c, "b", false);
        close(&mut c, "a", None, End::Expired, 1000);
        close(&mut c, "b", None, End::Released("restart"), 1000);
        assert_eq!(late(&c, "a", None), Answered::Expired, "an answer after Claude gave up is not ok");
        assert_eq!(late(&c, "b", None), Answered::Released("restart"));
    }

    #[test]
    fn a_plan_and_a_request_are_not_answered_as_each_other() {
        let mut c = crate::Core::default();
        let _ = pending(&mut c, "plan", true);
        close(&mut c, "plan", None, End::Expired, 1);
        assert_eq!(late(&c, "plan", Some(false)), Answered::WrongKind, "a plan id answered as a request");
        assert_eq!(late(&c, "plan", Some(true)), Answered::Expired);
        assert_eq!(late(&c, "plan", None), Answered::Expired, "the computer's own pages ask for no kind");
    }

    #[test]
    fn the_record_is_kept_twenty_minutes_and_then_forgotten() {
        let mut c = crate::Core::default();
        let _ = pending(&mut c, "a", false);
        close(&mut c, "a", None, End::Expired, 0);
        let _ = pending(&mut c, "b", false);
        close(&mut c, "b", None, End::Expired, 19 * 60_000);
        assert!(c.closed.contains_key("a"), "still there at 19 minutes");
        let _ = pending(&mut c, "c", false);
        close(&mut c, "c", None, End::Expired, CLOSED_KEEP_MS + 1);
        assert!(!c.closed.contains_key("a") && c.closed.contains_key("c"), "forgotten past the keep time");
    }

    #[test]
    fn two_threads_answering_one_id_close_it_once() {
        let c = Arc::new(std::sync::Mutex::new(crate::Core::default()));
        let _ = pending(&mut c.lock().unwrap(), "a", false);
        let wins: usize = (0..8).map(|i| { let c = c.clone(); std::thread::spawn(move || close(&mut c.lock().unwrap(), "a", decision("allow"),
            End::Answered(if i % 2 == 0 { By::Pc } else { By::Phone }, true), 5)) }).collect::<Vec<_>>().into_iter().map(|t| t.join().unwrap() as usize).sum();
        assert_eq!(wins, 1, "exactly one is told it closed the request");
    }

    #[test]
    fn answered_in_claudes_own_prompt_after_release_is_late_for_a_phone_and_the_record_survives_a_restart() {
        let mut c = crate::Core::default();
        let _ = pending(&mut c, "a", false);
        let _ = pending(&mut c, "b", false);
        let _ = pending(&mut c, "c", true);
        close(&mut c, "a", None, End::Computer, 10);
        assert_eq!(late(&c, "a", None), Answered::LatePc);
        close(&mut c, "b", decision("allow"), End::Answered(By::Phone, true), 11);
        release_all(&mut c, "restart", 12);
        assert!(c.pending.is_empty());
        let back = closed_from_json(&closed_json(&c.closed));
        assert_eq!(back.len(), 3);
        assert_eq!(back["a"].end, End::Computer);
        assert_eq!(back["b"].end, End::Answered(By::Phone, true));
        assert!(back["c"].plan && back["c"].end == End::Released("restart"), "a plan held at the restart is released with the reason");
        c.closed = back;
        assert_eq!(late(&c, "c", None), Answered::Released("restart"), "after a restart a phone is told released, not unknown");
    }

    #[test]
    fn a_settings_change_hands_back_only_what_no_phone_was_told_of() {
        let mut c = crate::Core::default();
        c.settings = json!({ "approveWait": -1, "petsVisible": false, "paused": false });
        for (sid, id) in [("s1", "plain"), ("s2", "sent"), ("s3", "phonesonly")] {
            let _ = pending(&mut c, id, false);
            let mut s = crate::state::Session::default();
            s.id = sid.into(); s.answer = Some(json!({ "id": id, "plan": false }));
            c.sessions.insert(sid.into(), s);
        }
        c.phones.mark_sent("sent");
        c.pending.get_mut("phonesonly").unwrap().for_phones = true;
        assert_eq!(unseen_ids(&c), vec!["plain".to_string()]);
    }
}
