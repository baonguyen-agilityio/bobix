//! The fake desktop: prints a QR string, accepts a pairing, then plays a scenario. It holds made-up sessions and
//! requests only, and answers commands by the same rules the real desktop will apply.

use super::scenario::Scenario;
use super::{new_link, now_ms, pid, Conn, Recv};
use bobix_link::command::{self, Refusal, VerifyCtx};
use bobix_link::envelope::{Direction, OpenError, Stream, CURRENT_VERSION};
use bobix_link::keys::{Identity, PublicKeys, SignPublic};
use bobix_link::link::Link;
use bobix_link::messages::*;
use bobix_link::pairing::*;
use bobix_link::relay_frames::{Frame, Push, PushKind};
use bobix_link::{b64, b64_decode};
use std::collections::HashMap;
use std::time::Duration;

struct Item {
    what: DetailKind,
    text: String,
    cut: bool,
    risky: bool,
    questions: Vec<Question>,
}

struct Known {
    link: Link,
    sign: Option<SignPublic>,
    keys: Option<PublicKeys>,
    name: String,
    rights: Rights,
    confirmed: bool,
}

pub struct Desktop {
    id: Identity,
    conn: Conn,
    scenario: Scenario,
    pairing: PairingSession,
    phones: HashMap<String, Known>,
    items: HashMap<String, Item>,
    seen: HashMap<String, CmdResult>,
    confirm_at: Option<tokio::time::Instant>,
    cancelled: bool,
    pub name: String,
    pub print: bool,
    /// What happened, in order, for tests and for the person running the tool.
    pub events: Vec<String>,
}

const COMPUTER_NAME: &str = "Studio Mac (demo)";

fn items_for(s: Scenario) -> HashMap<String, Item> {
    let tool = |text: &str, cut: bool, risky: bool| Item { what: DetailKind::ToolInput, text: text.into(), cut, risky, questions: vec![] };
    let mut m = HashMap::new();
    m.insert("req1".into(), tool("npm test -- --watch=false", false, false));
    m.insert("plan1".into(), Item { what: DetailKind::Plan, text: "# Plan\n\n1. Add the retry\n2. Test it\n".into(), cut: false, risky: false, questions: vec![] });
    m.insert(
        "q1".into(),
        Item {
            what: DetailKind::Question,
            text: "Which database?".into(),
            cut: false,
            risky: false,
            questions: vec![Question { text: "Which database?".into(), options: vec!["SQLite".into(), "Postgres".into()], multi_select: false }],
        },
    );
    m.insert("lines1".into(), Item { what: DetailKind::LastLines, text: "ran 12 tests\nall passed".into(), cut: false, risky: false, questions: vec![] });
    match s {
        Scenario::LongCommand => {
            m.insert("req1".into(), tool(&format!("cat <<'EOF'\n{}", "x".repeat(MAX_TOOL_INPUT - 16) + "\nEOF"), false, false));
        }
        Scenario::CutCommand => {
            m.insert("req1".into(), tool(&"echo 'this input was cut' ".repeat(3), true, false));
        }
        Scenario::Destructive => {
            m.insert("req1".into(), tool("rm -rf ./build ./node_modules", false, true));
        }
        Scenario::LongPlan => {
            let mut plan = String::from("# A long plan\n\n");
            while plan.len() < MAX_PLAN - 40 {
                plan.push_str("- step: change a file and run its tests\n");
            }
            m.insert("plan1".into(), Item { what: DetailKind::Plan, text: plan, cut: false, risky: false, questions: vec![] });
        }
        Scenario::OddInput => {
            m.insert("req1".into(), tool("ls \u{202e}gpj.sh\u{200b}\nrm secret", false, true));
        }
        _ => {}
    }
    m
}

