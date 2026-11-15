use bobix_link::command::{self, Command, Refusal, VerifyCtx};
use bobix_link::envelope::{Direction, Update, VersionRange};
use bobix_link::keys::{soft::SoftSigner, Identity, PublicKeys};
use bobix_link::messages::*;
use bobix_link::pairing::*;
use bobix_link::relay_frames::*;
use serde_json::{json, Value};

const NOW: u64 = 1_800_000_000_000;

struct World {
    desktop: Identity,
    phone: Identity,
    signer: SoftSigner,
}

fn world() -> World {
    World { desktop: Identity::generate(), phone: Identity::generate(), signer: SoftSigner::generate() }
}

fn rights() -> Rights {
    Rights { answer: true, plan: true, question: true, start: false, stop: false, activity: true }
}

fn answer(shown: &str) -> CommandBody {
    CommandBody::Answer { id: "req1".into(), choice: Choice::Allow, shown: shown.into() }
}

fn signed(w: &World, counter: u64, issued: u64, body: &CommandBody) -> Command {
    command::build(1, &w.phone.device_id(), &w.desktop.device_id(), counter, issued, body, |m| w.signer.sign(m))
}

fn verify(w: &World, counter: u64, now: u64, c: &Command) -> Result<CommandBody, Refusal> {
    let key = w.signer.public();
    command::verify(
        &VerifyCtx { version: 1, phone_id: &w.phone.device_id(), computer_id: &w.desktop.device_id(), sign_key: &key, counter, now_ms: now },
        c,
    )
}

// ---- commands -------------------------------------------------------------------------------------------------

#[test]
fn a_signed_command_verifies() {
    let w = world();
    let c = signed(&w, 77, NOW, &answer("abc"));
    assert_eq!(c.cid.len(), 32);
    assert_eq!(verify(&w, 77, NOW, &c), Ok(answer("abc")));
}

#[test]
fn a_command_for_another_computer_is_refused() {
    let w = world();
    let other = Identity::generate();
    let c = command::build(1, &w.phone.device_id(), &other.device_id(), 1, NOW, &answer("x"), |m| w.signer.sign(m));
    assert_eq!(verify(&w, 1, NOW, &c), Err(Refusal::Computer));
    // Even with the field rewritten to this computer, the signature names the other one.
    let mut forged = c.clone();
    forged.computer = w.desktop.device_id();
    assert_eq!(verify(&w, 1, NOW, &forged), Err(Refusal::Signature));
}

#[test]
fn command_age_window() {
    let w = world();
    let at = |issued: u64| signed(&w, 5, issued, &answer("x"));
    assert!(verify(&w, 5, NOW, &at(NOW - COMMAND_MAX_AGE_MS)).is_ok());
    assert_eq!(verify(&w, 5, NOW, &at(NOW - COMMAND_MAX_AGE_MS - 1)), Err(Refusal::Clock));
    assert!(verify(&w, 5, NOW, &at(NOW + COMMAND_MAX_AHEAD_MS)).is_ok());
    assert_eq!(verify(&w, 5, NOW, &at(NOW + COMMAND_MAX_AHEAD_MS + 1)), Err(Refusal::Clock));
    assert_eq!(verify(&w, 5, NOW, &at(u64::MAX)), Err(Refusal::Clock), "a time near the top of u64 must not overflow");
    assert_eq!(verify(&w, 5, u64::MAX, &at(NOW)), Err(Refusal::Clock));
    assert_eq!(Refusal::Clock.why(), "clock");
}

#[test]
fn another_key_or_a_bad_encoding_is_refused() {
    let w = world();
    let c = signed(&w, 5, NOW, &answer("x"));
    let stranger = SoftSigner::generate();
    let by_stranger = command::build(1, &w.phone.device_id(), &w.desktop.device_id(), 5, NOW, &answer("x"), |m| stranger.sign(m));
    assert_eq!(verify(&w, 5, NOW, &by_stranger), Err(Refusal::Signature));
    let mut bad = c.clone();
    bad.sig = bobix_link::b64(&[0u8; 64]); // raw r|s, not DER
    assert_eq!(verify(&w, 5, NOW, &bad), Err(Refusal::Encoding));
    let mut bad = c.clone();
    bad.sig = "!!not base64!!".into();
    assert_eq!(verify(&w, 5, NOW, &bad), Err(Refusal::Encoding));
    let mut bad = c.clone();
    bad.cid = "short".into();
    assert_eq!(verify(&w, 5, NOW, &bad), Err(Refusal::Encoding));
    let mut bad = c.clone();
    bad.body = "{\"kind\":\"nope\"}".into();
    assert_eq!(verify(&w, 5, NOW, &bad), Err(Refusal::Encoding));
}

#[test]
fn the_signature_binds_the_packet_counter_the_body_and_every_field() {
    let w = world();
    let c = signed(&w, 5, NOW, &answer("x"));
    assert_eq!(verify(&w, 6, NOW, &c), Err(Refusal::Signature), "another packet counter");
    let mut changed = c.clone();
    changed.body = changed.body.replace("allow", "deny");
    assert_eq!(verify(&w, 5, NOW, &changed), Err(Refusal::Signature), "another body");
    let mut changed = c.clone();
    changed.issued_at += 1;
    assert_eq!(verify(&w, 5, NOW, &changed), Err(Refusal::Signature), "another time");
    let mut changed = c;
    changed.cid = bobix_link::hex(&[9u8; 16]);
    assert_eq!(verify(&w, 5, NOW, &changed), Err(Refusal::Signature), "another cid");
}

