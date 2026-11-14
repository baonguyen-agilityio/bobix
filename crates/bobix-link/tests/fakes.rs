//! Runs fake-desktop and fake-phone against the in-process relay stub: one test per scenario, each checking that the
//! scenario ends in the state its name says.

#[path = "../examples/common/mod.rs"]
mod common;

use bobix_link::messages::*;
use bobix_link::pairing::Rights;
use bobix_link::relay_frames::{ErrorCode, Frame, PresenceState};
use common::{desktop::Desktop, phone::Phone, scenario::{Scenario, ALL}, stub};
use std::time::Duration;
use tokio::task::JoinHandle;

const WAIT: Duration = Duration::from_secs(5);

fn all_rights() -> Rights {
    Rights { answer: true, plan: true, question: true, start: true, stop: true, activity: true }
}

struct Run {
    phone: Phone,
    computer: String,
    desktop: JoinHandle<Vec<String>>,
}

async fn desktop_on(relay: &str, s: Scenario) -> (String, JoinHandle<Vec<String>>) {
    let d = Desktop::start(relay, s).await.unwrap();
    let qr = d.qr();
    (qr, tokio::spawn(d.serve(Duration::from_secs(20))))
}

async fn run(s: Scenario) -> Run {
    run_with(s, all_rights()).await
}

async fn run_with(s: Scenario, rights: Rights) -> Run {
    let relay = stub::spawn().await;
    let (qr, desktop) = desktop_on(&relay, s).await;
    let mut phone = Phone::connect(&relay, "Test phone (demo)").await.unwrap();
    let computer = phone.pair(&qr, rights).await.unwrap();
    Run { phone, computer, desktop }
}

impl Run {
    async fn finish(mut self) -> Vec<String> {
        let _ = self.phone.bye(&self.computer).await;
        self.desktop.await.unwrap()
    }

    async fn detail(&mut self, id: &str) -> Detail {
        let c = self.computer.clone();
        self.phone.send(&c, &PhoneMsg::Detail { id: id.into() }).await.unwrap();
        match self.phone.wait_for(WAIT, |m| matches!(m, DesktopMsg::Detail(d) if d.id == id)).await {
            Some(DesktopMsg::Detail(d)) => d,
            other => panic!("no detail for {id}: {other:?}"),
        }
    }

    async fn result_of(&mut self, cid: &str) -> CmdResult {
        match self.phone.wait_for(WAIT, |m| matches!(m, DesktopMsg::Result(r) if r.cid == cid)).await {
            Some(DesktopMsg::Result(r)) => r,
            other => panic!("no result for {cid}: {other:?}"),
        }
    }

    /// Looks at the item (so `shown` is what the phone saw), then answers.
    async fn answer(&mut self, id: &str, choice: Choice) -> CmdResult {
        let shown = self.detail(id).await.shown;
        let c = self.computer.clone();
        let cid = self.phone.command(&c, CommandBody::Answer { id: id.into(), choice, shown }).await.unwrap();
        self.result_of(&cid).await
    }

    async fn start(&mut self) -> (StartPrompt, CmdResult) {
        let c = self.computer.clone();
        self.phone.send(&c, &PhoneMsg::StartPreview { project: "demo-app".into(), iid: "12".into(), mode: Mode::Full }).await.unwrap();
        let Some(DesktopMsg::StartPrompt(p)) = self.phone.wait_for(WAIT, |m| matches!(m, DesktopMsg::StartPrompt(_))).await else { panic!("no preview") };
        let cid = self
            .phone
            .command(&c, CommandBody::Start { project: p.project.clone(), iid: p.iid.clone(), mode: Mode::Full, prompt_hash: p.prompt_hash.clone() })
            .await
            .unwrap();
        let r = self.result_of(&cid).await;
        (p, r)
    }
}

fn refused(why: &str) -> impl Fn(&CmdResult) -> bool + '_ {
    move |r| r.outcome == Outcome::Refused && r.why.as_deref() == Some(why)
}

#[tokio::test]
async fn a_phone_pairs_and_one_command_gets_one_result() {
    let mut r = run(Scenario::Normal).await;
    let Some(DesktopMsg::Home(h)) = r.phone.wait_for(WAIT, |m| matches!(m, DesktopMsg::Home(_))).await else { panic!("no home") };
    assert_eq!(h.computer.state, ComputerState::Online);
    assert!(h.waiting.iter().any(|w| w.id == "req1" && w.what == WaitingKind::Request));
    assert_eq!(r.answer("req1", Choice::Allow).await.outcome, Outcome::Ok);
    let events = r.finish().await;
    assert!(events.iter().any(|e| e == "paired") && events.iter().any(|e| e.contains("-> Ok")), "{events:?}");
}