impl Desktop {
    /// Connects, opens a pairing and is ready to print its QR string.
    pub async fn start(relay: &str, scenario: Scenario) -> Result<Desktop, String> {
        let mut conn = Conn::connect(relay).await?;
        let id = Identity::generate();
        conn.login(&id).await?;
        let code = PairingCode::new(relay, id.public()).map_err(|e| format!("{e:?}"))?;
        conn.send(&Frame::PairOpen { relay_token_hash: b64(&code.relay_token_hash()), ttl: 120 }).await?;
        Ok(Desktop {
            id,
            conn,
            scenario,
            pairing: PairingSession::new(code, now_ms()),
            phones: HashMap::new(),
            items: items_for(scenario),
            seen: HashMap::new(),
            confirm_at: None,
            cancelled: false,
            name: COMPUTER_NAME.into(),
            print: false,
            events: vec![],
        })
    }

    pub fn qr(&self) -> String {
        self.pairing.code().to_uri()
    }

    pub fn device_id(&self) -> String {
        self.id.device_id()
    }

    fn note(&mut self, line: String) {
        if self.print {
            println!("{line}");
        }
        self.events.push(line);
    }

    /// Serves until the phone says goodbye or nothing happens for `idle`. Returns what happened.
    pub async fn serve(mut self, idle: Duration) -> Vec<String> {
        loop {
            let wait = match self.confirm_at {
                Some(t) => t.saturating_duration_since(tokio::time::Instant::now()),
                None => idle,
            };
            match self.conn.recv(wait).await {
                Recv::Frame(f) => {
                    if self.on_frame(f).await == Flow::Stop {
                        break;
                    }
                }
                Recv::Timeout if self.confirm_at.is_some() => {
                    self.confirm_at = None;
                    if !self.cancelled {
                        self.confirm_all().await;
                    }
                }
                Recv::Timeout | Recv::Closed => break,
            }
        }
        self.events
    }

    async fn on_frame(&mut self, f: Frame) -> Flow {
        let Frame::Msg { pid, from, body } = f else { return Flow::Go };
        let _ = self.conn.send(&Frame::Ack { pid }).await;
        let Some(packet) = b64_decode(&body) else { return Flow::Go };
        if self.phones.get(&from).is_some_and(|k| k.confirmed) {
            return self.on_packet(&from, &packet).await;
        }
        self.on_hello(&from, &packet).await
    }

    async fn on_hello(&mut self, from: &str, body: &[u8]) -> Flow {
        let Some((phone_box, packet)) = split_hello_body(body) else { return Flow::Go };
        let mut link = new_link(Direction::ToPhone, &self.id, phone_box);
        let Ok(opened) = link.open(packet) else { return Flow::Go };
        let Ok(PhoneMsg::Hello(hello)) = PhoneMsg::from_json(&opened.plaintext) else { return Flow::Go };
        match self.pairing.on_hello(&hello, from, now_ms()) {
            Err(e) => self.note(format!("hello refused: {e:?}")),
            Ok(Offer::Duplicate) => {}
            Ok(Offer::New { phone, sign, name, rights, .. }) => {
                self.note(format!("phone '{name}' asks for rights {:#08b}", rights.to_mask()));
                self.phones.insert(from.into(), Known { link, sign: Some(sign), keys: Some(phone), name, rights, confirmed: false });
                if self.scenario == Scenario::TwoPhones {
                    self.confirm_at = Some(tokio::time::Instant::now() + Duration::from_millis(700));
                } else {
                    self.confirm_all().await;
                }
            }
            Ok(Offer::Cancelled) => {
                self.cancelled = true;
                self.confirm_at = None;
                self.phones.insert(from.into(), Known { link, sign: None, keys: None, name: String::new(), rights: Rights::default(), confirmed: false });
                self.note("pairing cancelled: a second phone answered the same code".into());
                let ids: Vec<String> = self.phones.keys().cloned().collect();
                for id in ids {
                    self.tell(&id, &DesktopMsg::Revoked { why: "pairing cancelled".into() }).await;
                }
                return Flow::Stop;
            }
        }
        Flow::Go
    }