#[test]
fn length_prefixes_keep_neighbouring_fields_apart() {
    let a = command::command_bytes(1, "ab", "c", "answer", "t", "cid", 1, 2, "{}");
    let b = command::command_bytes(1, "a", "bc", "answer", "t", "cid", 1, 2, "{}");
    assert_ne!(a, b);
}

#[test]
fn shown_must_match_what_the_desktop_holds() {
    let held = Detail::build("r1", DetailKind::ToolInput, "rm -rf build", false, vec![]);
    assert!(command::shown_matches(&held, &held.shown));
    // what the phone was shown is the text as displayed; the desktop recomputes from what it holds
    assert!(!command::shown_matches(&Detail::build("r1", DetailKind::ToolInput, "rm -rf /", false, vec![]), &held.shown));
    let odd = Detail::build("r1", DetailKind::ToolInput, "ls \u{202e}x", false, vec![]);
    assert!(odd.odd && command::shown_matches(&odd, &odd.shown), "an item with hidden characters is not stale against its own hash");
}

#[test]
fn shown_covers_the_options_their_order_and_cut() {
    let q = |opts: &[&str]| vec![Question { text: "Which?".into(), options: opts.iter().map(|s| s.to_string()).collect(), multi_select: false }];
    let base = Detail::build("q1", DetailKind::Question, "Which?", false, q(&["SQLite", "Postgres"]));
    for changed in [
        Detail::build("q1", DetailKind::Question, "Which?", false, q(&["Postgres", "SQLite"])), // order
        Detail::build("q1", DetailKind::Question, "Which?", false, q(&["SQLite", "Postgres", "Other"])), // options
        Detail::build("q1", DetailKind::Question, "Which?", true, q(&["SQLite", "Postgres"])), // cut
        Detail::build("q1", DetailKind::Question, "Which?", false, vec![Question { multi_select: true, ..q(&["SQLite", "Postgres"]).remove(0) }]),
    ] {
        assert!(!command::shown_matches(&changed, &base.shown));
    }
    assert!(command::shown_matches(&base, &base.shown));
    // a boundary between two options cannot be moved: ["ab", "c"] is not ["a", "bc"]
    let a = Detail::build("q", DetailKind::Question, "t", false, q(&["ab", "c"]));
    let b = Detail::build("q", DetailKind::Question, "t", false, q(&["a", "bc"]));
    assert_ne!(a.shown, b.shown);
}

#[test]
fn sync_gives_unknown_for_a_command_never_received() {
    let known = CmdResult::new("aaaa", Outcome::LatePc);
    let out = command::sync_results(&["aaaa".into(), "bbbb".into()], |cid| (cid == "aaaa").then(|| known.clone()));
    assert_eq!(out[0], known);
    assert_eq!(out[1], CmdResult::new("bbbb", Outcome::Unknown));
}

#[test]
fn bye_and_auth_bytes_are_domain_separated() {
    assert_ne!(command::bye_bytes(1, "a", "b", 1), command::bye_bytes(1, "a", "b", 2));
    assert_ne!(command::auth_bytes("wss://r.example", "x").unwrap(), command::bye_bytes(1, "x", "", 0));
}

// ---- messages -------------------------------------------------------------------------------------------------

fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(v: &T) -> Value {
    let text = serde_json::to_string(v).unwrap();
    assert_eq!(&serde_json::from_str::<T>(&text).unwrap(), v);
    serde_json::from_str(&text).unwrap()
}

#[test]
fn desktop_messages_round_trip_with_documented_names() {
    let waiting = Waiting {
        id: "r1".into(), session: "s1".into(), what: WaitingKind::Request, preview: "npm test".into(),
        cut: false, risky: true, at_computer: false, seconds_left: Some(280),
    };
    let home = DesktopMsg::Home(Home {
        sent_at: NOW,
        computer: Computer { name: "Studio".into(), state: ComputerState::Online, unfinished: 0 },
        sessions: vec![SessionInfo { id: "s1".into(), title: "Fix login".into(), project: "demo".into(), state: "waiting".into(), pet: Pet { design: "cat".into(), color: 3 }, can_stop: false }],
        waiting: vec![waiting],
        groups: vec![Group { group: GroupKind::NeedsYou, ids: vec!["r1".into()] }],
    });
    let v = round_trip(&home);
    assert_eq!(v["kind"], "home");
    assert_eq!(v["sentAt"], NOW);
    assert_eq!(v["waiting"][0]["secondsLeft"], 280);
    assert_eq!(v["waiting"][0]["atComputer"], false);
    assert_eq!(v["groups"][0]["group"], "needsYou");

    let detail = DesktopMsg::Detail(Detail {
        id: "r1".into(), what: DetailKind::ToolInput, text: "ls".into(), shown: shown_hash("ls", false, &[]), odd: false, cut: false, questions: vec![],
    });
    assert_eq!(round_trip(&detail)["what"], "toolInput");

    let prompt = DesktopMsg::StartPrompt(StartPrompt {
        project: "demo".into(), iid: "12".into(), mode: Mode::Quick, prompt: "Do it".into(),
        prompt_hash: prompt_hash("Do it"), folder_known: true, free_disk: 5_000_000_000,
    });
    let v = round_trip(&prompt);
    assert_eq!((v["kind"].as_str(), v["promptHash"].is_string(), v["folderKnown"].as_bool()), (Some("startPrompt"), true, Some(true)));

    for (o, name) in [
        (Outcome::Ok, "ok"), (Outcome::LatePc, "latePc"), (Outcome::LatePhone, "latePhone"), (Outcome::Expired, "expired"),
        (Outcome::Released, "released"), (Outcome::Stale, "stale"), (Outcome::Refused, "refused"), (Outcome::Unknown, "unknown"),
    ] {
        let r = DesktopMsg::Result(CmdResult { cid: "c".into(), outcome: o, by: Some("pc".into()), why: None });
        let v = round_trip(&r);
        assert_eq!((v["kind"].as_str(), v["outcome"].as_str(), v["by"].as_str()), (Some("result"), Some(name), Some("pc")));
        assert!(v.get("why").is_none(), "an absent `why` is not sent");
    }

    assert_eq!(round_trip(&DesktopMsg::Paired { computer: "Studio".into(), rights: rights() })["kind"], "paired");
    assert_eq!(round_trip(&DesktopMsg::Revoked { why: "removed".into() })["why"], "removed");
    assert_eq!(round_trip(&DesktopMsg::RelayMoved { url: "wss://r.example".into() })["kind"], "relayMoved");
    let n = DesktopMsg::Notice(Notice { what: NoticeKind::Waiting, reference: "r1".into(), session: "s1".into(), title: "t".into(), body: "b".into() });
    let v = round_trip(&n);
    assert_eq!((v["kind"].as_str(), v["what"].as_str(), v["ref"].as_str()), (Some("notice"), Some("waiting"), Some("r1")));
    assert_eq!(round_trip(&DesktopMsg::Error { code: "update".into(), who: Some(Who::Desktop) })["who"], "desktop");
}

