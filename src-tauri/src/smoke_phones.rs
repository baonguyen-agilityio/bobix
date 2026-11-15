// The smoke run with phone access: the real app is driven through switching on, pairing, rights and disconnecting by
// `fake-phone` (crates/bobix-link/examples), against `relay --dev`. BOBIX_SMOKE_PHONES="<relay ws url>|<fake-phone binary>".
// Made-up data only. Because BOBIX_DATA_DIR is set the keys live in a file inside it: the OS keychain is never touched.
use crate::{phones, Bobix};
use serde_json::{json, Value};
use std::{io::{BufRead, BufReader, Write}, process::{Child, ChildStdin, Command, Stdio}, sync::{Arc, Mutex}, thread, time::{Duration, Instant}};

pub struct Fake { child: Child, stdin: ChildStdin, pub lines: Arc<Mutex<Vec<String>>> }

impl Fake {
    pub fn spawn(bin: &str, relay: &str, qr: &str, name: &str) -> Option<Fake> {
        let mut child = Command::new(bin).args(["--relay", relay, "--qr", qr, "--name", name]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().ok()?;
        let lines = Arc::new(Mutex::new(vec![]));
        for out in [child.stdout.take().map(|o| Box::new(o) as Box<dyn std::io::Read + Send>), child.stderr.take().map(|o| Box::new(o) as Box<dyn std::io::Read + Send>)].into_iter().flatten() {
            let l = lines.clone();
            thread::spawn(move || for line in BufReader::new(out).lines().map_while(Result::ok) { l.lock().unwrap().push(line); });
        }
        let stdin = child.stdin.take()?;
        Some(Fake { child, stdin, lines })
    }
    pub fn say(&mut self, line: &str) { let _ = writeln!(self.stdin, "{line}"); let _ = self.stdin.flush(); }
    pub fn saw(&self, needle: &str) -> bool { self.lines.lock().unwrap().iter().any(|l| l.contains(needle)) }
    pub fn stop(mut self) { let _ = self.child.kill(); let _ = self.child.wait(); }
}

pub fn until(secs: u64, mut f: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end { if f() { return true; } thread::sleep(Duration::from_millis(100)); }
    f()
}

// Holds a permission request of session `sid` as Claude's hook would; returns the hook's output (when it ends) and the request id.
fn hold(b: &Arc<Bobix>, sid: &str, command: &str) -> (thread::JoinHandle<String>, String) { hold_tool(b, sid, "Bash", json!({ "command": command })) }
fn hold_tool(b: &Arc<Bobix>, sid: &str, tool: &str, input: Value) -> (thread::JoinHandle<String>, String) {
    let ask = json!({ "session_id": sid, "cwd": "/tmp/demo-app", "hook_event_name": "PermissionRequest", "tool_name": tool, "tool_input": input }).to_string();
    let (hook, port) = (crate::hooks::hook_target(&crate::paths::home()), b.port);
    let h = thread::spawn(move || crate::smoke::run_hook_out(&hook, port, &ask));
    let mut got = None;
    until(10, || { got = b.core.lock().unwrap().pending.iter().find(|(_, p)| p.sid == sid).map(|(k, _)| k.clone()); got.is_some() });
    (h, got.unwrap_or_default())
}
// The same with the hook's process in hand, so a test can end it the way Claude's own prompt being answered does.
fn hold_child(b: &Arc<Bobix>, sid: &str, command: &str) -> (Child, String) {
    let ask = json!({ "session_id": sid, "cwd": "/tmp/demo-app", "hook_event_name": "PermissionRequest", "tool_name": "Bash", "tool_input": { "command": command } }).to_string();
    let hook = crate::hooks::hook_target(&crate::paths::home());
    let mut child = Command::new("node").arg(hook).arg(b.port.to_string()).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().expect("node");
    child.stdin.take().unwrap().write_all(ask.as_bytes()).ok();
    let mut got = None;
    until(10, || { got = b.core.lock().unwrap().pending.iter().find(|(_, p)| p.sid == sid).map(|(k, _)| k.clone()); got.is_some() });
    (child, got.unwrap_or_default())
}
fn outcomes(f: &Fake, what: &str) -> usize { f.lines.lock().unwrap().iter().filter(|l| l.contains("\"kind\":\"result\"") && l.contains(what)).count() }

pub fn run(app: &tauri::AppHandle, b: &Arc<Bobix>, spec: &str) -> Value {
    let mut r = json!({});
    let Some((relay, bin)) = spec.split_once('|') else { return json!({ "pass": false, "error": "spec" }) };
    let v = || phones::view(b);
    // From a fresh start with the switch off: no connection, no identity.
    r["offAtStart"] = json!(v()["on"] == false && v()["linked"] == false && v()["deviceId"].is_null());
    r["relaySet"] = json!(phones::set_relay(b, relay).is_ok());
    r["on"] = json!(phones::enable(b, true).is_ok() && until(15, || v()["status"] == "online"));
    r["status"] = v()["status"].clone();
    r["deviceId"] = json!(v()["deviceId"].as_str().is_some_and(|d| d.len() == 32));
    let uri = phones::pair_start(b).ok().and_then(|x| x["pairing"]["uri"].as_str().map(String::from)).unwrap_or_default();
    r["qr"] = json!(uri.starts_with("bobix://") && v()["pairing"]["qr"].as_str().is_some_and(|s| s.contains("<svg")));
    let Some(mut fake) = Fake::spawn(bin, relay, &uri, "Demo <b>phone</b>") else { r["pass"] = json!(false); r["error"] = json!("fake-phone"); return r };
    r["offered"] = json!(until(15, || v()["pairing"]["state"] == "offered") && v()["pairing"]["name"] == "Demo <b>phone</b>");
    // nothing works before the owner confirms; start and stop stay unticked
    r["notBeforeConfirm"] = json!(v()["phones"].as_array().is_some_and(|a| a.is_empty()));
    let granted = phones::rights_of(&json!({ "answer": true, "plan": true, "question": true, "start": false, "stop": true, "activity": true }));
    r["confirmed"] = json!(phones::pair_confirm(b, granted).is_ok() && until(10, || v()["phones"].as_array().is_some_and(|a| a.len() == 1)));
    let phone = v()["phones"][0].clone();
    r["rights"] = json!(phone["rights"]["answer"] == true && phone["rights"]["stop"] == true && phone["rights"]["start"] == false && phone["rights"]["activity"] == true);
    // Settings → Phones through the real panel and IPC: the section draws, the commands are granted to the page, and a name
    // with markup is shown as text.
    r["panel"] = crate::smoke::ask_for(app, crate::panel::LABEL, "(async () => { bsel.settings = 'set:phones'; showTab('settings'); \
        await new Promise(r => setTimeout(r, 2500)); const st = await bridge.phonesState(); const n = document.querySelector('[data-ph-name]'); \
        return { section: !!document.querySelector('#sec-phones'), ipc: st.on === true && st.phones.length === 1, name: n ? n.textContent : null, noMarkup: n ? n.children.length === 0 : false }; })()", 12);
    r["panelOk"] = json!(r["panel"]["section"] == true && r["panel"]["ipc"] == true && r["panel"]["name"] == "Demo <b>phone</b>" && r["panel"]["noMarkup"] == true);
    r["pairedSeen"] = json!(until(10, || fake.saw("paired with")));
    r["confirmWithoutPairing"] = json!(phones::pair_confirm(b, granted).is_err());
    // What a phone sees: a request held in the app reaches it as a redacted preview, a notice and, on asking, the whole thing.
    let token = format!("ghp_{}", "a1B2c3D4e5F6g7H8i9J0k1L2m3N4");
    let ask = json!({ "session_id": "ph1", "cwd": "/tmp/demo-app", "hook_event_name": "PermissionRequest", "tool_name": "Bash",
        "tool_input": { "command": format!("ls \u{202e}gpj.sh && echo {token}"), "description": "list" } }).to_string();
    let (hook, port) = (crate::hooks::hook_target(&crate::paths::home()), b.port);
    let held = thread::spawn(move || crate::smoke::run_hook_out(&hook, port, &ask));
    let rid = { let mut got = None; until(10, || { got = b.core.lock().unwrap().pending.iter().find(|(_, p)| p.sid == "ph1").map(|(k, _)| k.clone()); got.is_some() }); got.unwrap_or_default() };
    r["homeSeen"] = json!(until(10, || fake.lines.lock().unwrap().iter().any(|l| l.contains("\"kind\":\"home\"") && l.contains(&rid))));
    r["noticeSeen"] = json!(until(10, || fake.lines.lock().unwrap().iter().any(|l| l.contains("\"kind\":\"notice\"") && l.contains("\"what\":\"waiting\"") && l.contains(&rid))));
    fake.say(&format!("detail {rid}"));
    r["detailSeen"] = json!(until(10, || fake.lines.lock().unwrap().iter().any(|l| l.contains("\"kind\":\"detail\"") && l.contains("\\\\u202E") && l.contains("\"odd\":true"))));
    // nothing secret or hidden reaches the phone in a preview or a notice, and no forbidden field anywhere
    let all = fake.lines.lock().unwrap().join("\n");
    let previews: String = fake.lines.lock().unwrap().iter().filter(|l| l.contains("\"kind\":\"home\"") || l.contains("\"kind\":\"notice\"")).cloned().collect();
    r["redacted"] = json!(!previews.contains(&token) && !previews.contains('\u{202e}') && !all.contains("transcript") && !all.contains("/tmp/demo-app"));
    crate::answers::answer(b, &rid, "allow");
    r["closedSeen"] = json!(until(10, || fake.lines.lock().unwrap().iter().any(|l| l.contains("\"what\":\"closed\""))));
    r["hookAnswered"] = json!(held.join().unwrap_or_default().contains("\"behavior\":\"allow\""));
    // Pets hidden and nobody at the computer (the input time is fixed under the smoke run): a request is held for the phones,
    // and a phone allows it, another it denies.
    crate::update_settings(b, json!({ "petsVisible": false }));
    let (h1, r1) = hold(b, "ph2", "echo allow me");
    r["heldForPhones"] = json!(b.core.lock().unwrap().pending.get(&r1).is_some_and(|p| p.for_phones));
    fake.say(&format!("answer {r1} allow"));
    r["phoneAllowed"] = json!(until(10, || outcomes(&fake, "\"outcome\":\"ok\"") >= 1) && h1.join().unwrap_or_default().contains("\"behavior\":\"allow\""));
    let (h2, r2) = hold(b, "ph3", "echo deny me");
    fake.say(&format!("answer {r2} deny"));
    r["phoneDenied"] = json!(until(10, || outcomes(&fake, "\"outcome\":\"ok\"") >= 2) && h2.join().unwrap_or_default().contains("\"behavior\":\"deny\""));
    // answered at the computer first: the phone is told it was late
    let (h3, r3) = hold(b, "ph4", "echo computer first");
    fake.say(&format!("detail {r3}"));
    until(10, || fake.lines.lock().unwrap().iter().any(|l| l.contains("\"kind\":\"detail\"") && l.contains("computer first")));
    crate::answers::answer(b, &r3, "allow");
    fake.say(&format!("answer {r3} allow"));
    r["latePc"] = json!(until(10, || outcomes(&fake, "\"outcome\":\"latePc\"") >= 1) && h3.join().unwrap_or_default().contains("allow"));
    // answered in Claude's own prompt while a phone could still answer: the hook closes, the phone is told it was late
    let (mut child, r5) = hold_child(b, "ph6", "echo terminal first");
    fake.say(&format!("detail {r5}"));
    until(10, || fake.lines.lock().unwrap().iter().any(|l| l.contains("\"kind\":\"detail\"") && l.contains("terminal first")));
    if let Some(p) = b.core.lock().unwrap().pending.get_mut(&r5) { p.released = true; } // someone is at the computer (the input time is fixed here)
    b.broadcast();
    r["atComputerSeen"] = json!(until(10, || fake.lines.lock().unwrap().iter().any(|l| l.contains("\"kind\":\"home\"") && l.contains(&r5) && l.contains("\"atComputer\":true"))));
    let _ = child.kill();
    let _ = child.wait();
    until(10, || !b.core.lock().unwrap().pending.contains_key(&r5));
    fake.say(&format!("answer {r5} allow"));
    r["latePcTerminal"] = json!(until(10, || outcomes(&fake, "\"outcome\":\"latePc\"") >= 2));
    r["logged"] = json!(phones::log(b).as_array().is_some_and(|a| a.len() >= 3 && a.iter().all(|l| !l["what"].as_str().unwrap_or("").contains("ghp_"))));
    // Claude's questions: an option and free text come back to Claude in the shape the spike recorded
    let q = |n: &str| json!({ "questions": [{ "question": n, "header": "H", "options": [{ "label": "SQLite", "description": "d" }, { "label": "Postgres", "description": "d" }], "multiSelect": false }] });
    let (hq1, q1) = hold_tool(b, "ph7", "AskUserQuestion", q("Which database?"));
    r["questionHeld"] = json!(b.core.lock().unwrap().pending.get(&q1).is_some_and(|p| p.kind() == crate::answers::Kind::Question));
    fake.say(&format!("detail {q1}"));
    until(10, || fake.lines.lock().unwrap().iter().any(|l| l.contains("\"what\":\"question\"") && l.contains("SQLite")));
    fake.say(&format!("question {q1} SQLite"));
    let out1 = hq1.join().unwrap_or_default();
    r["questionOption"] = json!(out1.contains("\"answers\":{\"Which database?\":\"SQLite\"}") && out1.contains("\"behavior\":\"allow\"") && out1.contains("\"questions\""));
    let (hq2, q2) = hold_tool(b, "ph8", "AskUserQuestion", q("Pick one"));
    fake.say(&format!("detail {q2}"));
    until(10, || fake.lines.lock().unwrap().iter().any(|l| l.contains("Pick one") && l.contains("\"what\":\"question\"")));
    fake.say(&format!("question {q2} my_own_words"));
    r["questionFreeText"] = json!(hq2.join().unwrap_or_default().contains("\"Pick one\":\"my_own_words\""));
    // Stop: a held request is denied with interrupt; the session stays
    let (hs, rs) = hold(b, "ph9", "sleep 99");
    fake.say(&format!("stop ph9 {rs}"));
    let outs = hs.join().unwrap_or_default();
    r["stopped"] = json!(outs.contains("\"behavior\":\"deny\"") && outs.contains("\"interrupt\":true") && b.core.lock().unwrap().sessions.contains_key("ph9"));
    let (hs2, rs2) = hold(b, "ph10", "sleep 98");
    fake.say(&format!("stop ph9 {rs2}")); // another session's name: refused, the request stays held
    thread::sleep(Duration::from_secs(2));
    r["staleStop"] = json!(b.core.lock().unwrap().pending.contains_key(&rs2));
    crate::answers::answer(b, &rs2, "deny");
    let _ = hs2.join();
    // Last lines: a session's recent tool lines, redacted, without its prompt
    for (ev, extra) in [("UserPromptSubmit", json!({ "prompt": "SECRET PROMPT TEXT" })), ("PreToolUse", json!({ "tool_name": "Bash", "tool_input": { "command": format!("deploy --key {token}") } }))] {
        let mut e = json!({ "session_id": "ph1", "cwd": "/tmp/demo-app", "hook_event_name": ev });
        for (k, v) in extra.as_object().unwrap() { e[k] = v.clone(); }
        crate::smoke::run_hook_out(&crate::hooks::hook_target(&crate::paths::home()), b.port, &e.to_string());
    }
    fake.say("detail ph1");
    r["lastLines"] = json!(until(10, || fake.lines.lock().unwrap().iter().any(|l| l.contains("\"what\":\"lastLines\"") && l.contains("deploy --key") && !l.contains(&token) && !l.contains("SECRET PROMPT"))));
    // a disconnected phone's next command is refused: the request stays held
    let (h4, r4) = hold(b, "ph5", "echo after disconnect");
    let pid = phones::view(b)["phones"][0]["id"].as_str().unwrap_or("").to_string();
    phones::disconnect(b, &pid).ok();
    fake.say(&format!("answer {r4} allow"));
    thread::sleep(Duration::from_secs(2));
    r["refusedAfterDisconnect"] = json!(b.core.lock().unwrap().pending.contains_key(&r4));
    crate::answers::answer(b, &r4, "deny");
    let _ = h4.join();
    crate::update_settings(b, json!({ "petsVisible": true }));
    r["disconnected"] = json!(v()["phones"].as_array().is_some_and(|a| a.is_empty()));
    r["told"] = json!(until(10, || fake.saw("revoked") || fake.saw("Revoked")));
    if std::env::var_os("BOBIX_SMOKE_DEBUG").is_some() { r["lines"] = json!(fake.lines.lock().unwrap().iter().map(|l| l.chars().take(260).collect::<String>()).collect::<Vec<_>>()); }
    fake.stop();
    r["off"] = json!(phones::enable(b, false).is_ok() && until(20, || v()["status"] == "off" && v()["linked"] == false));
    r["pass"] = json!(["offAtStart", "relaySet", "on", "deviceId", "qr", "offered", "notBeforeConfirm", "confirmed", "rights", "pairedSeen", "homeSeen", "noticeSeen", "detailSeen", "redacted", "panelOk", "closedSeen", "hookAnswered", "atComputerSeen", "latePcTerminal", "questionHeld", "questionOption", "questionFreeText", "stopped", "staleStop", "lastLines", "heldForPhones", "phoneAllowed", "phoneDenied", "latePc", "logged", "refusedAfterDisconnect", "confirmWithoutPairing", "disconnected", "off"].iter().all(|k| r[*k] == true));
    r
}