    /// The owner "confirmed": everything the phone asked for is granted (the real desktop shows a checkbox per right).
    async fn confirm_all(&mut self) {
        if !self.pairing.confirm(now_ms()) {
            return;
        }
        let ids: Vec<String> = self.phones.keys().cloned().collect();
        let allow = ids.clone();
        let _ = self.conn.send(&Frame::Peers { allow }).await;
        for id in ids {
            let (computer, rights) = (self.name.clone(), self.phones[&id].rights);
            self.phones.get_mut(&id).unwrap().confirmed = true;
            self.tell(&id, &DesktopMsg::Paired { computer, rights }).await;
        }
    }

    pub async fn tell(&mut self, phone: &str, m: &DesktopMsg) {
        let stream = if matches!(m, DesktopMsg::Home(_)) { Stream::State } else { Stream::Event };
        let Some(k) = self.phones.get_mut(phone) else { return };
        let sealed = k.link.seal(stream, &m.to_json());
        self.send_packet(phone, &sealed.packet, matches!(m, DesktopMsg::Notice(_))).await;
    }

    async fn send_packet(&mut self, phone: &str, packet: &[u8], push: bool) {
        let push = push.then(|| Push { kind: PushKind::Alert, priority: 10, replaces: None });
        let _ = self.conn.send(&Frame::Send { pid: pid(), to: phone.into(), ttl: 600, collapse: None, push, body: b64(packet) }).await;
    }

    fn home(&self, state: ComputerState) -> DesktopMsg {
        let mut waiting: Vec<Waiting> = self
            .items
            .iter()
            .filter(|(_, i)| i.what != DetailKind::LastLines)
            .map(|(id, i)| Waiting {
                id: id.clone(),
                session: "s1".into(),
                what: match i.what {
                    DetailKind::Plan => WaitingKind::Plan,
                    DetailKind::Question => WaitingKind::Question,
                    _ => WaitingKind::Request,
                },
                preview: visible_escapes(&i.text).0.chars().take(60).collect(),
                cut: i.cut,
                risky: i.risky,
                seconds_left: Some(280),
            })
            .collect();
        waiting.sort_by(|a, b| a.id.cmp(&b.id));
        DesktopMsg::Home(Home {
            sent_at: now_ms(),
            computer: Computer { name: self.name.clone(), state, unfinished: 2 },
            sessions: vec![SessionInfo { id: "s1".into(), title: "Fix the login retry".into(), project: "demo-app".into(), state: "waiting".into(), pet: Pet { design: "cat".into(), color: 2 } }],
            groups: vec![Group { group: GroupKind::NeedsYou, ids: waiting.iter().map(|w| w.id.clone()).collect() }],
            waiting,
        })
    }

    async fn after_pairing(&mut self, phone: &str) {
        let state = if self.scenario == Scenario::Asleep { ComputerState::Asleep } else { ComputerState::Online };
        let home = self.home(state);
        self.tell(phone, &home).await;
        match self.scenario {
            Scenario::Asleep => {
                let _ = self.conn.send(&Frame::State { asleep: true }).await;
            }
            Scenario::Offline => {
                self.note("going offline".into());
            }
            Scenario::Revoked => {
                self.tell(phone, &DesktopMsg::Revoked { why: "removed on the computer".into() }).await;
                let _ = self.conn.send(&Frame::Revoke { id: phone.into() }).await;
            }
            Scenario::RelayMoved => {
                self.tell(phone, &DesktopMsg::RelayMoved { url: "wss://new-relay.example/ws".into() }).await;
            }
            Scenario::LatePush => self.late_push(phone).await,
            _ => {}
        }
    }

