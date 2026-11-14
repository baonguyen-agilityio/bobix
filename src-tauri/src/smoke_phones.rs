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
    r["disconnected"] = json!(phones::disconnect(b, &id).is_ok() && v()["phones"].as_array().is_some_and(|a| a.is_empty()));
    r["told"] = json!(until(10, || fake.saw("revoked") || fake.saw("Revoked")));
    fake.stop();
    r["off"] = json!(phones::enable(b, false).is_ok() && until(20, || v()["status"] == "off" && v()["linked"] == false));
    r["pass"] = json!(["offAtStart", "relaySet", "on", "deviceId", "qr", "offered", "notBeforeConfirm", "confirmed", "rights", "pairedSeen", "confirmWithoutPairing", "disconnected", "off"].iter().all(|k| r[*k] == true));
    r
}
