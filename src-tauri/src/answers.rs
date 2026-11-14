// Permission requests held in Bobix until someone answers them on a pet, the panel or a notification. Port of
// waitForAnswer / answer / releaseUnseen in the 0.1 (Electron) app's main.js.
use crate::{server::Reply, settings, state::Session, Bobix};
use fancy_regex::Regex;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::{Arc, LazyLock}, time::Duration};
use tokio::sync::oneshot;

// plan: a ticket flow's plan (ExitPlanMode), held until answered; approving it puts that one session in auto mode.
pub struct Pending { pub sid: String, pub plan: bool, tx: oneshot::Sender<Option<Value>> }

// Who answered: the computer (a page, a pet, a notification) or a phone.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum By { Pc, Phone }
// How a request ended. Answered(by, allowed); Expired: nobody answered in time, or Claude moved on; Released(why):
// handed back to Claude's own prompt ("settings": the pets were hidden or paused, "restart": Bobix is restarting).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum End { Answered(By, bool), Expired, Released(&'static str) }
// A closed request, kept CLOSED_KEEP_MS so a late answer is told why it is late, not just "no".
pub struct Closed { pub plan: bool, pub end: End, pub at: u64 }
const CLOSED_KEEP_MS: u64 = 30 * 60_000; // the contract is 20 minutes at least
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
fn full_text(e: &Value) -> String {
    let i = &e["tool_input"];
    let Some(m) = i.as_object() else { return String::new() };
    let text = |v: &Value| v.as_str().map(String::from).unwrap_or_else(|| v.to_string());
    let main = ["command", "file_path", "url", "pattern", "query", "plan"].into_iter().find(|k| m.get(*k).and_then(Value::as_str).is_some_and(|s| !s.is_empty()));
    let rest: Vec<String> = m.iter().filter(|(k, _)| Some(k.as_str()) != main).map(|(k, v)| format!("{k}: {}", text(v))).collect();
    let all = match main { Some(k) => std::iter::once(text(&m[k])).chain((!rest.is_empty()).then(|| rest.join("\n"))).collect::<Vec<_>>().join("\n\n"), None => rest.join("\n") };
    escaped(&all)
}

fn new_id() -> String {
    let mut b = [0u8; 16];
    let _ = getrandom::fill(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// Holds session `s`'s request when the person can see it (settings::hold_secs). Called with the core locked.
// plan: a ticket flow's plan, held until you answer whatever the pets show (it is the flow's one gate).
pub fn wait(b: &Arc<Bobix>, pending: &mut HashMap<String, Pending>, closed: &mut HashMap<String, Closed>, settings: &Value, s: &mut Session, e: &Value, plan: bool) -> Option<Reply> {
    let secs = if plan { f64::INFINITY } else { settings::hold_secs(settings, &s.pet) };
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
    pending.insert(id.clone(), Pending { sid: s.id.clone(), plan, tx });
    // Nobody answered in time, or Claude moved on (Esc, session closed: the hook's request closes).
    let (b1, id1) = (b.clone(), id.clone());
    tauri::async_runtime::spawn(async move {
        if forever { let _ = gone_rx.await; } else {
            tokio::select! { _ = gone_rx => {}, _ = tokio::time::sleep(Duration::from_secs_f64(secs)) => {} }
        }
        done(&b1, &id1, None, End::Expired);
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
            End::Expired => Answered::Expired, End::Released(why) => Answered::Released(why) },
    }
}

// An answer: allow | deny. On a ticket flow's plan, allow approves it (auto mode) and deny sends it back; `change`: what to
// change, from the Work screen. `expect`: Some(false) only a plain request, Some(true) only a plan; a page that shows what
// it asks about passes None. The first answer wins; the others are told why they are late.
pub fn respond(b: &Arc<Bobix>, id: &str, choice: &str, change: &str, by: By, expect: Option<bool>) -> Answered {
    if !matches!(choice, "allow" | "deny") { return Answered::Invalid; }
    let mut c = b.core.lock().unwrap();
    let plan = match c.pending.get(id) {
        Some(p) if expect.is_some_and(|e| e != p.plan) => return Answered::WrongKind,
        Some(p) => p.plan,
        None => return late(&c, id, expect),
    };
    let d = if plan { plan_decision(choice == "allow", change) } else { decision(choice).unwrap() };
    let end = End::Answered(by, choice == "allow");
    if !close(&mut c, id, Some(d), end, crate::state::now()) { return late(&c, id, expect); }
    drop(c);
    b.broadcast();
    Answered::Ok
}

// An answer from a page or a notification (the computer).
pub fn answer(b: &Arc<Bobix>, id: &str, choice: &str) -> bool { answer_with(b, id, choice, "") }
pub fn answer_with(b: &Arc<Bobix>, id: &str, choice: &str, change: &str) -> bool { respond(b, id, choice, change, By::Pc, None) == Answered::Ok }

// A held request whose pet is now hidden (or the pets paused / off) goes back to Claude, so it asks in its own UI.
pub fn release_unseen(b: &Arc<Bobix>) {
    let ids: Vec<String> = {
        let c = b.core.lock().unwrap();
        c.sessions.values().filter(|s| s.answer.as_ref().is_some_and(|a| a["plan"] != true) && settings::hold_secs(&c.settings, &s.pet) == 0.0)
            .filter_map(|s| s.answer.as_ref()?["id"].as_str().map(String::from)).collect()
    };
    for id in ids { done(b, &id, None, End::Released("settings")); }
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
        c.pending.insert(id.into(), Pending { sid: "s1".into(), plan, tx });
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
}