    /// Delivers packets out of order, replayed and far too old, then one more on time. The receiver handles each by
    /// the rule of its stream and the pairing stays alive.
    async fn late_push(&mut self, phone: &str) {
        let notice = |t: &str| DesktopMsg::Notice(Notice { what: NoticeKind::Done, reference: t.into(), session: "s1".into(), title: t.into(), body: t.into() }).to_json();
        let Some(k) = self.phones.get_mut(phone) else { return };
        let (base, _) = k.link.reserve_counter(Stream::Event);
        for _ in 0..101 {
            k.link.reserve_counter(Stream::Event); // later packets of this desktop count above everything crafted here
        }
        let late: Vec<(&str, u64)> = vec![
            ("on-time", base),
            ("jumped", base + 100),
            ("late-inside-window", base + 50),
            ("replayed", base + 50),
            ("far-too-old", base.saturating_sub(5_000)),
        ];
        let mut packets = vec![];
        for (t, c) in late {
            packets.push(k.link.seal_with(Stream::Event, c, &notice(t)));
        }
        let (s, _) = k.link.reserve_counter(Stream::State);
        for _ in 0..11 {
            k.link.reserve_counter(Stream::State);
        }
        let home = self.home(ComputerState::Online).to_json();
        let k = self.phones.get_mut(phone).unwrap();
        packets.push(k.link.seal_with(Stream::State, s + 10, &home));
        packets.push(k.link.seal_with(Stream::State, s + 5, &home)); // older home after a newer one
        packets.push(k.link.seal_with(Stream::Event, base + 1, &notice("final")));
        for p in packets {
            self.send_packet(phone, &p, true).await;
        }
    }

    async fn on_packet(&mut self, from: &str, packet: &[u8]) -> Flow {
        let Some(k) = self.phones.get_mut(from) else { return Flow::Go };
        let opened = match k.link.open(packet) {
            Ok(o) => o,
            Err(OpenError::Dropped) => {
                self.note("packet dropped by its counter rule".into());
                return Flow::Go;
            }
            Err(e) => {
                self.note(format!("packet refused: {e:?}"));
                return Flow::Go;
            }
        };
        let msg = match PhoneMsg::from_json(&opened.plaintext) {
            Ok(m) if m.validate().is_ok() => m,
            _ => return Flow::Go,
        };
        match msg {
            PhoneMsg::PairedAck => {
                if self.pairing.ack(now_ms()) {
                    self.note("paired".into());
                    self.after_pairing(from).await;
                    if self.scenario == Scenario::Offline {
                        return Flow::Stop; // dropping the connection is the point of this scenario
                    }
                }
            }
            PhoneMsg::Sync { waiting, .. } => {
                let seen = self.seen.clone();
                for r in command::sync_results(&waiting, |cid| seen.get(cid).cloned()) {
                    self.note(format!("sync {} -> {:?}", r.cid, r.outcome));
                    self.tell(from, &DesktopMsg::Result(r)).await;
                }
            }
            PhoneMsg::Detail { id } => {
                if let Some(d) = self.detail(&id) {
                    self.tell(from, &DesktopMsg::Detail(d)).await;
                }
            }
            PhoneMsg::StartPreview { project, iid, mode } => {
                let prompt = format!("Implement ticket {iid} of {project} in {mode:?} mode.\nKeep the change small and add a test.");
                let (known, disk) = (self.scenario != Scenario::FolderMissing, if self.scenario == Scenario::LowDisk { 40_000_000 } else { 80_000_000_000 });
                let m = DesktopMsg::StartPrompt(StartPrompt { project, iid, mode, prompt_hash: shown_hash(&prompt), prompt, folder_known: known, free_disk: disk });
                self.tell(from, &m).await;
            }
            PhoneMsg::Cmd(cmd) => {
                let r = self.on_command(from, opened.counter, &cmd);
                self.note(format!("command {} -> {:?}{}", cmd.cid, r.outcome, r.why.as_deref().map(|w| format!(" ({w})")).unwrap_or_default()));
                self.seen.insert(cmd.cid.clone(), r.clone());
                self.tell(from, &DesktopMsg::Result(r)).await;
            }
            PhoneMsg::Bye { sig } => {
                let k = &self.phones[from];
                let ok = k.keys.is_some_and(|keys| {
                    let bytes = command::bye_bytes(CURRENT_VERSION, from, &self.id.device_id(), opened.counter);
                    b64_decode(&sig).is_some_and(|s| bobix_link::keys::verify_auth(&keys.auth, &bytes, &s))
                });
                self.note(if ok { "bye".into() } else { "bye with a bad signature ignored".into() });
                if ok {
                    return Flow::Stop;
                }
            }
            PhoneMsg::Hello(_) | PhoneMsg::Error { .. } => {}
        }
        Flow::Go
    }

