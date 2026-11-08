// Permission requests held in Bobix until someone answers them on a pet, the panel or a notification. Port of
// waitForAnswer / answer / releaseUnseen in the 0.1 (Electron) app's main.js.
use crate::{always, server::Reply, settings, state::Session, Bobix};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tokio::sync::oneshot;

// rule: what "Always allow" remembers for this request (always::rule_for), None when it may not be remembered.
pub struct Pending { pub sid: String, rule: Option<Value>, tx: oneshot::Sender<Option<Value>> }

// "always" allows like "allow"; the rule is kept in Bobix's settings (answer), never as Claude's own permission rules.
fn decision(choice: &str, rule: &Option<Value>) -> Option<Value> {
    Some(match choice {
        "allow" => json!({ "behavior": "allow" }),
        "always" if rule.is_some() => json!({ "behavior": "allow" }),
        "deny" => json!({ "behavior": "deny", "message": "Denied from Bobix." }),
        _ => return None,
    })
}
// What the hook prints for Claude Code.
pub fn output(d: Value) -> Value { json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": d } }) }

// The request's main text, whole (the session keeps a short one): the command, file, address… else its input.
fn full_text(e: &Value) -> String {
    let i = &e["tool_input"];
    let t = ["command", "file_path", "url", "pattern", "query"].iter().find_map(|k| i[*k].as_str().filter(|s| !s.is_empty()))
        .map(String::from).unwrap_or_else(|| if i.is_object() { i.to_string() } else { String::new() });
    crate::js::head16(&t, 2000).to_string()
}

fn new_id() -> String {
    let mut b = [0u8; 16];
    let _ = getrandom::fill(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// Holds session `s`'s request when the person can see it (settings::hold_secs). Called with the core locked.
pub fn wait(b: &Arc<Bobix>, pending: &mut std::collections::HashMap<String, Pending>, settings: &Value, s: &mut Session, e: &Value) -> Option<Reply> {
    let secs = settings::hold_secs(settings, &s.pet);
    if secs == 0.0 { return None; }
    // one open request per session
    if let Some(old) = s.answer.as_ref().and_then(|a| a["id"].as_str()).and_then(|id| pending.remove(id)) { let _ = old.tx.send(None); }
    let id = new_id();
    let rule = always::rule_for(&crate::project::project_of(&s.cwd), e);
    let risky = always::risky(e["tool_name"].as_str().unwrap_or(""), &e["tool_input"]);
    let (tx, rx) = oneshot::channel();
    let (gone_tx, gone_rx) = oneshot::channel::<()>();
    let forever = secs.is_infinite();
    // until: null = no countdown. always: "Always allow" is offered. risky: it deletes or overwrites (a warning).
    s.answer = Some(json!({ "id": id, "until": if forever { Value::Null } else { json!(crate::state::now() + (secs * 1000.0) as u64) },
        "always": rule.is_some(), "risky": risky, "full": full_text(e) }));
    pending.insert(id.clone(), Pending { sid: s.id.clone(), rule, tx });
    // Nobody answered in time, or Claude moved on (Esc, session closed: the hook's request closes).
    let (b1, id1) = (b.clone(), id.clone());
    tauri::async_runtime::spawn(async move {
        if forever { let _ = gone_rx.await; } else {
            tokio::select! { _ = gone_rx => {}, _ = tokio::time::sleep(Duration::from_secs_f64(secs)) => {} }
        }
        done(&b1, &id1, None);
    });
    Some(Reply::Answer { secs: if forever { "forever".into() } else { (secs as u64).to_string() }, rx, gone: gone_tx })
}

// Ends a held request: with a decision for Claude, or None (Claude asks in its own UI).
pub fn done(b: &Arc<Bobix>, id: &str, decision: Option<Value>) {
    {
        let mut c = b.core.lock().unwrap();
        let Some(p) = c.pending.remove(id) else { return };
        if let Some(s) = c.sessions.get_mut(&p.sid) {
            if s.answer.as_ref().is_some_and(|a| a["id"] == id) { s.answer = None; }
            if decision.is_some() { s.ask.clear(); s.ask_tool.clear(); s.state = "working".into(); s.since = crate::state::now(); }
        }
        let _ = p.tx.send(decision.map(output));
    }
    b.broadcast();
}

// An answer from a page or a notification: allow | always | deny. "always" also remembers the rule (Bobix's settings).
pub fn answer(b: &Arc<Bobix>, id: &str, choice: &str) -> bool {
    let (d, rule) = { let c = b.core.lock().unwrap(); c.pending.get(id).map(|p| (decision(choice, &p.rule), p.rule.clone())).unwrap_or((None, None)) };
    let Some(d) = d else { return false };
    if choice == "always" { if let Some(r) = rule { crate::remember(b, r); } }
    done(b, id, Some(d));
    true
}

// A held request whose pet is now hidden (or the pets paused / off) goes back to Claude, so it asks in its own UI.
pub fn release_unseen(b: &Arc<Bobix>) {
    let ids: Vec<String> = {
        let c = b.core.lock().unwrap();
        c.sessions.values().filter(|s| s.answer.is_some() && settings::hold_secs(&c.settings, &s.pet) == 0.0)
            .filter_map(|s| s.answer.as_ref()?["id"].as_str().map(String::from)).collect()
    };
    for id in ids { done(b, &id, None); }
}

#[cfg(test)]
mod tests {
    #[test]
    fn decisions_match_the_electron_build() {
        let s = Some(serde_json::json!({ "project": "/p", "tool": "Bash", "input": "{}" }));
        assert_eq!(super::decision("allow", &s).unwrap(), serde_json::json!({ "behavior": "allow" }));
        assert_eq!(super::decision("always", &s).unwrap(), serde_json::json!({ "behavior": "allow" }), "never Claude's own permission rules");
        assert!(super::decision("always", &None).is_none(), "not offered: not taken");
        assert_eq!(super::decision("deny", &s).unwrap()["message"], "Denied from Bobix.");
        assert!(super::decision("maybe", &s).is_none());
        let e = serde_json::json!({ "tool_input": { "command": "npm test", "description": "d" } });
        assert_eq!(super::full_text(&e), "npm test");
        assert_eq!(super::full_text(&serde_json::json!({ "tool_input": { "a": 1 } })), "{\"a\":1}");
        assert_eq!(super::new_id().len(), 32);
    }
}
