// Permission requests held in Bobix until someone answers them on a pet, the panel or a notification. Port of
// waitForAnswer / answer / releaseUnseen in the 0.1 (Electron) app's main.js.
use crate::{server::Reply, settings, state::Session, Bobix};
use fancy_regex::Regex;
use serde_json::{json, Value};
use std::{sync::{Arc, LazyLock}, time::Duration};
use tokio::sync::oneshot;

// plan: a ticket flow's plan (ExitPlanMode), held until answered; approving it puts that one session in auto mode.
pub struct Pending { pub sid: String, pub plan: bool, tx: oneshot::Sender<Option<Value>> }

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
// plan: a ticket flow's plan, held until you answer whatever the pets show (it is the flow's one gate).
pub fn wait(b: &Arc<Bobix>, pending: &mut std::collections::HashMap<String, Pending>, settings: &Value, s: &mut Session, e: &Value, plan: bool) -> Option<Reply> {
    let secs = if plan { f64::INFINITY } else { settings::hold_secs(settings, &s.pet) };
    if secs == 0.0 { return None; }
    // one open request per session
    if let Some(old) = s.answer.as_ref().and_then(|a| a["id"].as_str()).and_then(|id| pending.remove(id)) { let _ = old.tx.send(None); }
    let id = new_id();
    let risky = risky(e["tool_name"].as_str().unwrap_or(""), &e["tool_input"]);
    let (tx, rx) = oneshot::channel();
    let (gone_tx, gone_rx) = oneshot::channel::<()>();
    let forever = secs.is_infinite();
    // until: null = no countdown. risky: it deletes or overwrites (the panel warns). full: the request's text, whole.
    s.answer = Some(json!({ "id": id, "until": if forever { Value::Null } else { json!(crate::state::now() + (secs * 1000.0) as u64) },
        "risky": risky, "full": full_text(e), "plan": plan }));
    pending.insert(id.clone(), Pending { sid: s.id.clone(), plan, tx });
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
        if p.plan && decision.is_some() { let ok = decision.as_ref().is_some_and(|d| d["behavior"] == "allow"); c.tickets.answered(&p.sid, ok, crate::state::now()); c.tickets.save(); }
        if let Some(s) = c.sessions.get_mut(&p.sid) {
            if s.answer.as_ref().is_some_and(|a| a["id"] == id) { s.answer = None; }
            if decision.is_some() { s.ask.clear(); s.ask_tool.clear(); s.state = "working".into(); s.since = crate::state::now(); }
        }
        let _ = p.tx.send(decision.map(output));
    }
    b.broadcast();
}

// An answer from a page or a notification: allow | deny. On a ticket flow's plan, allow approves it (auto mode) and
// deny sends it back; `change`: what to change, from the Work screen.
pub fn answer(b: &Arc<Bobix>, id: &str, choice: &str) -> bool { answer_with(b, id, choice, "") }
pub fn answer_with(b: &Arc<Bobix>, id: &str, choice: &str, change: &str) -> bool {
    let d = { let c = b.core.lock().unwrap(); c.pending.get(id).and_then(|p| if p.plan && matches!(choice, "allow" | "deny") { Some(plan_decision(choice == "allow", change)) } else { decision(choice) }) };
    let Some(d) = d else { return false };
    done(b, id, Some(d));
    true
}

// A held request whose pet is now hidden (or the pets paused / off) goes back to Claude, so it asks in its own UI.
pub fn release_unseen(b: &Arc<Bobix>) {
    let ids: Vec<String> = {
        let c = b.core.lock().unwrap();
        c.sessions.values().filter(|s| s.answer.as_ref().is_some_and(|a| a["plan"] != true) && settings::hold_secs(&c.settings, &s.pet) == 0.0)
            .filter_map(|s| s.answer.as_ref()?["id"].as_str().map(String::from)).collect()
    };
    for id in ids { done(b, &id, None); }
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
        assert_eq!(super::full_text(&e), "npm test");
        assert_eq!(super::full_text(&serde_json::json!({ "tool_input": { "a": 1 } })), "{\"a\":1}");
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
}