#[test]
fn phone_messages_round_trip_with_documented_names() {
    let w = world();
    let versions = VersionRange::current();
    assert_eq!(round_trip(&PhoneMsg::PairedAck)["kind"], "pairedAck");
    let v = round_trip(&PhoneMsg::Sync { waiting: vec!["c1".into()], versions });
    assert_eq!((v["kind"].as_str(), v["waiting"][0].as_str(), v["versions"]["max"].as_u64()), (Some("sync"), Some("c1"), Some(1)));
    assert_eq!(round_trip(&PhoneMsg::Detail { id: "r1".into() })["kind"], "detail");
    let v = round_trip(&PhoneMsg::StartPreview { project: "demo".into(), iid: "3".into(), mode: Mode::Full });
    assert_eq!((v["kind"].as_str(), v["mode"].as_str()), (Some("startPreview"), Some("full")));
    assert_eq!(round_trip(&PhoneMsg::Bye { sig: "x".into() })["kind"], "bye");

    let bodies = [
        (answer("s"), json!({"kind":"answer","id":"req1","choice":"allow","shown":"s"})),
        (CommandBody::Plan { id: "p".into(), decision: Decision::Back, note: Some("no".into()), shown: "s".into() }, json!({"kind":"plan","id":"p","decision":"back","note":"no","shown":"s"})),
        (CommandBody::Question { id: "q".into(), answers: vec!["a".into()], shown: "s".into() }, json!({"kind":"question","id":"q","answers":["a"],"shown":"s"})),
        (CommandBody::Start { project: "demo".into(), iid: "3".into(), mode: Mode::Quick, prompt_hash: "h".into() }, json!({"kind":"start","project":"demo","iid":"3","mode":"quick","promptHash":"h"})),
        (CommandBody::Stop { session: "s1".into(), request: "req1".into() }, json!({"kind":"stop","session":"s1","request":"req1"})),
    ];
    for (b, expected) in bodies {
        assert_eq!(serde_json::to_value(&b).unwrap(), expected);
        let cmd = PhoneMsg::Cmd(signed(&w, 1, NOW, &b));
        let v = round_trip(&cmd);
        assert_eq!((v["kind"].as_str(), v["issuedAt"].as_u64()), (Some("cmd"), Some(NOW)));
        assert!(v["body"].is_string(), "the body travels as the exact text that was hashed");
    }
}

#[test]
fn limits_are_measured_in_bytes() {
    let detail = |what, text: String| DesktopMsg::Detail(Detail { id: "r".into(), what, text, shown: String::new(), odd: false, cut: false, questions: vec![] });
    assert!(detail(DetailKind::ToolInput, "a".repeat(MAX_TOOL_INPUT)).validate().is_ok());
    assert_eq!(detail(DetailKind::ToolInput, "a".repeat(MAX_TOOL_INPUT + 1)).validate(), Err(LimitError::TooLong("detail", MAX_TOOL_INPUT)));
    // 3-byte characters: 6667 of them are 20 001 bytes, though only 6667 characters
    assert!(detail(DetailKind::Plan, "€".repeat(6667)).validate().is_err());
    assert!(detail(DetailKind::Plan, "€".repeat(6666)).validate().is_ok());
    let home = |p: String| DesktopMsg::Home(Home {
        sent_at: 0, computer: Computer { name: "c".into(), state: ComputerState::Asleep, unfinished: 2 }, sessions: vec![], groups: vec![],
        waiting: vec![Waiting { id: "i".into(), session: "s".into(), what: WaitingKind::Plan, preview: p, cut: false, risky: false, at_computer: false, seconds_left: None }],
    });
    assert!(home("a".repeat(MAX_PREVIEW)).validate().is_ok());
    assert!(home("a".repeat(MAX_PREVIEW + 1)).validate().is_err());
    assert!(home("é".repeat(151)).validate().is_err()); // 302 bytes
    let note = CommandBody::Plan { id: "p".into(), decision: Decision::Back, note: Some("n".repeat(MAX_NOTE + 1)), shown: String::new() };
    assert!(note.validate().is_err());
    assert_eq!(MAX_NOTE, 2_000);
    let ok = CommandBody::Plan { id: "p".into(), decision: Decision::Back, note: Some("n".repeat(2_000)), shown: String::new() };
    assert!(ok.validate().is_ok());
    let answer = |a: String| CommandBody::Question { id: "q".into(), answers: vec![a], shown: String::new() };
    assert!(answer("a".repeat(2_000)).validate().is_ok());
    assert!(answer("a".repeat(2_001)).validate().is_err());
    assert!(answer("é".repeat(1_001)).validate().is_err()); // 2002 bytes
}