#[tokio::test]
async fn the_same_cid_gets_its_first_result_and_is_not_applied_twice() {
    let mut r = run(Scenario::Normal).await;
    let shown = r.detail("req1").await.shown;
    let c = r.computer.clone();
    let cid = r.phone.command(&c, CommandBody::Answer { id: "req1".into(), choice: Choice::Allow, shown }).await.unwrap();
    assert_eq!(r.result_of(&cid).await.outcome, Outcome::Ok);
    // a "Check status" for it, and for one the desktop never saw
    r.phone.send(&c, &PhoneMsg::Sync { waiting: vec![cid.clone(), "f".repeat(32)], versions: bobix_link::envelope::VersionRange::current() }).await.unwrap();
    r.phone.settle(Duration::from_millis(300)).await;
    let results: Vec<_> = r.phone.log.iter().filter_map(|(_, m)| if let DesktopMsg::Result(x) = m { Some(x.clone()) } else { None }).collect();
    assert!(results.iter().filter(|x| x.cid == cid).all(|x| x.outcome == Outcome::Ok));
    assert!(results.iter().any(|x| x.cid == "f".repeat(32) && x.outcome == Outcome::Unknown));
    r.finish().await;
}

#[tokio::test]
async fn a_changed_shown_hash_is_stale_and_a_wrong_clock_is_refused() {
    let mut r = run(Scenario::Normal).await;
    let c = r.computer.clone();
    let cid = r.phone.command(&c, CommandBody::Answer { id: "req1".into(), choice: Choice::Allow, shown: shown_hash("something else", false, &[]) }).await.unwrap();
    assert_eq!(r.result_of(&cid).await.outcome, Outcome::Stale);
    let shown = r.detail("req1").await.shown;
    for skew in [-11 * 60 * 1000, 3 * 60 * 1000] {
        let cid = r.phone.command_at(&c, CommandBody::Answer { id: "req1".into(), choice: Choice::Allow, shown: shown.clone() }, skew).await.unwrap();
        assert!(refused("clock")(&r.result_of(&cid).await));
    }
    r.finish().await;
}

#[tokio::test]
async fn scenario_asleep() {
    let mut r = run(Scenario::Asleep).await;
    let Some(DesktopMsg::Home(h)) = r.phone.wait_for(WAIT, |m| matches!(m, DesktopMsg::Home(_))).await else { panic!() };
    assert_eq!((h.computer.state, h.computer.unfinished), (ComputerState::Asleep, 2));
    r.phone.settle(Duration::from_millis(300)).await;
    assert!(r.phone.frames.iter().any(|f| matches!(f, Frame::Presence { state: PresenceState::Asleep, .. })));
    r.finish().await;
}

#[tokio::test]
async fn scenario_offline() {
    let mut r = run(Scenario::Offline).await;
    let end = tokio::time::Instant::now() + WAIT;
    while !r.phone.frames.iter().any(|f| matches!(f, Frame::Presence { state: PresenceState::Offline, .. })) && tokio::time::Instant::now() < end {
        r.phone.pump(Duration::from_millis(100)).await;
    }
    assert!(r.phone.frames.iter().any(|f| matches!(f, Frame::Presence { state: PresenceState::Offline, .. })), "{:?}", r.phone.frames);
    // A command sent to a computer that is offline is queued by the relay, and can be withdrawn.
    let c = r.computer.clone();
    r.phone.command(&c, CommandBody::Stop { session: "s1".into(), request: "req1".into() }).await.unwrap();
    r.phone.settle(Duration::from_millis(300)).await;
    assert!(r.phone.frames.iter().any(|f| matches!(f, Frame::Queued { .. })));
}

#[tokio::test]
async fn scenario_answered_on_computer() {
    let mut r = run(Scenario::AnsweredOnComputer).await;
    let res = r.answer("req1", Choice::Allow).await;
    assert_eq!((res.outcome, res.by.as_deref()), (Outcome::LatePc, Some("computer")));
    r.finish().await;
}

#[tokio::test]
async fn scenario_two_phones() {
    let relay = stub::spawn().await;
    let (qr, desktop) = desktop_on(&relay, Scenario::TwoPhones).await;
    let mut a = Phone::connect(&relay, "Phone A").await.unwrap();
    let mut b = Phone::connect(&relay, "Phone B").await.unwrap();
    let (ra, rb) = tokio::join!(a.pair(&qr, all_rights()), b.pair(&qr, all_rights()));
    assert_eq!(ra, Err("pairing cancelled".to_string()));
    assert_eq!(rb, Err("pairing cancelled".to_string()));
    let events = desktop.await.unwrap();
    assert!(events.iter().any(|e| e.contains("pairing cancelled")), "{events:?}");
    assert!(!events.iter().any(|e| e == "paired"));
}