    fn detail(&self, id: &str) -> Option<Detail> {
        let i = self.items.get(id)?;
        let (text, odd) = visible_escapes(&i.text);
        Some(Detail { id: id.into(), what: i.what, shown: shown_hash(&text), text, odd, cut: i.cut, questions: i.questions.clone() })
    }

    /// The decision for a verified or refused command.
    fn on_command(&mut self, from: &str, counter: u64, cmd: &command::Command) -> CmdResult {
        if let Some(stored) = self.seen.get(&cmd.cid) {
            return stored.clone(); // a repeated cid gets its first result, nothing is done twice
        }
        let k = &self.phones[from];
        let ctx = VerifyCtx {
            version: CURRENT_VERSION,
            phone_id: from,
            computer_id: &self.id.device_id(),
            sign_key: k.sign.as_ref().expect("a confirmed phone has a sign key"),
            counter,
            now_ms: now_ms(),
        };
        let body = match command::verify(&ctx, cmd) {
            Ok(b) => b,
            Err(r @ (Refusal::Clock | Refusal::Computer | Refusal::Signature | Refusal::Encoding)) => return CmdResult::refused(&cmd.cid, r.why()),
        };
        let r = &k.rights;
        let allowed = match body {
            CommandBody::Answer { .. } => r.answer,
            CommandBody::Plan { .. } => r.plan,
            CommandBody::Question { .. } => r.question,
            CommandBody::Start { .. } => r.start,
            CommandBody::Stop { .. } => r.stop,
        };
        if !allowed {
            return CmdResult::refused(&cmd.cid, "right");
        }
        let held = |id: &str| self.detail(id).map(|d| d.text);
        let ok = CmdResult::new(&cmd.cid, Outcome::Ok);
        match &body {
            CommandBody::Answer { id, choice, shown } => {
                let Some(text) = held(id) else { return CmdResult::refused(&cmd.cid, "unknown") };
                if !command::shown_matches(&text, shown) {
                    return CmdResult::new(&cmd.cid, Outcome::Stale);
                }
                match self.scenario {
                    Scenario::AnsweredOnComputer => CmdResult { by: Some("computer".into()), ..CmdResult::new(&cmd.cid, Outcome::LatePc) },
                    Scenario::Expired => CmdResult::new(&cmd.cid, Outcome::Expired),
                    _ if self.items[id].cut && *choice == Choice::Allow => CmdResult::refused(&cmd.cid, "cut"),
                    _ => ok,
                }
            }
            CommandBody::Plan { id, shown, .. } | CommandBody::Question { id, shown, .. } => match held(id) {
                None => CmdResult::refused(&cmd.cid, "unknown"),
                Some(t) if !command::shown_matches(&t, shown) => CmdResult::new(&cmd.cid, Outcome::Stale),
                Some(_) => ok,
            },
            CommandBody::Start { project, iid, mode, prompt_hash } => {
                let prompt = format!("Implement ticket {iid} of {project} in {mode:?} mode.\nKeep the change small and add a test.");
                match self.scenario {
                    Scenario::FolderMissing => CmdResult::refused(&cmd.cid, "folder"),
                    Scenario::LowDisk => CmdResult::refused(&cmd.cid, "disk"),
                    _ if &shown_hash(&prompt) != prompt_hash => CmdResult::new(&cmd.cid, Outcome::Stale),
                    _ => ok,
                }
            }
            CommandBody::Stop { .. } => ok,
        }
    }
}

#[derive(PartialEq, Eq)]
enum Flow {
    Go,
    Stop,
}
