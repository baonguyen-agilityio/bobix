//! A pretend phone for trying the desktop (or `fake-desktop`) without the phone app.
//!
//!   fake-phone --relay ws://127.0.0.1:8787 --qr 'bobix://pair?...' [--name "Pixel (demo)"]
//!
//! After pairing it prints what arrives and reads commands from standard input, one per line:
//!   detail <id> | preview <project> <iid> <full|quick> | answer <id> <allow|deny> | plan <id> <approve|back> [note]
//!   question <id> <answer>... | start <project> <iid> <full|quick> | stop <session> <held request id> | sync <cid>... | bye | quit
//! `answer`, `plan` and `question` send back the `shown` hash of the latest `detail` for that id (asking for it first).
//! It signs with a software key and prints no key material. It speaks plain `ws://` only.
mod common;

use bobix_link::messages::{Choice, CommandBody, Decision, Mode, PhoneMsg};
use bobix_link::pairing::Rights;
use common::phone::Phone;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn die(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1)
}

fn mode(s: &str) -> Mode {
    if s == "quick" { Mode::Quick } else { Mode::Full }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let relay = arg(&args, "--relay").unwrap_or_else(|| "ws://127.0.0.1:8787".into());
    let Some(qr) = arg(&args, "--qr") else { die("--qr '<QR string>' is required") };
    let name = arg(&args, "--name").unwrap_or_else(|| "Fake phone (demo)".into());
    let mut p = Phone::connect(&relay, &name).await.unwrap_or_else(|e| die(&e));
    p.print = true;
    let rights = Rights { answer: true, plan: true, question: true, start: true, stop: true, activity: true };
    let computer = p.pair(&qr, rights).await.unwrap_or_else(|e| die(&format!("pairing failed: {e}")));
    println!("paired with {computer}");
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Ok(Some(line)) = line else { break };
                let w: Vec<&str> = line.split_whitespace().collect();
                let r = match w.as_slice() {
                    ["quit"] => break,
                    ["bye"] => { let r = p.bye(&computer).await; p.settle(Duration::from_millis(300)).await; if r.is_ok() { break } r }
                    ["detail", id] => p.send(&computer, &PhoneMsg::Detail { id: id.to_string() }).await,
                    ["preview", project, iid, m] => p.send(&computer, &PhoneMsg::StartPreview { project: project.to_string(), iid: iid.to_string(), mode: mode(m) }).await,
                    ["sync", cids @ ..] => p.send(&computer, &PhoneMsg::Sync { waiting: cids.iter().map(|c| c.to_string()).collect(), versions: bobix_link::envelope::VersionRange::current() }).await,
                    ["stop", s, req] => p.command(&computer, CommandBody::Stop { session: s.to_string(), request: req.to_string() }).await.map(|c| println!("cid {c}")),
                    ["start", project, iid, m] => {
                        if p.prompt_hash_for(project, iid).is_none() {
                            let _ = p.send(&computer, &PhoneMsg::StartPreview { project: project.to_string(), iid: iid.to_string(), mode: mode(m) }).await;
                            p.settle(Duration::from_millis(500)).await;
                        }
                        let h = p.prompt_hash_for(project, iid).unwrap_or_default();
                        p.command(&computer, CommandBody::Start { project: project.to_string(), iid: iid.to_string(), mode: mode(m), prompt_hash: h }).await.map(|c| println!("cid {c}"))
                    }
                    [kind @ ("answer" | "plan" | "question"), id, rest @ ..] => {
                        if p.shown_for(id).is_none() {
                            let _ = p.send(&computer, &PhoneMsg::Detail { id: id.to_string() }).await;
                            p.settle(Duration::from_millis(500)).await;
                        }
                        let shown = p.shown_for(id).unwrap_or_default();
                        let id = id.to_string();
                        let body = match (*kind, rest) {
                            ("answer", [c]) => CommandBody::Answer { id, choice: if *c == "deny" { Choice::Deny } else { Choice::Allow }, shown },
                            ("plan", [d, note @ ..]) => CommandBody::Plan { id, decision: if *d == "back" { Decision::Back } else { Decision::Approve }, note: (!note.is_empty()).then(|| note.join(" ")), shown },
                            ("question", a) => CommandBody::Question { id, answers: a.iter().map(|x| x.to_string()).collect(), shown },
                            _ => { eprintln!("usage: see the top of fake-phone.rs"); continue }
                        };
                        p.command(&computer, body).await.map(|c| println!("cid {c}"))
                    }
                    [] => continue,
                    _ => { eprintln!("unknown command; see the top of fake-phone.rs"); continue }
                };
                if let Err(e) = r { eprintln!("{e}") }
            }
            _ = p.pump(Duration::from_millis(200)) => {}
        }
    }
}