#[tokio::test]
async fn scenario_expired() {
    let mut r = run(Scenario::Expired).await;
    assert_eq!(r.answer("req1", Choice::Allow).await.outcome, Outcome::Expired);
    r.finish().await;
}

#[tokio::test]
async fn scenario_long_command() {
    let mut r = run(Scenario::LongCommand).await;
    let d = r.detail("req1").await;
    assert_eq!(d.text.len(), MAX_TOOL_INPUT);
    assert_eq!(d.shown, shown_hash(&d.text, d.cut, &d.questions));
    assert!(!d.cut);
    assert!(DesktopMsg::Detail(d).validate().is_ok());
    assert_eq!(r.answer("req1", Choice::Allow).await.outcome, Outcome::Ok);
    r.finish().await;
}

#[tokio::test]
async fn scenario_cut_command() {
    let mut r = run(Scenario::CutCommand).await;
    assert!(r.detail("req1").await.cut);
    assert!(refused("cut")(&r.answer("req1", Choice::Allow).await), "a remote Allow of a cut input is refused");
    assert_eq!(r.answer("req1", Choice::Deny).await.outcome, Outcome::Ok);
    r.finish().await;
}

#[tokio::test]
async fn scenario_destructive() {
    let mut r = run(Scenario::Destructive).await;
    let Some(DesktopMsg::Home(h)) = r.phone.wait_for(WAIT, |m| matches!(m, DesktopMsg::Home(_))).await else { panic!() };
    assert!(h.waiting.iter().any(|w| w.id == "req1" && w.risky));
    assert!(r.detail("req1").await.text.starts_with("rm -rf"));
    assert_eq!(r.answer("req1", Choice::Allow).await.outcome, Outcome::Ok);
    r.finish().await;
}

#[tokio::test]
async fn scenario_long_plan() {
    let mut r = run(Scenario::LongPlan).await;
    let d = r.detail("plan1").await;
    assert!(d.text.len() > 19_000 && d.text.len() <= MAX_PLAN);
    let c = r.computer.clone();
    let cid = r.phone.command(&c, CommandBody::Plan { id: "plan1".into(), decision: Decision::Approve, note: None, shown: d.shown }).await.unwrap();
    assert_eq!(r.result_of(&cid).await.outcome, Outcome::Ok);
    r.finish().await;
}

#[tokio::test]
async fn scenario_question() {
    let mut r = run(Scenario::Question).await;
    let d = r.detail("q1").await;
    assert_eq!(d.questions[0].options, vec!["SQLite", "Postgres"]);
    let c = r.computer.clone();
    let cid = r.phone.command(&c, CommandBody::Question { id: "q1".into(), answers: vec!["SQLite".into()], shown: d.shown }).await.unwrap();
    assert_eq!(r.result_of(&cid).await.outcome, Outcome::Ok);
    r.finish().await;
}

#[tokio::test]
async fn scenario_folder_missing() {
    let mut r = run(Scenario::FolderMissing).await;
    let (prompt, res) = r.start().await;
    assert!(!prompt.folder_known);
    assert!(refused("folder")(&res));
    r.finish().await;
}

#[tokio::test]
async fn scenario_low_disk() {
    let mut r = run(Scenario::LowDisk).await;
    let (prompt, res) = r.start().await;
    assert!(prompt.free_disk < 100_000_000);
    assert!(refused("disk")(&res));
    r.finish().await;
}

#[tokio::test]
async fn a_start_with_the_whole_prompt_is_accepted_and_a_changed_prompt_is_stale() {
    let mut r = run(Scenario::Normal).await;
    let (prompt, res) = r.start().await;
    assert_eq!(prompt.prompt_hash, prompt_hash(&prompt.prompt));
    assert_eq!(res.outcome, Outcome::Ok);
    let c = r.computer.clone();
    let cid = r.phone.command(&c, CommandBody::Start { project: "demo-app".into(), iid: "12".into(), mode: Mode::Full, prompt_hash: prompt_hash("another prompt") }).await.unwrap();
    assert_eq!(r.result_of(&cid).await.outcome, Outcome::Stale);
    r.finish().await;
}

