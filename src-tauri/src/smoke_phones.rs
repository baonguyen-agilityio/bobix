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

pub fn run(b: &Arc<Bobix>, spec: &str) -> Value {
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
    let Some(mut fake) = Fake::spawn(bin, relay, &uri, "Demo phone") else { r["pass"] = json!(false); r["error"] = json!("fake-phone"); return r };
    r["offered"] = json!(until(15, || v()["pairing"]["state"] == "offered") && v()["pairing"]["name"] == "Demo phone");
    // nothing works before the owner confirms; start and stop stay unticked
    r["notBeforeConfirm"] = json!(v()["phones"].as_array().is_some_and(|a| a.is_empty()));
    let granted = phones::rights_of(&json!({ "answer": true, "plan": true, "question": true, "start": false, "stop": false, "activity": false }));
    r["confirmed"] = json!(phones::pair_confirm(b, granted).is_ok() && until(10, || v()["phones"].as_array().is_some_and(|a| a.len() == 1)));
    let phone = v()["phones"][0].clone();
    r["rights"] = json!(phone["rights"]["answer"] == true && phone["rights"]["stop"] == false && phone["rights"]["start"] == false && phone["rights"]["activity"] == false);
    r["pairedSeen"] = json!(until(10, || fake.saw("paired with")));
    r["confirmWithoutPairing"] = json!(phones::pair_confirm(b, granted).is_err());
    let id = phone["id"].as_str().unwrap_or("").to_string();
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
    r["detailSeen"] = json!(until(10, || fake.lines.lock().unwrap().iter().any(|l| l.contains("\"kind\":\"detail\"") && l.contains("\\u{202e}") && l.contains("\"odd\":true"))));
    // nothing secret or hidden reaches the phone in a preview or a notice, and no forbidden field anywhere
    let all = fake.lines.lock().unwrap().join("\n");
    let previews: String = fake.lines.lock().unwrap().iter().filter(|l| l.contains("\"kind\":\"home\"") || l.contains("\"kind\":\"notice\"")).cloned().collect();
    r["redacted"] = json!(!previews.contains(&token) && !previews.contains('\u{202e}') && !all.contains("transcript") && !all.contains("/tmp/demo-app"));
    crate::answers::answer(b, &rid, "allow");
    r["closedSeen"] = json!(until(10, || fake.lines.lock().unwrap().iter().any(|l| l.contains("\"what\":\"closed\""))));
    r["hookAnswered"] = json!(held.join().unwrap_or_default().contains("\"behavior\":\"allow\""));
    r["disconnected"] = json!(phones::disconnect(b, &id).is_ok() && v()["phones"].as_array().is_some_and(|a| a.is_empty()));
    r["told"] = json!(until(10, || fake.saw("revoked") || fake.saw("Revoked")));
    fake.stop();
    r["off"] = json!(phones::enable(b, false).is_ok() && until(20, || v()["status"] == "off" && v()["linked"] == false));
    r["pass"] = json!(["offAtStart", "relaySet", "on", "deviceId", "qr", "offered", "notBeforeConfirm", "confirmed", "rights", "pairedSeen", "homeSeen", "noticeSeen", "detailSeen", "redacted", "closedSeen", "hookAnswered", "confirmWithoutPairing", "disconnected", "off"].iter().all(|k| r[*k] == true));
    r
}