#[test]
fn session_info_says_whether_stop_is_possible() {
    let info = |can_stop| SessionInfo { id: "s".into(), title: "t".into(), project: "p".into(), state: "waiting".into(), pet: Pet { design: "cat".into(), color: 1 }, can_stop };
    assert_eq!(serde_json::to_value(info(true)).unwrap()["canStop"], true);
    assert_eq!(serde_json::to_value(info(false)).unwrap()["canStop"], false);
    // a desktop from before the flag omits it: that reads as "not possible"
    let old: SessionInfo = serde_json::from_value(json!({"id":"s","title":"t","project":"p","state":"idle","pet":{"design":"cat","color":1}})).unwrap();
    assert!(!old.can_stop);
}

#[test]
fn error_grant_names_the_phone() {
    let f = Frame::Error { code: ErrorCode::Grant, who: Some(Who::Phone), peer: Some("phone-id".into()) };
    assert_eq!(serde_json::to_value(&f).unwrap(), json!({"type":"error","code":"grant","who":"phone","peer":"phone-id"}));
    assert_eq!(Frame::parse(&f.to_text().unwrap()).unwrap(), f);
    // an older relay sends no peer
    assert_eq!(Frame::parse(r#"{"type":"error","code":"grant","who":"phone"}"#), Ok(Frame::Error { code: ErrorCode::Grant, who: Some(Who::Phone), peer: None }));
}

#[test]
fn odd_characters_become_visible_escapes() {
    let (text, odd) = visible_escapes("ls\u{202e}gpj.sh\u{200b}\nnext\ttab");
    assert!(odd);
    assert_eq!(text, "ls\\u{202e}gpj.sh\\u{200b}\nnext\ttab");
    assert_eq!(visible_escapes("plain"), ("plain".to_string(), false));
    assert!(!is_plain_text("a\u{202e}b", 10));
    assert!(!is_plain_text("a\u{0}b", 10));
    assert!(is_plain_text("Demo's phone", 64));
}

// ---- versions -------------------------------------------------------------------------------------------------

#[test]
fn a_peer_one_version_behind_works_and_two_behind_gets_update() {
    let ours = VersionRange { min: 2, max: 3 };
    assert_eq!(ours.negotiate(&VersionRange { min: 1, max: 2 }), Ok(2));
    assert_eq!(ours.negotiate(&VersionRange { min: 1, max: 1 }), Err(Update::Peer));
    // and the relay frame that says so
    let f = Frame::Error { code: ErrorCode::Update, who: Some(Who::Phone), peer: None };
    assert_eq!(serde_json::to_value(&f).unwrap(), json!({"type":"error","code":"update","who":"phone"}));
}

// ---- pairing --------------------------------------------------------------------------------------------------

fn code(w: &World) -> PairingCode {
    PairingCode::new("wss://relay.example/ws?x=1&y=2", w.desktop.public()).unwrap()
}

fn hello(w: &World, c: &PairingCode, name: &str, r: Rights) -> Hello {
    Hello::new(c, &w.phone.public(), &w.signer.public(), name, r, VersionRange::current())
}

#[test]
fn the_pairing_uri_round_trips() {
    let w = world();
    let c = code(&w);
    let uri = c.to_uri();
    assert!(uri.starts_with("bobix://pair?r=wss%3A%2F%2Frelay.example%2Fws%3Fx%3D1%26y%3D2&a="));
    assert_eq!(PairingCode::parse(&uri), Ok(c.clone()));
    assert_eq!(c.secret.len(), 32);
}

#[test]
fn only_wss_or_a_local_relay_is_accepted() {
    let k = world().desktop.public();
    assert!(PairingCode::new("wss://relay.example", k).is_ok());
    for ok in ["ws://localhost:8080", "ws://127.0.0.1:9", "ws://[::1]:9/x"] {
        assert!(PairingCode::new(ok, k).is_ok(), "{ok}");
    }
    for bad in ["ws://relay.example", "ws://localhost.evil.example", "http://localhost", "https://relay.example", "wss://", "relay.example", "ws://127.0.0.1.evil.com"] {
        assert_eq!(PairingCode::new(bad, k).unwrap_err(), PairError::InsecureRelay, "{bad}");
    }
    // a hand-written code with an insecure relay is refused when parsed
    let uri = PairingCode::new("wss://r.example", k).unwrap().to_uri().replace("wss%3A", "ws%3A");
    assert_eq!(PairingCode::parse(&uri), Err(PairError::InsecureRelay));
    for bad in ["", "bobix://pair", "bobix://pair?r=x", "http://x", "bobix://pair?r=wss%3A%2F%2Fx&a=1&b=2&t=3", "bobix://pair?r=a&r=b&a=1&b=2&t=3", "bobix://pair?z=1"] {
        assert!(PairingCode::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn relay_token_and_mac_key_are_derived_apart() {
    let w = world();
    let c = code(&w);
    assert_ne!(c.relay_token(), c.mac_key());
    assert_ne!(c.relay_token_hash(), c.relay_token());
    let other = code(&w);
    assert_ne!(c.relay_token(), other.relay_token());
}

#[test]
fn hello_mac_covers_every_field() {
    let w = world();
    let c = code(&w);
    let h = hello(&w, &c, "Pixel", rights());
    assert!(h.verify(&c).is_ok());
    let mut more = h.clone();
    more.rights.start = true; // rights changed after the MAC was computed
    assert_eq!(more.verify(&c).unwrap_err(), PairError::Mac);
    let mut renamed = h.clone();
    renamed.name = "Pixel 2".into();
    assert_eq!(renamed.verify(&c).unwrap_err(), PairError::Mac);
    let mut swapped = h.clone();
    swapped.boxk = bobix_link::b64(&Identity::generate().public().boxk);
    assert_eq!(swapped.verify(&c).unwrap_err(), PairError::Mac);
    let mut resigned = h.clone();
    resigned.sign = bobix_link::b64(&SoftSigner::generate().public().to_sec1());
    assert_eq!(resigned.verify(&c).unwrap_err(), PairError::Mac);
    // a phone that never saw the code cannot make a valid MAC
    assert_eq!(hello(&w, &code(&w), "Pixel", rights()).verify(&c).unwrap_err(), PairError::Mac);
    // a name with hidden characters, or an empty one, is refused even with a good MAC
    assert_eq!(hello(&w, &c, "Pix\u{202e}el", rights()).verify(&c).unwrap_err(), PairError::Hello);
    assert_eq!(hello(&w, &c, "", rights()).verify(&c).unwrap_err(), PairError::Hello);
}

#[test]
fn hello_travels_sealed_to_the_desktop_with_the_phone_box_key_in_front() {
    use bobix_link::counters::RecvState;
    use bobix_link::envelope::{Stream, CURRENT_VERSION};
    use bobix_link::link::Link;
    let w = world();
    let c = code(&w);
    let v = VersionRange::current();
    let mut phone = Link::new(Direction::ToDesktop, *w.phone.box_secret(), c.desktop.boxk, CURRENT_VERSION, v, [0; 3], NOW, RecvState::default());
    let h = hello(&w, &c, "Pixel", rights());
    let body = hello_body(&w.phone.public().boxk, &phone.seal(Stream::Event, &serde_json::to_vec(&PhoneMsg::Hello(h.clone())).unwrap()).unwrap().packet);
    // desktop side
    let (phone_box, packet) = split_hello_body(&body).unwrap();
    let mut desktop = Link::new(Direction::ToPhone, *w.desktop.box_secret(), phone_box, CURRENT_VERSION, v, [0; 3], NOW, RecvState::default());
    let opened = desktop.open(packet).unwrap();
    assert_eq!(PhoneMsg::from_json(&opened.plaintext).unwrap(), PhoneMsg::Hello(h));
    assert!(split_hello_body(&[0u8; 10]).is_none());
}

fn offer(s: &mut PairingSession, h: &Hello, id: &str, now: u64) -> Result<Offer, PairError> {
    let outer: [u8; 32] = bobix_link::b64_decode(&h.boxk).unwrap().try_into().unwrap();
    s.on_hello(h, &outer, id, now)
}

#[test]
fn a_pairing_runs_open_offered_confirmed_done() {
    let w = world();
    let mut s = PairingSession::new(code(&w), NOW);
    assert_eq!(s.status(NOW), PairStatus::Open);
    let h = hello(&w, s.code(), "Pixel", rights());
    let id = w.phone.device_id();
    assert!(matches!(offer(&mut s, &h, &id, NOW + 1000), Ok(Offer::New { .. })));
    assert_eq!(s.status(NOW + 1000), PairStatus::Offered);
    assert_eq!(offer(&mut s, &h, &id, NOW + 2000), Ok(Offer::Duplicate));
    assert!(s.confirm(NOW + 3000));
    assert_eq!(s.status(NOW + 3000), PairStatus::Confirmed);
    assert!(s.ack(NOW + 4000));
    assert_eq!(s.status(NOW + 4000), PairStatus::Done);
}

#[test]
fn a_second_valid_hello_cancels_the_pairing() {
    let w = world();
    let mut s = PairingSession::new(code(&w), NOW);
    let first = hello(&w, s.code(), "Pixel", rights());
    assert!(offer(&mut s, &first, &w.phone.device_id(), NOW).is_ok());
    let second = world();
    let h2 = Hello::new(s.code(), &second.phone.public(), &second.signer.public(), "Other", rights(), VersionRange::current());
    assert_eq!(offer(&mut s, &h2, &second.phone.device_id(), NOW), Ok(Offer::Cancelled));
    assert_eq!(s.status(NOW), PairStatus::Cancelled);
    assert!(!s.confirm(NOW), "a cancelled pairing cannot be confirmed");
}

#[test]
fn an_invalid_hello_changes_nothing() {
    let w = world();
    let mut s = PairingSession::new(code(&w), NOW);
    let stranger = world();
    let bad = Hello::new(&code(&w), &stranger.phone.public(), &stranger.signer.public(), "Spy", rights(), VersionRange::current());
    assert_eq!(offer(&mut s, &bad, &stranger.phone.device_id(), NOW), Err(PairError::Mac));
    assert_eq!(s.status(NOW), PairStatus::Open);
    // the relay says the sender is someone else than the keys in the hello
    let good = hello(&w, s.code(), "Pixel", rights());
    assert_eq!(offer(&mut s, &good, &stranger.phone.device_id(), NOW), Err(PairError::Hello));
    assert_eq!(s.status(NOW), PairStatus::Open);
}

#[test]
fn the_code_lives_two_minutes_and_an_unacknowledged_pairing_ten() {
    let w = world();
    let mut s = PairingSession::new(code(&w), NOW);
    let h = hello(&w, s.code(), "Pixel", rights());
    let id = w.phone.device_id();
    assert_eq!(offer(&mut s, &h, &id, NOW + CODE_LIFETIME_MS + 1), Err(PairError::Expired));
    assert_eq!(s.status(NOW + CODE_LIFETIME_MS + 1), PairStatus::Expired);

    let mut s = PairingSession::new(code(&w), NOW);
    let h = hello(&w, s.code(), "Pixel", rights());
    assert!(offer(&mut s, &h, &id, NOW + CODE_LIFETIME_MS).is_ok());
    assert!(s.confirm(NOW + CODE_LIFETIME_MS + 5), "an offer waits for the owner past the 2 minutes");
    let late = NOW + CODE_LIFETIME_MS + 5 + ACK_LIFETIME_MS + 1;
    assert_eq!(s.status(late), PairStatus::Expired);
    assert!(!s.ack(late));
}

// ---- relay frames ---------------------------------------------------------------------------------------------

#[test]
fn every_frame_round_trips_with_documented_names() {
    let versions = VersionRange::current();
    let frames: Vec<(Frame, Value)> = vec![
        (Frame::Challenge { nonce: "n".into() }, json!({"type":"challenge","nonce":"n"})),
        (Frame::Auth { id: "i".into(), key: "k".into(), relay: "wss://r.example".into(), sig: "s".into(), versions }, json!({"type":"auth","id":"i","key":"k","relay":"wss://r.example","sig":"s","versions":{"min":1,"max":1}})),
        (Frame::Peers { allow: vec!["a".into()] }, json!({"type":"peers","allow":["a"]})),
        (Frame::Revoke { id: "a".into() }, json!({"type":"revoke","id":"a"})),
        (
            Frame::Send {
                pid: "p".into(), to: "t".into(), ttl: 600, collapse: Some("c".into()),
                push: Some(Push { kind: PushKind::LiveActivity, priority: 10, replaces: Some("r".into()) }), body: "b".into(),
            },
            json!({"type":"send","pid":"p","to":"t","ttl":600,"collapse":"c","push":{"kind":"liveActivity","priority":10,"replaces":"r"},"body":"b"}),
        ),
        (Frame::Send { pid: "p".into(), to: "t".into(), ttl: 1, collapse: None, push: None, body: "b".into() }, json!({"type":"send","pid":"p","to":"t","ttl":1,"body":"b"})),
        (Frame::Msg { pid: "p".into(), from: "f".into(), body: "b".into() }, json!({"type":"msg","pid":"p","from":"f","body":"b"})),
        (Frame::Ack { pid: "p".into() }, json!({"type":"ack","pid":"p"})),
        (Frame::Queued { pid: "p".into() }, json!({"type":"queued","pid":"p"})),
        (Frame::Delivered { pid: "p".into() }, json!({"type":"delivered","pid":"p"})),
        (Frame::Dropped { pid: "p".into() }, json!({"type":"dropped","pid":"p"})),
        (Frame::Withdraw { pid: "p".into() }, json!({"type":"withdraw","pid":"p"})),
        (Frame::Withdrawn { pid: "p".into() }, json!({"type":"withdrawn","pid":"p"})),
        (Frame::TooLate { pid: "p".into() }, json!({"type":"tooLate","pid":"p"})),
        (Frame::Presence { peer: "x".into(), state: PresenceState::Asleep }, json!({"type":"presence","peer":"x","state":"asleep"})),
        (Frame::State { asleep: true }, json!({"type":"state","asleep":true})),
        (Frame::PairOpen { relay_token_hash: "h".into(), ttl: 120 }, json!({"type":"pairOpen","relayTokenHash":"h","ttl":120})),
        (Frame::PairSend { relay_token: "t".into(), body: "b".into() }, json!({"type":"pairSend","relayToken":"t","body":"b"})),
        (
            Frame::PushToken { platform: Platform::Ios, env: ApnsEnv::Sandbox, kind: TokenKind::PushToStart, r#ref: Some("r".into()), token: "t".into(), grant: "g".into() },
            json!({"type":"pushToken","platform":"ios","env":"sandbox","kind":"pushToStart","ref":"r","token":"t","grant":"g"}),
        ),
        (Frame::Error { code: ErrorCode::Grant, who: None, peer: None }, json!({"type":"error","code":"grant"})),
    ];
    for (f, expected) in frames {
        assert_eq!(serde_json::to_value(&f).unwrap(), expected);
        assert_eq!(Frame::parse(&f.to_text().unwrap()).unwrap(), f);
    }
    for (c, name) in [(ErrorCode::Revoked, "revoked"), (ErrorCode::Oversize, "oversize"), (ErrorCode::Invite, "invite"), (ErrorCode::Update, "update"), (ErrorCode::RelayUrl, "relayUrl")] {
        assert_eq!(serde_json::to_value(&c).unwrap(), name);
    }
    // an error code a newer relay invents still parses
    assert_eq!(Frame::parse(r#"{"type":"error","code":"brandNew"}"#), Ok(Frame::Error { code: ErrorCode::Other, who: None, peer: None }));
}

#[test]
fn frames_are_cut_at_256_kib_and_the_body_stays_opaque() {
    let body = "A".repeat(MAX_FRAME);
    let big = Frame::Msg { pid: "p".into(), from: "f".into(), body };
    assert_eq!(big.to_text(), Err(FrameError::Oversize));
    let text = format!(r#"{{"type":"msg","pid":"p","from":"f","body":"{}"}}"#, "A".repeat(MAX_FRAME));
    assert_eq!(Frame::parse(&text), Err(FrameError::Oversize));
    // a body that is not even base64 is not the relay's business
    let f = Frame::parse(r#"{"type":"msg","pid":"p","from":"f","body":"not base64 at all!"}"#).unwrap();
    assert_eq!(f.pid(), Some("p"));
    assert_eq!(Frame::parse("{"), Err(FrameError::Malformed));
    assert_eq!(Frame::parse(r#"{"type":"nope"}"#), Err(FrameError::Malformed));
}

#[test]
fn pairing_keys_are_public_keys_only() {
    let k: PublicKeys = world().desktop.public();
    assert_eq!(k.device_id().len(), 32);
}

#[test]
fn the_outer_hello_key_must_be_the_macd_box_key() {
    let w = world();
    let mut s = PairingSession::new(code(&w), NOW);
    let h = hello(&w, s.code(), "Pixel", rights());
    let other = Identity::generate().public().boxk; // the packet was opened with another key than the one the MAC covers
    assert_eq!(s.on_hello(&h, &other, &w.phone.device_id(), NOW), Err(PairError::Hello));
    assert_eq!(s.status(NOW), PairStatus::Open, "nothing changed");
    let own: [u8; 32] = bobix_link::b64_decode(&h.boxk).unwrap().try_into().unwrap();
    assert!(matches!(s.on_hello(&h, &own, &w.phone.device_id(), NOW), Ok(Offer::New { .. })));
}

#[test]
fn low_order_keys_in_a_code_or_a_hello_are_refused() {
    let w = world();
    let mut zero = w.desktop.public();
    zero.boxk = [0; 32];
    let uri = PairingCode::with_secret("wss://r.example", zero, [1; 32]).unwrap().to_uri();
    assert_eq!(PairingCode::parse(&uri), Err(PairError::Malformed));
    // a hello with a zero box key and a valid MAC (a phone that knows the code secret)
    let c = code(&w);
    let mut keys = w.phone.public();
    keys.boxk = [0; 32];
    let h = Hello::new(&c, &keys, &w.signer.public(), "Pixel", rights(), VersionRange::current());
    assert_eq!(h.verify(&c).unwrap_err(), PairError::Hello);
}

#[test]
fn a_relay_address_cannot_hide_another_host_behind_userinfo() {
    for bad in [
        "ws://localhost:80@evil.example/x", "ws://127.0.0.1:1@evil.example", "ws://[::1]@evil.example", "ws://localhost\\@evil.example",
        "wss://user@relay.example", "wss://relay.example@evil.example/x", "ws://localhost @evil.example", "ws://", "wss://", "wss:///x",
    ] {
        assert!(!relay_url_ok(bad), "{bad}");
    }
    for ok in ["wss://relay.example", "wss://relay.example:8443/ws?a=b", "ws://localhost:8787", "ws://127.0.0.1", "ws://[::1]:9/x"] {
        assert!(relay_url_ok(ok), "{ok}");
    }
}

#[test]
fn a_plain_relay_is_accepted_on_a_private_network_address_and_nowhere_else() {
    for ok in [
        "ws://192.168.1.20:8787", "ws://10.0.0.5", "ws://172.16.0.1:9", "ws://172.31.255.255:1/x",
        "ws://[fd12:3456::1]:8787", "WS://192.168.0.7:8787/",
    ] {
        assert!(relay_url_ok(ok), "{ok}");
    }
    for bad in [
        "ws://169.254.169.254", "ws://169.254.1.1:8787", "ws://[fe80::1]:9", "ws://[fd00:::::]", "ws://[fd00::1::2]", "ws://172.32.0.1", "ws://172.15.0.1", "ws://192.169.0.1", "ws://11.0.0.1", "ws://8.8.8.8:8787", "ws://192.168.1.1.evil.example",
        "ws://192.168.1.256", "ws://192.168.01.1", "ws://192.168.1", "ws://0192.168.1.1", "ws://mac.local:8787", "ws://my-mac",
        "ws://[2001:db8::1]", "ws://[::ffff:10.0.0.1]", "ws://192.168.1.20@evil.example", "ws://user@192.168.1.20", "ws://192.168.1.20:99999",
    ] {
        assert!(!relay_url_ok(bad), "{bad}");
    }
    // The address is signed as it is written (lowercase, default port dropped); only loopback is rewritten to `localhost`.
    for (given, want) in [("ws://192.168.1.20:8787/", "ws://192.168.1.20:8787"), ("ws://192.168.1.20:80", "ws://192.168.1.20"), ("ws://[FD12::1]:9", "ws://[fd12::1]:9")] {
        assert_eq!(normalize_relay_url(given).as_deref(), Some(want), "{given}");
    }
    // A pairing code and the login signature carry it.
    let k = world().desktop.public();
    assert!(PairingCode::new("ws://192.168.1.20:8787", k).is_ok());
    assert!(command::auth_bytes("ws://192.168.1.20:8787", "c").is_some());
    assert!(command::auth_bytes("ws://8.8.8.8:8787", "c").is_none());
}

#[test]
fn the_pairing_secret_is_not_printed_by_debug() {
    let w = world();
    let c = code(&w);
    let shown = format!("{c:?} {:?}", PairingSession::new(c.clone(), NOW).code());
    for secret in [bobix_link::hex(&c.secret), bobix_link::b64(&c.secret), format!("{:?}", c.secret)] {
        assert!(!shown.contains(&secret));
    }
    assert!(shown.contains("<hidden>"));
}

#[test]
fn stop_needs_both_the_stop_and_the_answer_right() {
    let stop = CommandBody::Stop { session: "s1".into(), request: "req1".into() };
    let r = |answer, stop| Rights { answer, stop, ..Rights::default() };
    assert!(r(true, true).allows(&stop));
    assert!(!r(false, true).allows(&stop), "a phone with `stop` alone cannot deny a held request");
    assert!(!r(true, false).allows(&stop));
    assert!(r(true, false).allows(&answer("x")) && !r(false, true).allows(&answer("x")));
    assert!(!Rights::default().allows(&CommandBody::Start { project: "p".into(), iid: "1".into(), mode: Mode::Full, prompt_hash: "h".into() }));
}

#[test]
fn relay_urls_normalise_to_one_form() {
    let table = [
        ("wss://Relay.Example", "wss://relay.example"),
        ("WSS://relay.example:443/", "wss://relay.example"),
        ("wss://relay.example:8443/ws/", "wss://relay.example:8443/ws"),
        ("wss://relay.example:0443/ws//", "wss://relay.example/ws"),
        ("wss://relay.example/Ws?Token=A#frag", "wss://relay.example/Ws?Token=A"),
        ("wss://relay.example/?", "wss://relay.example"),
        ("wss://relay.example:", "wss://relay.example"),
        ("ws://LOCALHOST:8787", "ws://localhost:8787"),
        ("ws://127.0.0.1:8787/", "ws://localhost:8787"),
        ("ws://[::1]:8787", "ws://localhost:8787"),
        ("ws://localhost:80", "ws://localhost"),
        ("wss://[2001:DB8::1]:9000/x", "wss://[2001:db8::1]:9000/x"),
    ];
    for (given, want) in table {
        assert_eq!(normalize_relay_url(given).as_deref(), Some(want), "{given}");
        assert_eq!(normalize_relay_url(want).as_deref(), Some(want), "idempotent: {want}");
    }
    for bad in [
        "wss://relay.example:+443", "wss://relay.example:-1", "wss://relay.example:4 3", "wss://2001:db8::1", "wss://a:b:443", "wss://2001:db8::1:443",
        "wss://[relay.example]", "wss://[::1", "wss://[::g]", "wss://relay.exa\tmple", "wss://relay.example/a\nb", "wss://rel\u{0}ay.example",
        "wss://RÉLAY.example", "wss://relay.exämple:443", "ws://localhost\t", "wss://relay.example\u{7f}",
    ] {
        assert_eq!(normalize_relay_url(bad), None, "{bad:?}");
        assert!(!relay_url_ok(bad), "{bad:?}");
    }
    for bad in ["ws://relay.example", "http://relay.example", "wss://", "wss://a@b", "wss://relay.example:99999", "wss://relay.example:x", "relay.example", "wss://rel ay"] {
        assert_eq!(normalize_relay_url(bad), None, "{bad}");
    }
}

#[test]
fn auth_bytes_bind_the_relay_address_and_the_challenge() {
    let a = command::auth_bytes("wss://a.example", "n1").unwrap();
    assert_eq!(a, command::auth_bytes("WSS://A.example:443/", "n1").unwrap(), "same address, same bytes");
    assert_ne!(a, command::auth_bytes("wss://b.example", "n1").unwrap());
    assert_ne!(a, command::auth_bytes("wss://a.example", "n2").unwrap());
    assert_eq!(command::auth_bytes("ws://relay.example", "n1"), None);
}

#[test]
fn a_signature_made_for_relay_a_is_refused_by_relay_b() {
    use command::AuthRefusal;
    let id = Identity::generate();
    let ch = "challenge-from-relay-b";
    let frame = |url: &str| match command::auth_frame(&id, url, ch, VersionRange::current()).unwrap() {
        Frame::Auth { id, key, relay, sig, .. } => (id, key, relay, sig),
        _ => unreachable!(),
    };
    let check = |own: &str, (i, k, r, s): (String, String, String, String)| command::verify_auth_frame(own, ch, &i, &k, &r, &s);
    // honest: signed for the address the relay has
    assert_eq!(check("wss://b.example", frame("wss://b.example")), Ok(id.public().auth));
    assert_eq!(check("wss://B.example:443/", frame("wss://b.example")), Ok(id.public().auth), "normalised on both sides");
    // signed for relay A, shown to relay B: a clear, distinct refusal
    assert_eq!(check("wss://b.example", frame("wss://a.example")), Err(AuthRefusal::OtherRelay));
    assert_eq!(AuthRefusal::OtherRelay.code(), ErrorCode::RelayUrl);
    // a signature for A with the claimed address rewritten to B is simply invalid
    let (i, k, _, s) = frame("wss://a.example");
    assert_eq!(command::verify_auth_frame("wss://b.example", ch, &i, &k, "wss://b.example", &s), Err(AuthRefusal::Invalid));
    // wrong id, wrong challenge, garbage
    let (i, k, r, s) = frame("wss://b.example");
    assert_eq!(command::verify_auth_frame("wss://b.example", ch, "00".repeat(16).as_str(), &k, &r, &s), Err(AuthRefusal::Invalid));
    assert_eq!(command::verify_auth_frame("wss://b.example", "another", &i, &k, &r, &s), Err(AuthRefusal::Invalid));
    assert_eq!(command::verify_auth_frame("wss://b.example", ch, &i, "!!", &r, &s), Err(AuthRefusal::Invalid));
    assert_eq!(AuthRefusal::Invalid.code(), ErrorCode::Auth);
}