#[tokio::test]
async fn stop_names_a_held_request_and_nothing_else_can_be_stopped() {
    let mut r = run(Scenario::Normal).await;
    let c = r.computer.clone();
    let cid = r.phone.command(&c, CommandBody::Stop { session: "s1".into(), request: "req1".into() }).await.unwrap();
    assert_eq!(r.result_of(&cid).await.outcome, Outcome::Ok);
    let cid = r.phone.command(&c, CommandBody::Stop { session: "s1".into(), request: "gone".into() }).await.unwrap();
    assert!(refused("not-held")(&r.result_of(&cid).await));
    // the request must belong to the session being stopped
    let cid = r.phone.command(&c, CommandBody::Stop { session: "another-session".into(), request: "req1".into() }).await.unwrap();
    assert!(refused("not-held")(&r.result_of(&cid).await));
    r.finish().await;
}

#[tokio::test]
async fn scenario_two_computers() {
    let relay = stub::spawn().await;
    let (qr_a, da) = desktop_on(&relay, Scenario::TwoComputers).await;
    let (qr_b, db) = desktop_on(&relay, Scenario::TwoComputers).await;
    let mut phone = Phone::connect(&relay, "Test phone (demo)").await.unwrap();
    let a = phone.pair(&qr_a, all_rights()).await.unwrap();
    let b = phone.pair(&qr_b, all_rights()).await.unwrap();
    assert_ne!(a, b);
    // A command meant for A that reaches B is refused by B; each computer takes its own.
    let body = || CommandBody::Stop { session: "s1".into(), request: "req1".into() };
    let cid = phone.command_addressed(&b, &a, body()).await.unwrap();
    match phone.wait_for(WAIT, |m| matches!(m, DesktopMsg::Result(r) if r.cid == cid)).await {
        Some(DesktopMsg::Result(r)) => assert!(refused("computer")(&r), "{r:?}"),
        other => panic!("{other:?}"),
    }
    for c in [&a, &b] {
        let cid = phone.command(c, body()).await.unwrap();
        assert!(phone.wait_for(WAIT, |m| matches!(m, DesktopMsg::Result(r) if r.cid == cid && r.outcome == Outcome::Ok)).await.is_some());
    }
    let homes: Vec<&String> = phone.log.iter().filter(|(_, m)| matches!(m, DesktopMsg::Home(_))).map(|(c, _)| c).collect();
    assert!(homes.contains(&&a) && homes.contains(&&b));
    for c in [a, b] {
        let _ = phone.bye(&c).await;
    }
    let _ = (da.await, db.await);
}

#[tokio::test]
async fn scenario_revoked() {
    let mut r = run(Scenario::Revoked).await;
    let Some(DesktopMsg::Revoked { why }) = r.phone.wait_for(WAIT, |m| matches!(m, DesktopMsg::Revoked { .. })).await else { panic!() };
    assert_eq!(why, "removed on the computer");
    r.phone.settle(Duration::from_millis(300)).await;
    assert!(r.phone.frames.iter().any(|f| matches!(f, Frame::Error { code: ErrorCode::Revoked, .. })), "the relay cuts the phone off too");
}

#[tokio::test]
async fn scenario_late_push() {
    let mut r = run(Scenario::LatePush).await;
    r.phone.settle(Duration::from_millis(500)).await;
    let titles: Vec<String> = r.phone.log.iter().filter_map(|(_, m)| if let DesktopMsg::Notice(n) = m { Some(n.title.clone()) } else { None }).collect();
    assert_eq!(titles, ["on-time", "jumped", "late-inside-window", "final"]);
    // replayed, far too old, and an older `home` after a newer one: dropped silently, three times
    assert_eq!(r.phone.drops.len(), 3, "{:?}", r.phone.drops);
    // and the pairing is intact
    assert_eq!(r.answer("req1", Choice::Allow).await.outcome, Outcome::Ok);
    r.finish().await;
}

#[tokio::test]
async fn scenario_odd_input() {
    let mut r = run(Scenario::OddInput).await;
    let d = r.detail("req1").await;
    assert!(d.odd);
    assert!(d.text.contains("\\u{202e}") && d.text.contains("\\u{200b}"));
    assert!(!d.text.contains('\u{202e}') && !d.text.contains('\u{200b}'), "no raw hidden character reaches the phone");
    assert_eq!(d.shown, shown_hash(&d.text, d.cut, &d.questions));
    assert_eq!(r.answer("req1", Choice::Deny).await.outcome, Outcome::Ok);
    r.finish().await;
}

#[tokio::test]
async fn scenario_relay_moved() {
    let mut r = run(Scenario::RelayMoved).await;
    let Some(DesktopMsg::RelayMoved { url }) = r.phone.wait_for(WAIT, |m| matches!(m, DesktopMsg::RelayMoved { .. })).await else { panic!() };
    assert_eq!(url, "wss://new-relay.example/ws");
    assert!(bobix_link::pairing::relay_url_ok(&url));
    r.finish().await;
}

#[tokio::test]
async fn scenario_unknown_command() {
    let mut r = run(Scenario::UnknownCommand).await;
    let (c, cid) = (r.computer.clone(), "a".repeat(32));
    r.phone.send(&c, &PhoneMsg::Sync { waiting: vec![cid.clone()], versions: bobix_link::envelope::VersionRange::current() }).await.unwrap();
    assert_eq!(r.result_of(&cid).await.outcome, Outcome::Unknown);
    r.finish().await;
}

#[tokio::test]
async fn every_scenario_name_is_known_and_an_unknown_one_lists_them() {
    assert_eq!(ALL.len(), 19);
    for (name, s) in ALL {
        assert_eq!(Scenario::parse(name), Ok(s));
    }
    let err = Scenario::parse("nope").unwrap_err();
    for (name, _) in ALL {
        assert!(err.contains(name), "{err}");
    }
}

#[tokio::test]
async fn a_malformed_or_expired_qr_is_refused_with_a_reason() {
    let relay = stub::spawn().await;
    let mut phone = Phone::connect(&relay, "Test phone (demo)").await.unwrap();
    for bad in ["", "hello", "bobix://pair?r=wss%3A%2F%2Fx", "https://example.com"] {
        let e = phone.pair(bad, all_rights()).await.unwrap_err();
        assert!(e.contains("cannot be used"), "{e}");
    }
    // a code the relay no longer holds open
    let mut late = Phone::connect(&relay, "Late phone").await.unwrap();
    let id = bobix_link::keys::Identity::generate();
    let mut conn = common::Conn::connect(&relay).await.unwrap();
    conn.login(&id).await.unwrap();
    let code = bobix_link::pairing::PairingCode::new(&relay, id.public()).unwrap();
    conn.send(&Frame::PairOpen { relay_token_hash: bobix_link::b64(&code.relay_token_hash()), ttl: 0 }).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let e = late.pair(&code.to_uri(), all_rights()).await.unwrap_err();
    assert_eq!(e, "the pairing code expired");
}

#[tokio::test]
async fn the_tools_print_no_key_material() {
    let mut r = run(Scenario::Normal).await;
    r.answer("req1", Choice::Allow).await;
    let secrets = [r.phone.id.box_secret().to_vec(), r.phone.id.auth_seed().to_vec()];
    let enc = |s: &[u8]| [bobix_link::hex(s), bobix_link::b64(s)];
    let out = r.phone.lines.join("\n");
    for s in &secrets {
        for e in enc(s) {
            assert!(!out.contains(&e));
        }
    }
    assert!(!out.contains("signSecret") && !out.contains("macKey"));
    let events = r.finish().await.join("\n");
    assert!(!events.contains("macKey") && !events.contains("secret"));
}

#[tokio::test]
async fn a_sessions_last_lines_go_only_to_a_phone_with_the_activity_right() {
    let mut with = run(Scenario::Normal).await;
    assert!(with.detail("lines1").await.text.contains("all passed"));
    with.finish().await;

    let mut without = run_with(Scenario::Normal, Rights { activity: false, ..all_rights() }).await;
    let c = without.computer.clone();
    without.phone.send(&c, &PhoneMsg::Detail { id: "lines1".into() }).await.unwrap();
    let err = without.phone.wait_for(WAIT, |m| matches!(m, DesktopMsg::Error { .. } | DesktopMsg::Detail(_))).await;
    assert!(matches!(err, Some(DesktopMsg::Error { ref code, .. }) if code == "right"), "{err:?}");
    // the other items still show
    assert_eq!(without.detail("req1").await.id, "req1");
    without.finish().await;
}

#[tokio::test]
async fn an_answer_built_from_changed_options_is_stale() {
    let mut r = run(Scenario::Question).await;
    let d = r.detail("q1").await;
    let c = r.computer.clone();
    // the phone saw one set of options; the item the desktop holds now has a different one
    let other = shown_hash(&d.text, d.cut, &[Question { text: d.text.clone(), options: vec!["Postgres".into(), "SQLite".into()], multi_select: false }]);
    let cid = r.phone.command(&c, CommandBody::Question { id: "q1".into(), answers: vec!["SQLite".into()], shown: other }).await.unwrap();
    assert_eq!(r.result_of(&cid).await.outcome, Outcome::Stale);
    r.finish().await;
}
