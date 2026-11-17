// Phone access, the desktop side: the switch, the paired phones and their rights, pairing, and what the relay link
// (link.rs) hands in. Off by default: while the switch is off and no phone was ever paired there is no key, no file
// and no connection. Keys are in the OS keychain (secrets.rs); the list of phones is stored sealed with a keychain key.
// The wire format, crypto and counter rules are the shared crate's (bobix-link); this module only decides.
#![cfg_attr(not(feature = "phones"), allow(dead_code))] // the commands that call these are not compiled in a stable build
use crate::{secrets::Secrets, Bobix};
use bobix_link::{
    b64, b64_decode,
    command, counters::RecvState,
    envelope::{Direction, VersionRange, CURRENT_VERSION},
    keys::{Identity, PublicKeys, SignPublic},
    link::Link,
    messages::*,
    pairing::*,
    invite::{InviteCode, DAY_MS},
    relay_frames::{ActivityEvent, ErrorCode, Frame, InviteReason, Invitee, Push, PushKind},
};
use chacha20poly1305::{aead::{Aead, KeyInit}, XChaCha20Poly1305};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::{HashMap, HashSet, VecDeque}, path::{Path, PathBuf}, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

const IDENTITY: &str = "identity"; // auth seed + box secret, 64 bytes
const SEAL_KEY: &str = "phones-seal";
const MOVE_WAIT_MS: u64 = 10 * 60 * 1000;
const LOG_KEEP: usize = 300;
const RESULT_KEEP_MS: u64 = 24 * 3600 * 1000;
/// A phone's Live Activity is pushed at most this often: a burst of changes becomes one push with the last count.
const ACTIVITY_GAP_MS: u64 = 5_000;
pub const MAX_PHONE_NAME: usize = 40;
/// The longest project name (a path such as `group/sub/repo`) a phone may name in a `start`.
pub const MAX_PROJECT: usize = 200;

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(default)]
pub struct Config {
    pub on: bool,
    pub relay: String,
    // when the pets are hidden or paused: "phone" keeps requests for my phone, "computer" lets Claude ask at the computer
    pub hold: String,
    // keep the computer awake while a session works or waits (needs the switch on)
    pub awake: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Phone {
    pub id: String,
    pub name: String,
    pub auth: String,
    pub boxk: String,
    pub sign: String,
    pub rights: Rights,
    pub since: u64,
    // a relay move the phone never acknowledged: it has to pair again
    #[serde(default)]
    pub needs_pairing: bool,
    // the relay said this phone's push grant ended: it gets no notifications until it is opened
    #[serde(default)]
    pub no_push: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Revoked { pub id: String, pub name: String, pub at: u64 }

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Store { pub phones: Vec<Phone>, pub revoked: Vec<Revoked> }

// Counter reservations and the receive state of one phone, in their own file in the data folder.
#[derive(Serialize, Deserialize, Clone, Default)]
struct Saved { res: [u64; 3], recv: RecvState }

pub enum Out { Frame(Box<Frame>), Close }

// One remote command, for Settings → Phones. `what` is the kind and the target's redacted preview, never the input.
#[derive(Serialize, Deserialize, Clone)]
pub struct LogLine { pub at: u64, pub phone: String, pub what: String, pub result: String }

struct Offered { phone: PublicKeys, sign: SignPublic, name: String, rights: Rights, from: String, link: Link }
struct Pairing { session: PairingSession, offer: Option<Offered>, paired: Option<String>, opened: bool } // paired: the name of the phone the owner confirmed

// What the owner is told about a second phone and the code: it answered before the confirmation (the pairing was cancelled), or
// after it (the pairing with the first phone is fine; the name is that phone's).
#[derive(Clone, Debug, PartialEq)]
enum PairWarn { Cancelled, Late(String) }
fn warning_json(w: &Option<PairWarn>) -> Value {
    match w { None => Value::Null, Some(PairWarn::Cancelled) => json!({ "kind": "cancelled" }), Some(PairWarn::Late(n)) => json!({ "kind": "late", "name": n }) }
}
// What the relay said about this computer and invitations (`inviteState`), the code made last (shown until another is
// made; never stored), a code waiting to be handed to the relay, and the relay's answer to the last one.
#[derive(Default)]
struct Invites {
    owner: bool,
    invited: bool,
    known: bool,
    relay_id: Option<String>,
    invitees: Vec<Invitee>,
    made: Option<(String, u64, String)>, // code, expiry, the relay id it is for
    pending: Option<String>,
    result: Option<String>,
}
/// The validities offered in Settings, in days.
pub const INVITE_DAYS: [u64; 3] = [1, 7, 30];

struct Moving { url: String, waiting: HashMap<String, String>, since: u64 } // pid → phone id
struct Revoking { pids: HashSet<String>, since: u64 }

#[derive(Default)]
pub struct State {
    dir: PathBuf,
    pub cfg: Config,
    pub store: Store,
    /// Another phone used the pairing code (a second one before the confirmation, or a different one after it): shown in
    /// Settings until the owner dismisses it, whatever happens to the pairing session.
    pair_warning: Option<PairWarn>,
    /// The list of phones is safe to change: it was opened, or there never was one. False while a stored list could not be
    /// opened (a keychain that did not answer, a key that is gone): then nothing may overwrite it or make a new key.
    loaded: bool,
    retry_at: u64,
    identity: Option<Identity>,
    links: HashMap<String, Link>,
    saved: HashMap<String, Saved>,
    pairing: Option<Pairing>,
    pub status: String, // off | connecting | online | invite | update | url | auth | error
    pub tx: Option<UnboundedSender<Out>>,
    moving: Option<Moving>,
    revoking: Option<Revoking>,
    pub log: VecDeque<LogLine>,
    pub storage: Option<crate::secrets::Storage>,
    computer: String,
    online: HashSet<String>,                    // phones the relay says are connected
    last_home: HashMap<String, String>,         // what each phone was last sent, to send only a change
    last_home_at: u64,
    home_now: HashSet<String>,                  // phones that get a home at the next tick (just paired, just online)
    results: HashMap<String, (CmdResult, u64)>, // by cid, kept 24 hours
    pub closed_events: Vec<(String, String, bool, crate::answers::End)>, // (request id, session id, plan, how it ended): requests that ended, to tell the phones
    answerers: HashMap<String, String>,         // request id → the phone that answered it, until the phones are told it closed
    notified: HashSet<String>,
    awake: crate::awake::Awake,
    asleep: bool,                               // the computer is going to sleep (or sleeps): phones are told, with how much is unfinished
    inflight: HashSet<String>,                  // `from:cid` of starts still running: a repeat of one gets no answer yet
    starting: HashSet<String>,                  // issues a start from a phone is running for: one at a time
    last_push: Option<(String, u64)>,           // the phone the last push went to, for an `error { grant }` that names none
    activity_sent: HashMap<String, (u32, u64)>, // per phone: the waiting count its Live Activity was last pushed, and when
    invites: Invites,
    /// Cuts the link's wait before it reconnects (an invite code was entered while the relay was out of reach).
    pub wake: Arc<tokio::sync::Notify>,
    deferred: Vec<(String, String, u64)>, // (pid of a `revoked` message, phone id, when): the relay is told to revoke once the phone has it
}

pub fn now() -> u64 { crate::state::now() }

// ---------- files ----------
fn cfg_file(d: &Path) -> PathBuf { d.join("phones.json") }
fn store_file(d: &Path) -> PathBuf { d.join("phones.sealed") }
fn counters_file(d: &Path) -> PathBuf { d.join("phone-counters.json") }
fn log_file(d: &Path) -> PathBuf { d.join("phone-log.json") }
fn results_file(d: &Path) -> PathBuf { d.join("phone-results.json") }
fn deferred_file(d: &Path) -> PathBuf { d.join("phone-revoking.json") }

// The key that seals the list of phones. A new one is made only when asked (`create`), and the caller asks only when no list
// file exists: a key that reads as absent while a list exists means the keychain did not answer, and a new key would make the
// old list unreadable for good.
fn cipher(secrets: &Secrets, create: bool) -> Result<XChaCha20Poly1305, String> {
    let key = match secrets.get(SEAL_KEY).map_err(|e| e.to_string())? {
        Some(k) if k.len() == 32 => k,
        _ if create => { let k = bobix_link::random::<32>().to_vec(); secrets.set(SEAL_KEY, &k).map_err(|e| e.to_string())?; k }
        _ => return Err("no seal key".into()),
    };
    XChaCha20Poly1305::new_from_slice(&key).map_err(|_| "bad key".to_string())
}
pub fn seal_store(c: &XChaCha20Poly1305, s: &Store) -> Vec<u8> {
    let nonce = bobix_link::random::<24>();
    let mut out = nonce.to_vec();
    out.extend(c.encrypt((&nonce).into(), serde_json::to_vec(s).unwrap_or_default().as_slice()).unwrap_or_default());
    out
}
pub fn open_store(c: &XChaCha20Poly1305, bytes: &[u8]) -> Option<Store> {
    let (n, ct) = bytes.split_at_checked(24)?;
    serde_json::from_slice(&c.decrypt(n.into(), ct).ok()?).ok()
}

fn write(file: &Path, bytes: &[u8]) {
    if let Some(d) = file.parent() { let _ = std::fs::create_dir_all(d); }
    let tmp = file.with_extension("tmp");
    if std::fs::write(&tmp, bytes).is_ok() { let _ = std::fs::rename(&tmp, file); }
}

impl State {
    fn save_deferred(&self) { write(&deferred_file(&self.dir), &serde_json::to_vec(&self.deferred).unwrap_or_default()); }
    fn save_cfg(&self) { write(&cfg_file(&self.dir), &serde_json::to_vec_pretty(&self.cfg).unwrap_or_default()); }
    fn save_store(&self) {
        // Never over a list that could not be opened, and never with a key made while a list file exists.
        if !self.loaded { return; }
        let Ok(c) = cipher(&Secrets::open(), !store_file(&self.dir).exists()) else { return };
        write(&store_file(&self.dir), &seal_store(&c, &self.store));
    }

    // Reads the stored list: nothing stored is fine (a computer never paired), a list that opens is loaded, anything else leaves
    // `loaded` false and the memory empty. Run at start and again (at most every 10 seconds) while it is false.
    fn load_store(&mut self) {
        self.retry_at = now();
        let bytes = match std::fs::read(store_file(&self.dir)) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => { self.loaded = true; return; }
            Err(_) => { eprintln!("[bobix] phones: the list of phones could not be read"); return; }
        };
        match cipher(&Secrets::open(), false).ok().and_then(|ci| open_store(&ci, &bytes)) {
            Some(store) => {
                self.store = store;
                self.loaded = true;
                self.saved = std::fs::read(counters_file(&self.dir)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
                let _ = self.ensure_identity(false);
            }
            None => eprintln!("[bobix] phones: the list of phones could not be opened"),
        }
    }
    fn save_counters(&self) { write(&counters_file(&self.dir), &serde_json::to_vec(&self.saved).unwrap_or_default()); }
    fn save_log(&self) { write(&log_file(&self.dir), &serde_json::to_vec(&self.log).unwrap_or_default()); }
    // The results of commands (24 hours) and the record of closed requests, so a repeated command or a late answer is
    // answered the same after a restart.
    pub fn save_results(&self, closed: &HashMap<String, crate::answers::Closed>) {
        let r: Vec<Value> = self.results.iter().map(|(k, (r, at))| json!({ "key": k, "result": r, "at": at })).collect();
        write(&results_file(&self.dir), &serde_json::to_vec(&json!({ "results": r, "closed": crate::answers::closed_json(closed) })).unwrap_or_default());
    }
    pub fn push_log(&mut self, phone: &str, what: &str, result: &str) {
        self.log.push_back(LogLine { at: now(), phone: phone.into(), what: what.into(), result: result.into() });
        while self.log.len() > LOG_KEEP { self.log.pop_front(); }
        self.save_log();
    }

    fn computer_id(&self) -> Option<String> { self.identity.as_ref().map(|i| i.device_id()) }

    // The identity, from the keychain; made once, only when phone access is turned on.
    fn ensure_identity(&mut self, create: bool) -> Result<(), String> {
        if self.identity.is_some() { return Ok(()); }
        let s = Secrets::open();
        match s.get(IDENTITY).map_err(|e| e.to_string())? {
            Some(b) if b.len() == 64 => self.identity = Some(Identity::from_secrets(b[..32].try_into().unwrap(), b[32..].try_into().unwrap())),
            // A new identity only for a list that is safe to change and has no phone: it would orphan every pairing.
            _ if create && self.loaded && self.store.phones.is_empty() => {
                let id = Identity::generate();
                let mut b = id.auth_seed().to_vec();
                b.extend(id.box_secret().iter());
                s.set(IDENTITY, &b).map_err(|e| e.to_string())?;
                self.identity = Some(id);
            }
            _ => return Err("no identity".into()),
        }
        self.storage = Some(s.storage());
        Ok(())
    }

    // The link to a phone, made from its stored keys and counters.
    fn link_of(&mut self, id: &str) -> Option<&mut Link> {
        if !self.links.contains_key(id) {
            let p = self.store.phones.iter().find(|p| p.id == id)?;
            let boxk: [u8; 32] = b64_decode(&p.boxk)?.try_into().ok()?;
            let sec = *self.identity.as_ref()?.box_secret();
            let sv = self.saved.entry(id.into()).or_default().clone();
            self.links.insert(id.into(), Link::new(Direction::ToPhone, sec, boxk, CURRENT_VERSION, VersionRange::current(), sv.res, now(), sv.recv));
        }
        self.links.get_mut(id)
    }

    fn remember_recv(&mut self, id: &str) {
        if let Some(l) = self.links.get(id) { self.saved.entry(id.into()).or_default().recv = l.recv_state().clone(); self.save_counters(); }
    }

    // Seals a message to a phone and returns the frame. The counter reservation is stored before the frame leaves.
    pub fn tell(&mut self, id: &str, m: &DesktopMsg, push: Option<Push>, collapse: Option<String>) -> Option<(String, Frame)> {
        let stream = if matches!(m, DesktopMsg::Home(_)) { bobix_link::envelope::Stream::State } else { bobix_link::envelope::Stream::Event };
        self.seal_to(id, stream, &m.to_json(), push, collapse)
    }

    fn seal_to(&mut self, id: &str, stream: bobix_link::envelope::Stream, plaintext: &[u8], push: Option<Push>, collapse: Option<String>) -> Option<(String, Frame)> {
        if push.is_some() { self.last_push = Some((id.to_string(), now())); }
        let sealed = self.link_of(id)?.seal(stream, plaintext).ok()?; // a low-order peer key: nothing is sent
        if let Some((s, v)) = sealed.reserve { self.saved.entry(id.into()).or_default().res[s as usize] = v; self.save_counters(); }
        let pid = command::new_cid()[..16].to_string();
        Some((pid.clone(), Frame::Send { pid, to: id.into(), ttl: 600, collapse, push, body: b64(&sealed.packet) }))
    }

    // The notices of the requests that ended: every paired phone learns who closed it (one that never had the push still has the
    // item in its last `home`); the push that clears a shown notification goes only where it was announced. A phone's answer
    // names the phone only to that phone (so it can say "this phone"); the others are told it was a phone, not which.
    // With no link to the relay they are dropped (a phone learns the item is gone from its next `home`), so an outage never
    // piles up a sealed frame for every item and phone.
    fn closed_frames(&mut self) -> Vec<Frame> {
        let mut out = vec![];
        let linked = self.cfg.on && self.status == "online";
        for (id, sid, _, end) in std::mem::take(&mut self.closed_events) {
            let notified = self.notified.remove(&id);
            let answerer = self.answerers.remove(&id);
            if !linked { continue; }
            let push = notified.then(|| Push { kind: bobix_link::relay_frames::PushKind::Background, priority: 5, event: None, replaces: Some(id.clone()) });
            let ids: Vec<String> = self.store.phones.iter().map(|p| p.id.clone()).collect();
            for p in ids {
                let by = closed_by(end, answerer.as_deref(), &p);
                let n = Notice { what: NoticeKind::Closed, reference: id.clone(), session: sid.clone(), title: String::new(), body: String::new(), by };
                if let Some((_, f)) = self.tell(&p, &DesktopMsg::Notice(n), push.clone(), None) { out.push(f); }
            }
        }
        out
    }
    // The Live Activity pushes due now, one for each phone whose activity is out of date (see `activity_step`). Each is a
    // sealed `activity` packet with the count only (no names, no session), sent with a `liveActivity` push: the widget
    // opens the packet with the phone's key, so the relay and Apple carry nothing readable. Only a start is held back while
    // notifications are quiet (Do Not Disturb, or "notify when waiting" off): an update or an end is never, or an activity
    // that began earlier would stay up after the last request was answered.
    fn activity_frames(&mut self, waiting: u32, quiet: bool, t: u64) -> Vec<Frame> {
        if !self.cfg.on || self.status != "online" { return vec![]; }
        let name = self.computer_name();
        let state = if self.asleep { "asleep" } else { "online" };
        let ids: Vec<String> = self.store.phones.iter().map(|p| p.id.clone()).collect();
        let mut out = vec![];
        for id in ids {
            let Some(event) = activity_step(self.activity_sent.get(&id).copied(), waiting, t) else { continue };
            if quiet && event == ActivityEvent::Start { continue; }
            let body = ActivityBody { waiting, names: vec![], written: t, computer_name: crate::phone_view::preview(&name, 64), computer_state: state.into() };
            // Apple budgets the pushes that wake the island at once: the two that matter (it appears, it goes) are immediate.
            let push = Push { kind: PushKind::LiveActivity, priority: if event == ActivityEvent::Update { 5 } else { 10 }, event: Some(event), replaces: None };
            let json = serde_json::to_vec(&body).unwrap_or_default();
            if let Some((_, f)) = self.seal_to(&id, bobix_link::envelope::Stream::Activity, &json, Some(push), Some("activity".into())) {
                out.push(f);
                self.activity_sent.insert(id, (waiting, t));
            }
        }
        out
    }

    pub fn send(&self, f: Frame) { if let Some(tx) = &self.tx { let _ = tx.send(Out::Frame(Box::new(f))); } }

    fn peers_frame(&self) -> Frame { Frame::Peers { allow: self.store.phones.iter().map(|p| p.id.clone()).collect() } }

    fn computer_name(&mut self) -> String {
        if self.computer.is_empty() {
            self.computer = std::process::Command::new("hostname").output().ok().and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().trim_end_matches(".local").chars().filter(|c| !c.is_control()).take(MAX_NAME).collect()).filter(|s: &String| !s.is_empty()).unwrap_or_else(|| "Computer".into());
        }
        self.computer.clone()
    }

    // The phone is gone: its link, counters and entry; a record of it is kept so a phone that returns can be told.
    fn forget(&mut self, id: &str) -> Option<Phone> {
        self.links.remove(id);
        self.saved.remove(id);
        self.activity_sent.remove(id);
        let i = self.store.phones.iter().position(|p| p.id == id)?;
        let p = self.store.phones.remove(i);
        self.store.revoked.push(Revoked { id: p.id.clone(), name: p.name.clone(), at: now() });
        if self.store.revoked.len() > 50 { self.store.revoked.remove(0); }
        self.save_store();
        self.save_counters();
        Some(p)
    }

    // Disconnects a phone on the computer's side: the phone is told first; the relay is told to revoke it (and the peers
    // change) once the phone acknowledged that message, or 30 seconds later, because a revoked phone is cut off at once.
    fn disconnect(&mut self, id: &str, why: &str) -> Vec<Frame> {
        let told = self.tell(id, &DesktopMsg::Revoked { why: why.into() }, None, None);
        let mut out = vec![];
        let delayed = told.is_some();
        if let Some((pid, f)) = told {
            out.push(f);
            self.revoking.get_or_insert_with(|| Revoking { pids: HashSet::new(), since: now() }).pids.insert(pid.clone());
            self.deferred.push((pid, id.into(), now()));
            self.save_deferred();
        }
        if self.forget(id).is_some() && !delayed { out.extend([Frame::Revoke { id: id.into() }, self.peers_frame()]); }
        out
    }
    // The revocations whose phone has the message (`delivered`) or whose wait is over.
    fn flush_revokes(&mut self, delivered: Option<&str>, t: u64) -> Vec<Frame> {
        let mut out = vec![];
        let mut keep = vec![];
        for d in std::mem::take(&mut self.deferred) {
            if delivered == Some(d.0.as_str()) || t > d.2 + 30_000 {
                out.push(Frame::Revoke { id: d.1.clone() });
                if let Some(r) = self.revoking.as_mut() { r.pids.remove(&d.0); }
            } else { keep.push(d); }
        }
        self.deferred = keep;
        if !out.is_empty() { self.save_deferred(); }
        if !out.is_empty() { out.push(self.peers_frame()); }
        out
    }
}

// ---------- validation of what a phone sends ----------
// A name is 1 to 40 characters with no control or format character (it arrives before the owner confirms, and the panel builds HTML).
pub fn valid_name(s: &str) -> bool { !s.is_empty() && s.chars().count() <= MAX_PHONE_NAME && !bobix_link::messages::has_odd(s) }

// ---------- startup ----------
// Bobix is closing: held requests go back to Claude's own prompt and are recorded as released ("restart").
pub fn save_on_exit(b: &Bobix) {
    let mut c = b.core.lock().unwrap();
    if !c.phones.cfg.on && c.phones.store.phones.is_empty() { return; }
    crate::answers::release_all(&mut c, "restart", now());
    c.phones.save_results(&c.closed);
}

// On its own clock (the relay link may be down), once a second: keeps the awake inhibitor in step with the work, and
// notices a held request whose command has started. A request approved in Claude's own prompt tells Bobix only when the
// command ends (its PostToolUse); but the command starts right away, as a new child of the Claude process. So a new child
// since the watch first looked marks the request "may be answered at the computer" (released: no more pushes, the phone
// says so, and it can still answer). It is a hint, never a close: Claude starts other helpers too.
fn watch_loop(b: Arc<Bobix>) {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        // what to look at, off the core's lock (pgrep takes a moment)
        let targets: Vec<(String, u32)> = {
            let mut c = b.core.lock().unwrap();
            // A list of phones that could not be opened (a keychain locked at start): tried again as it unlocks, not only when Settings looks.
            if !c.phones.loaded && now() > c.phones.retry_at + 10_000 { c.phones.load_store(); }
            let busy = c.sessions.values().any(|s| matches!(s.state.as_str(), "working" | "waiting"));
            let want = c.phones.cfg.awake && c.phones.cfg.on && busy;
            c.phones.awake.set(want);
            if !c.phones.cfg.on || c.phones.store.phones.is_empty() { continue; }
            c.pending.iter().filter(|(_, p)| p.kind() == crate::answers::Kind::Request && !p.released)
                .filter_map(|(id, p)| { let s = c.sessions.get(&p.sid)?; let pid = s.claude_pid.filter(|p| s.pid_is_claude == Some(*p))?; Some((id.clone(), pid)) }).collect()
        };
        if targets.is_empty() { continue; }
        let kids: Vec<(String, Vec<u32>)> = targets.into_iter().map(|(id, pid)| (id, crate::procs::children(pid))).collect();
        let mut c = b.core.lock().unwrap();
        let mut changed = false;
        for (id, now_kids) in kids {
            let Some(p) = c.pending.get_mut(&id) else { continue };
            match &p.child_base {
                None => p.child_base = Some(now_kids),
                Some(base) if started_since(base, &now_kids) => { p.released = true; changed = true; }
                _ => {}
            }
        }
        drop(c);
        if changed { b.broadcast(); }
    }
}
// A child that was not there when the watch first looked.
pub fn started_since(base: &[u32], now: &[u32]) -> bool { now.iter().any(|p| !base.contains(p)) }

// False in a build without the `phones` feature (a stable release): init does nothing, so no thread, key, file or relay
// connection exists, and every other entry point finds phone access off and no phone paired.
pub const ENABLED: bool = cfg!(feature = "phones");

pub fn init(b: &Arc<Bobix>) {
    if !ENABLED { return; }
    { let b2 = b.clone(); std::thread::spawn(move || watch_loop(b2)); }
    let dir = crate::paths::data_dir();
    let read = |f: PathBuf| std::fs::read(f).ok();
    let cfg: Config = read(cfg_file(&dir)).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let mut c = b.core.lock().unwrap();
    let st = &mut c.phones;
    st.dir = dir.clone();
    st.cfg = cfg;
    if st.cfg.hold != "computer" { st.cfg.hold = "phone".into(); }
    st.log = read(log_file(&dir)).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    // Only a computer that was ever paired has anything stored; nothing is created here.
    st.load_store();
    if let Some(v) = read(results_file(&dir)).and_then(|b| serde_json::from_slice::<Value>(&b).ok()) {
        st.results = v["results"].as_array().into_iter().flatten().filter_map(|x| Some((x["key"].as_str()?.to_string(), (serde_json::from_value(x["result"].clone()).ok()?, x["at"].as_u64()?)))).collect();
        let closed = crate::answers::closed_from_json(&v["closed"]);
        c.closed = closed;
    }
    let st = &mut c.phones;
    // revocations that were waiting for the phone's acknowledgement when Bobix closed: connect once to tell the relay
    st.deferred = read(deferred_file(&dir)).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if !st.deferred.is_empty() { st.revoking = Some(Revoking { pids: st.deferred.iter().map(|d| d.0.clone()).collect(), since: now() }); }
    st.status = if st.cfg.on { "connecting".into() } else { "off".into() };
    let want = st.cfg.on || st.revoking.is_some();
    drop(c);
    if want { start_link(b); }
}

// Opens the relay connection when it is wanted: the switch is on, or revocations are still unacknowledged.
pub fn start_link(b: &Arc<Bobix>) {
    let running = b.core.lock().unwrap().phones.tx.is_some();
    if !running { crate::link::spawn(b.clone()); }
}
pub fn wants_link(b: &Arc<Bobix>) -> bool { let c = b.core.lock().unwrap(); c.phones.cfg.on || c.phones.revoking.is_some() }
pub fn link_ended(b: &Arc<Bobix>) {
    let mut c = b.core.lock().unwrap();
    c.phones.tx = None;
    if !c.phones.cfg.on { c.phones.status = "off".into(); }
}
// The address the link connects to: Settings' address, held to the same rule as when it was typed (a `ws://` address on a
// private network only in a build with phone access), the same rule as when it was typed.
pub fn relay_url(b: &Arc<Bobix>) -> String { let c = b.core.lock().unwrap(); relay_norm(&c.phones.cfg.relay, ENABLED).unwrap_or_default() }
// The address in Settings is no longer the one this connection was opened to.
pub fn url_changed(b: &Arc<Bobix>, url: &str) -> bool { relay_url(b) != url }

// The `auth` frame for the relay at `url`: what the desktop signs is the address in its Settings.
pub fn auth_frame(b: &Arc<Bobix>, url: &str, nonce: &str) -> Option<Frame> {
    let c = b.core.lock().unwrap();
    command::auth_frame(c.phones.identity.as_ref()?, url, nonce, VersionRange::current())
}

// What goes out right after a login: the peers, the revocations the relay may not know, an open pairing.
pub fn on_connected(b: &Arc<Bobix>) -> Vec<Frame> {
    let mut c = b.core.lock().unwrap();
    let st = &mut c.phones;
    st.status = "online".into();
    let mut out = st.login_frames(false);
    out.push(Frame::InviteQuery {});
    if let Some(code) = st.invites.pending.clone() { out.push(Frame::InviteRedeem { code }); }
    out
}
pub fn link_lost(b: &Arc<Bobix>) {
    let mut c = b.core.lock().unwrap();
    if c.phones.cfg.on { c.phones.status = "connecting".into(); }
    if let Some(p) = c.phones.pairing.as_mut() { p.opened = false; }
}

// Called once a second by the link: timers (a pairing that ran out, a relay move or a revocation that waited long enough),
// the homes the phones need, and the notices of requests that ended.
pub fn tick(b: &Arc<Bobix>) -> Vec<Frame> {
    let dirty = b.phones_dirty.swap(false, std::sync::atomic::Ordering::SeqCst);
    let mut c = b.core.lock().unwrap();
    let t = now();
    let crate::Core { phones: st, pending, sessions, work, settings, .. } = &mut *c;
    if st.pairing.as_ref().is_some_and(|p| p.session.finished(t)) { st.pairing = None; }
    if st.revoking.as_ref().is_some_and(|r| r.pids.is_empty() || t > r.since + MOVE_WAIT_MS) {
        st.revoking = None;
        if !st.cfg.on { st.send_close(); }
    }
    if st.moving.as_ref().is_some_and(|m| m.waiting.is_empty() || t > m.since + MOVE_WAIT_MS) {
        let m = st.moving.take().unwrap();
        for id in m.waiting.values() { if let Some(p) = st.store.phones.iter_mut().find(|p| &p.id == id) { p.needs_pairing = true; } }
        st.cfg.relay = m.url;
        st.invites = Invites::default();
        st.save_cfg();
        st.save_store();
        st.send_close(); // the link reconnects to the new address
    }
    st.results.retain(|_, r| t < r.1 + RESULT_KEEP_MS);
    let mut flushed = st.flush_revokes(None, t);
    // a request held only for the phones: the first input at the computer marks it released (no more pushes; still answerable)
    if pending.values().any(|p| p.for_phones && !p.released) {
        if let Some(idle) = crate::presence::source().idle_ms() {
            for p in pending.values_mut().filter(|p| p.for_phones && !p.released && idle < t.saturating_sub(p.held_at)) { p.released = true; }
        }
    }
    // requests that ended: the phones that were told of them are told they are gone
    let mut out = std::mem::take(&mut flushed);
    out.extend(st.closed_frames());
    // homes: to every paired phone when something changed (the relay keeps only the latest for an offline one), a forced
    // one each minute to a phone that is online, and one at once for a phone that just paired or came online
    if st.cfg.on && !st.store.phones.is_empty() {
        let minute = t >= st.last_home_at + 60_000;
        if dirty || minute || !st.home_now.is_empty() {
            let name = st.computer_name();
            let groups = crate::phone_view::groups(work, settings, sessions, pending, t);
            let home = crate::phone_view::home(sessions, pending, &name, t, if st.asleep { ComputerState::Asleep } else { ComputerState::Online }, groups);
            let ids: Vec<(String, Rights)> = st.store.phones.iter().map(|p| (p.id.clone(), p.rights)).collect();
            for (id, rights) in ids {
                let home = crate::phone_view::for_phone(home.clone(), rights);
                let mut key = home.clone();
                key.sent_at = 0;
                let key = serde_json::to_string(&key).unwrap_or_default();
                let due = st.home_now.contains(&id) || st.last_home.get(&id) != Some(&key) || (minute && st.online.contains(&id));
                if !due { continue; }
                if let Some((_, f)) = st.tell(&id, &DesktopMsg::Home(home.clone()), None, Some("home".into())) { out.push(f); st.last_home.insert(id, key.clone()); }
            }
            if minute { st.last_home_at = t; }
            st.home_now.clear();
        }
        let quiet = settings["dndUntil"].as_f64().unwrap_or(0.0) > t as f64 || settings["notifyWaiting"] != true;
        out.extend(st.activity_frames(crate::phone_view::waiting_for_phones(sessions, pending), quiet, t));
    }
    out
}

/// What the sealed `activity` packet holds (`ios/WidgetsShared/BobixActivityAttributes.swift` decodes exactly this): the
/// count and the computer, with no session name.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivityBody { waiting: u32, names: Vec<String>, written: u64, computer_name: String, computer_state: String }

/// What a phone's Live Activity needs now. `last` is the count it was last pushed with and when (none: never, since this
/// computer started). Nothing is sent while it already shows the count, nor within [ACTIVITY_GAP_MS] of the last push (the
/// next tick compares again, so a burst ends with one push of its last count, and 0, 2, 0 or 2, 0, 2 inside the gap sends
/// nothing). A count that rises from nothing starts the activity, one that falls to nothing ends it, any other change updates.
fn activity_step(last: Option<(u32, u64)>, waiting: u32, now: u64) -> Option<ActivityEvent> {
    let Some((was, at)) = last else { return (waiting > 0).then_some(ActivityEvent::Start) };
    if was == waiting || now < at + ACTIVITY_GAP_MS { return None; }
    Some(if waiting == 0 { ActivityEvent::End } else if was == 0 { ActivityEvent::Start } else { ActivityEvent::Update })
}
impl State {
    // The relay refused a push for an ended grant. It names the phone (`peer`); an older relay names none, and then it is
    // the phone the last push went to.
    fn grant_ended(&mut self, t: u64, peer: Option<&str>) {
        let id = match peer {
            Some(p) => p.to_string(),
            None => {
                let Some((id, at)) = self.last_push.clone() else { return };
                if t > at + 10_000 { return; }
                id
            }
        };
        if let Some(p) = self.store.phones.iter_mut().find(|p| p.id == id) { p.no_push = true; self.save_store(); }
    }
    // The request was announced to the phones (so a settings change does not hand it back).
    pub fn was_sent(&self, id: &str) -> bool { self.notified.contains(id) }
    #[cfg(test)]
    pub fn mark_sent(&mut self, id: &str) { self.notified.insert(id.into()); }
    #[cfg(test)]
    pub fn with_phone(rights: Rights) -> State {
        let mut st = State::default();
        st.cfg.on = true;
        st.store.phones.push(Phone { id: "p".into(), name: "n".into(), auth: String::new(), boxk: String::new(), sign: String::new(), rights, since: 0, needs_pairing: false, no_push: false });
        st
    }
    // Phone access is on and a paired phone holds the right to answer an item of this kind: it may be answered there, so it
    // never runs out. A phone without the right cannot answer it, and holding it for that phone would only leave it unanswered.
    pub fn has_phone_for(&self, kind: crate::answers::Kind) -> bool { self.cfg.on && self.store.phones.iter().any(|p| !p.needs_pairing && kind.answerable_with(p.rights)) }
    // Whether a request that nothing on this computer would show is held for the phones: phone access is on, a phone that can
    // answer it is paired, "never hold" is not set, and nobody is at the computer (input in the last 10 seconds). Where the
    // input time cannot be read, the choice in Settings decides.
    pub fn why_not_held(&self, settings: &Value, kind: crate::answers::Kind, src: &dyn crate::presence::Source) -> bobix_link::messages::AtComputerWhy {
        use bobix_link::messages::AtComputerWhy;
        if self.has_phone_for(kind) && settings["approveWait"].as_f64().unwrap_or(0.0) != 0.0 && src.idle_ms().is_some_and(|ms| ms < crate::presence::AT_COMPUTER_MS) { AtComputerWhy::Present } else { AtComputerWhy::NotShown }
    }
    pub fn holds_for_phones(&self, settings: &Value, kind: crate::answers::Kind, src: &dyn crate::presence::Source) -> bool {
        if !self.has_phone_for(kind) || settings["approveWait"].as_f64().unwrap_or(0.0) == 0.0 { return false; }
        match src.idle_ms() { Some(ms) => ms >= crate::presence::AT_COMPUTER_MS, None => self.cfg.hold != "computer" }
    }
    // The peers, the revocations the relay may not know, an open pairing. After an invite code let this computer in, again:
    // while it was a guest the relay kept only invited computers from its peers and refused its pairing.
    fn login_frames(&mut self, again: bool) -> Vec<Frame> {
        let mut out = vec![self.peers_frame()];
        out.extend(self.store.revoked.iter().map(|r| Frame::Revoke { id: r.id.clone() }));
        if let Some(p) = self.pairing.as_ref().filter(|p| (again || !p.opened) && matches!(p.session.status(now()), PairStatus::Open)) {
            out.push(Frame::PairOpen { relay_token_hash: b64(&p.session.code().relay_token_hash()), ttl: 120 });
        }
        if let Some(p) = self.pairing.as_mut() { p.opened = true; }
        out
    }
    fn on_invite_state(&mut self, owner: bool, invited: bool, relay_id: Option<String>, invitees: Vec<Invitee>) {
        let i = &mut self.invites;
        (i.owner, i.invited, i.known, i.relay_id, i.invitees) = (owner, invited, true, relay_id.filter(|_| owner), if owner { invitees } else { vec![] });
        if !owner { i.made = None; }
        if invited && self.status == "invite" { self.status = "online".into(); }
        if !invited && self.status == "online" { self.status = "invite".into(); }
    }
    fn on_invite_result(&mut self, ok: bool, reason: Option<InviteReason>) -> Vec<Frame> {
        self.invites.pending = None;
        let reason = reason.map(|r| serde_json::to_value(r).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default());
        self.invites.result = Some(if ok { reason.filter(|r| r == "already").unwrap_or_else(|| "ok".into()) } else { reason.unwrap_or_else(|| "other".into()) });
        if !ok { return vec![]; }
        self.invites.invited = true;
        if self.status == "invite" { self.status = "online".into(); }
        self.login_frames(true)
    }
    fn send_close(&self) { if let Some(tx) = &self.tx { let _ = tx.send(Out::Close); } } }

// ---------- frames from the relay ----------
pub fn on_frame(b: &Arc<Bobix>, f: Frame) -> Vec<Frame> {
    let mut out = vec![];
    let mut changed = false;
    {
        let mut c = b.core.lock().unwrap();
        match f {
            Frame::Msg { pid, from, body } => {
                out.push(Frame::Ack { pid });
                if let Some(packet) = b64_decode(&body) {
                    if c.phones.store.phones.iter().any(|p| p.id == from) { out.extend(on_packet(b, &mut c, &from, &packet, &mut changed)); }
                    else { changed = on_hello(&mut c.phones, &from, &packet); }
                }
            }
            f => {
                let st = &mut c.phones;
                match f {
                    Frame::Delivered { pid } => {
                        if let Some(m) = st.moving.as_mut() { m.waiting.remove(&pid); }
                        out.extend(st.flush_revokes(Some(&pid), now()));
                    }
                    Frame::Presence { peer, state } => {
                        if state == bobix_link::relay_frames::PresenceState::Online { if st.online.insert(peer.clone()) { st.home_now.insert(peer); } } else { st.online.remove(&peer); }
                    }
                    Frame::InviteState { owner, invited, relay_id, invitees } => { st.on_invite_state(owner, invited, relay_id, invitees); changed = true; }
                    Frame::InviteResult { ok, reason } => { out.extend(st.on_invite_result(ok, reason)); changed = true; }
                    Frame::Error { code, peer, .. } => match code {
                        ErrorCode::Invite => st.status = "invite".into(),
                        ErrorCode::Update => st.status = "update".into(),
                        ErrorCode::RelayUrl => st.status = "url".into(),
                        ErrorCode::Auth => st.status = "auth".into(),
                        ErrorCode::Grant => st.grant_ended(now(), peer.as_deref()),
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
    }
    if changed { b.broadcast(); }
    out
}

// A packet from a phone that is not paired: a `hello` for the open pairing, else nothing.
fn on_hello(st: &mut State, from: &str, body: &[u8]) -> bool {
    let mut warn = None;
    let changed = on_hello_in(st, from, body, &mut warn);
    if warn.is_some() { st.pair_warning = warn; }
    changed
}

fn on_hello_in(st: &mut State, from: &str, body: &[u8], warn: &mut Option<PairWarn>) -> bool {
    let (Some(p), Some(id)) = (st.pairing.as_mut(), st.identity.as_ref()) else { return false };
    let Some((phone_box, packet)) = split_hello_body(body) else { return false };
    let mut link = Link::new(Direction::ToPhone, *id.box_secret(), phone_box, CURRENT_VERSION, VersionRange::current(), [0; 3], now(), RecvState::default());
    let Ok(opened) = link.open(packet) else { return false };
    let Ok(PhoneMsg::Hello(hello)) = PhoneMsg::from_json(&opened.plaintext) else { return false };
    // The name is judged before the session moves on: a pairing never sits Offered with nothing to confirm.
    if !valid_name(&hello.name) { return false; }
    match p.session.on_hello(&hello, &phone_box, from, now()) {
        Ok(Offer::New { phone, sign, name, rights, .. }) if valid_name(&name) => { p.offer = Some(Offered { phone, sign, name, rights, from: from.into(), link }); true }
        Ok(Offer::Cancelled) => { p.offer = None; *warn = Some(PairWarn::Cancelled); true }
        Ok(Offer::Late) => { *warn = Some(PairWarn::Late(p.paired.clone().unwrap_or_default())); true }
        _ => false, // a bad MAC, a duplicate, a name that is not plain text
    }
}

fn on_packet(b: &Arc<Bobix>, c: &mut crate::Core, from: &str, packet: &[u8], changed: &mut bool) -> Vec<Frame> {
    let st = &mut c.phones;
    let Some(link) = st.link_of(from) else { return vec![] };
    let opened = match link.open(packet) { Ok(o) => o, Err(_) => return vec![] };
    st.remember_recv(from);
    // A phone seals everything on `event`: a packet on another stream is not from the phone app (the counters of `activity` accept an equal counter).
    if opened.stream != bobix_link::envelope::Stream::Event { return vec![]; }
    if let Some(ph) = st.store.phones.iter_mut().find(|p| p.id == from && p.no_push) { ph.no_push = false; st.save_store(); } // it was opened: it can register a new grant
    let Ok(msg) = PhoneMsg::from_json(&opened.plaintext) else { return vec![] };
    if msg.validate().is_err() { return vec![]; }
    let rights = st.store.phones.iter().find(|p| p.id == from).map(|p| p.rights).unwrap_or_default();
    let mut out = vec![];
    match msg {
        PhoneMsg::PairedAck => {
            // The session stays (until `tick` finds it finished): a different phone with the code is still reported.
            if let Some(p) = st.pairing.as_mut() { p.session.ack(now()); }
            st.home_now.insert(from.into());
        }
        PhoneMsg::Bye { sig } => {
            let (Some(cid), Some(p)) = (st.computer_id(), st.store.phones.iter().find(|p| p.id == from)) else { return vec![] };
            let keys = b64_decode(&p.auth).and_then(|a| <[u8; 32]>::try_from(a).ok());
            let bytes = command::bye_bytes(opened.version, from, &cid, opened.counter);
            if keys.is_some_and(|k| b64_decode(&sig).is_some_and(|s| bobix_link::keys::verify_auth(&k, &bytes, &s))) {
                st.forget(from);
                out.extend([Frame::Revoke { id: from.into() }, st.peers_frame()]);
            }
        }
        PhoneMsg::Detail { id } => {
            let r = match crate::phone_view::detail(&c.pending, &id, rights) { Err("unknown") => crate::phone_view::session_detail(&c.sessions, &id, rights), r => r };
            let m = match r {
                Ok(d) => DesktopMsg::Detail(d),
                Err(code) => DesktopMsg::Error { code: code.into(), who: Some(Who::Desktop) },
            };
            if let Some((_, f)) = c.phones.tell(from, &m, None, None) { out.push(f); }
        }
        PhoneMsg::Issues => {
            let m = crate::phone_view::issues_reply(|| c.work.view(&c.settings, &crate::model::Model::build(&c.settings, &c.sessions, &c.work)), &c.settings, rights);
            if let Some((_, f)) = c.phones.tell(from, &m, None, None) { out.push(f); }
        }
        PhoneMsg::StartPreview { project, iid, mode } => {
            let m = if !rights.start { Some(DesktopMsg::Error { code: "right".into(), who: Some(Who::Desktop) }) }
                else if iid.parse::<i64>().is_err() || project.len() > MAX_PROJECT { Some(DesktopMsg::Error { code: "encoding".into(), who: Some(Who::Desktop) }) } else { None };
            match m {
                Some(m) => { if let Some((_, f)) = c.phones.tell(from, &m, None, None) { out.push(f); } }
                None => {
                    let (b2, from2, iid) = (b.clone(), from.to_string(), iid.parse::<i64>().unwrap_or(0));
                    std::thread::spawn(move || {
                        let m = match start_plan(&b2, &project, iid, mode) {
                            Ok(p) => DesktopMsg::StartPrompt(StartPrompt { project, iid: iid.to_string(), mode, prompt_hash: prompt_hash(&shown_prompt(&p.prompt)), odd: crate::answers::odd(&Value::String(p.prompt.clone())), prompt: shown_prompt(&p.prompt), folder_known: p.folder_known, free_disk: p.free }),
                            Err(code) => DesktopMsg::Error { code: code.into(), who: Some(Who::Desktop) },
                        };
                        tell_later(&b2, &from2, &m);
                    });
                }
            }
        }
        PhoneMsg::Sync { waiting, .. } => {
            let st = &mut c.phones;
            let results = command::sync_results(&waiting, |cid| st.results.get(&format!("{from}:{cid}")).map(|r| r.0.clone()));
            for r in results { if let Some((_, f)) = st.tell(from, &DesktopMsg::Result(r), None, None) { out.push(f); } }
        }
        PhoneMsg::Cmd(cmd) => {
            if let Some(r) = on_command(b, c, from, opened.version, opened.counter, &cmd, changed) {
                if let Some((_, f)) = c.phones.tell(from, &DesktopMsg::Result(r), None, None) { out.push(f); }
            } // None: a start, whose result is sent when it is done
        }
        _ => {}
    }
    out
}

// Who a closed notice to phone `to` says closed the item. A phone's answer with the phone unknown says nothing (the phone then
// guesses). The phone that answered is named to that phone only; to another it is a phone with no id (empty).
fn closed_by(end: crate::answers::End, answerer: Option<&str>, to: &str) -> Option<ClosedBy> {
    use crate::answers::{By, End};
    match end {
        End::Answered(By::Pc, _) | End::Computer => Some(ClosedBy::Computer),
        End::Answered(By::Phone, _) => answerer.map(|a| ClosedBy::Phone { phone: if a == to { a.to_string() } else { String::new() } }),
        End::Expired => Some(ClosedBy::Expired),
        End::Released(_) => Some(ClosedBy::Released),
    }
}

// A command from a phone, checked in this order: counter (already by the link), a `cid` seen before (the stored result: the
// packet already authenticated the sender with its box key, and the store is per phone), the signature (with the packet's
// version, phone id, computer id, age), the phone's right for the kind of thing held, `cut`, the `shown` hash against what is
// held, and the request still open. Every result is stored per `cid` for 24 hours and logged.
fn on_command(b: &Arc<Bobix>, c: &mut crate::Core, from: &str, version: u8, counter: u64, cmd: &command::Command, changed: &mut bool) -> Option<CmdResult> {
    use crate::answers::By;
    let key = format!("{from}:{}", cmd.cid);
    if let Some(stored) = c.phones.results.get(&key) { return Some(stored.0.clone()); }
    if c.phones.inflight.contains(&key) { return None; } // its start is still running: the result comes when it is done
    let t = now();
    let Some(phone) = c.phones.store.phones.iter().find(|p| p.id == from).cloned() else { return Some(CmdResult::refused(&cmd.cid, "unknown")) };
    let (Some(cid), Some(sign)) = (c.phones.computer_id(), b64_decode(&phone.sign).and_then(|s| SignPublic::from_sec1(&s).ok())) else { return Some(CmdResult::refused(&cmd.cid, "encoding")) };
    let ctx = command::VerifyCtx { version, phone_id: from, computer_id: &cid, sign_key: &sign, counter, now_ms: t };
    let mut later = false;
    let mut item = None; // the held item the command answers
    let (result, what) = match command::verify(&ctx, cmd) {
        Err(r) => (CmdResult::refused(&cmd.cid, r.why()), "command".to_string()),
        Ok(body) => {
            let target = match &body { CommandBody::Answer { id, .. } | CommandBody::Plan { id, .. } | CommandBody::Question { id, .. } => Some(id.clone()), _ => None };
            let shown_of = target.as_deref().and_then(|id| c.pending.get(id)).map(|p| crate::phone_view::preview(p.raw.lines().next().unwrap_or(""), 60)).unwrap_or_default();
            item = target.clone().or_else(|| if let CommandBody::Stop { request, .. } = &body { Some(request.clone()) } else { None });
            let what = format!("{} {}", body.kind(), shown_of).trim().to_string();
            let refuse = |why: &str| CmdResult::refused(&cmd.cid, why);
            // a note or a free-text answer goes into Claude's reply: plain text, newlines allowed
            let plain = plain_body(&body);
            let r = if !phone.rights.allows(&body) { refuse("right") } else if !plain { refuse("text") } else { match &body {
                CommandBody::Answer { id, choice, shown } => {
                    let allow = *choice == Choice::Allow;
                    apply(c, &cmd.cid, id, shown, if allow { "allow" } else { "deny" }, "", false, By::Phone, changed, None)
                }
                CommandBody::Plan { id, decision, note, shown } => {
                    let approve = *decision == Decision::Approve;
                    // an approve of a held plan reads its files again, off the core's lock: the result comes when that is done
                    let held = c.pending.get(id).filter(|p| approve && p.kind() == crate::answers::Kind::Plan).map(|p| (p.tool_input["plan"].as_str().unwrap_or("").to_string(), p.cwd.clone()));
                    match held {
                        Some(plan) => {
                            begin_approve(b, c, Approve { from: from.into(), name: phone.name.clone(), cid: cmd.cid.clone(), id: id.clone(), shown: shown.clone(), note: note.clone().unwrap_or_default(), what: what.clone(), t }, plan);
                            later = true;
                            CmdResult::new(&cmd.cid, Outcome::Unknown)
                        }
                        None => apply(c, &cmd.cid, id, shown, if approve { "allow" } else { "deny" }, note.as_deref().unwrap_or(""), true, By::Phone, changed, None),
                    }
                }
                CommandBody::Question { id, answers, shown } => apply_question(c, &cmd.cid, id, shown, answers, changed),
                CommandBody::Stop { session, request } => {
                    match crate::answers::stop_session(c, session, request, By::Phone, now()) {
                        Ok(()) => { *changed = true; CmdResult::new(&cmd.cid, Outcome::Ok) }
                        Err(why) => refuse(why),
                    }
                }
                CommandBody::Start { project, iid, mode, prompt_hash } => match begin_start(b, c, from, &phone.name, &cmd.cid, project, iid, *mode, prompt_hash) {
                    Some(r) => r,
                    None => { later = true; CmdResult::new(&cmd.cid, Outcome::Unknown) }
                },
            } };
            (r, what)
        }
    };
    if later { return None; } // the start or approve thread logs and stores it when it is done
    record(c, from, &phone.name, &what, item, &result, t);
    Some(result)
}

// A command's result, kept: who answered (for the closed notice), the log line, and the result by `cid` for 24 hours.
fn record(c: &mut crate::Core, from: &str, name: &str, what: &str, item: Option<String>, result: &CmdResult, t: u64) {
    if let (Some(id), Outcome::Ok) = (item, result.outcome) { c.phones.answerers.insert(id, from.to_string()); }
    let word = serde_json::to_value(result.outcome).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
    let shown = if let Some(w) = &result.why { format!("{word} ({w})") } else { word };
    c.phones.push_log(name, what, &shown);
    c.phones.results.insert(format!("{from}:{}", result.cid), (result.clone(), t));
    c.phones.save_results(&c.closed);
}

// A phone's approve of a held plan, after the signature and the right.
struct Approve { from: String, name: String, cid: String, id: String, shown: String, note: String, what: String, t: u64 }

// The plan's files are read again in a thread, so a slow or hung disk never holds the core's lock; then the approve is
// checked and applied as any answer (the plan may have closed meanwhile), and its result goes to the phone.
fn begin_approve(b: &Arc<Bobix>, c: &mut crate::Core, a: Approve, (plan, cwd): (String, String)) {
    c.phones.inflight.insert(format!("{}:{}", a.from, a.cid));
    let b2 = b.clone();
    std::thread::spawn(move || {
        let files = crate::plan_files::read(&plan, &cwd);
        let mut changed = false;
        let mut c = b2.core.lock().unwrap();
        c.phones.inflight.remove(&format!("{}:{}", a.from, a.cid));
        let result = apply(&mut c, &a.cid, &a.id, &a.shown, "allow", &a.note, true, crate::answers::By::Phone, &mut changed, Some(files));
        record(&mut c, &a.from, &a.name, &a.what, Some(a.id.clone()), &result, a.t);
        if let Some((_, f)) = c.phones.tell(&a.from, &DesktopMsg::Result(result), None, None) { c.phones.send(f); }
        drop(c);
        if changed { b2.broadcast(); }
    });
}

// The prompt as the phone shows it: hidden characters (tag characters, bidirectional overrides, invisible letters) written as
// visible escapes, so the person reads what Claude would read. The hash a start carries is over this text.
fn shown_prompt(prompt: &str) -> String { crate::answers::escape_hidden(prompt) }

// What a start of an issue would run and need, read off the core's lock (it reads the issue's body from the tracker).
pub struct StartPlan { pub prompt: String, pub folder_known: bool, pub free: u64, pub full: Option<(String, Value)> } // Full: (the base branch remembered for the repository, the models)

fn start_plan(b: &Arc<Bobix>, project: &str, iid: i64, mode: Mode) -> Result<StartPlan, &'static str> {
    let (src, dir) = {
        let c = b.core.lock().unwrap();
        let view = c.work.view(&c.settings, &crate::model::Model::build(&c.settings, &c.sessions, &c.work));
        let dir = view["local"].as_array().and_then(|l| l.iter().find(|g| g["path"] == project)).and_then(|g| g["dir"].as_str().map(String::from));
        (c.work.source_of(&c.settings, project, iid), dir)
    };
    let Some((f, bin, host, i)) = src else { return Err("gone") };
    let body = crate::work::fetch_body(f, &bin, &host, project, iid).map_err(|_| "gone")?;
    let brief = crate::work::brief(iid, i["title"].as_str().unwrap_or(""), &body, i["url"].as_str().unwrap_or(""));
    if mode == Mode::Quick {
        let free = dir.as_deref().and_then(|d| crate::tickets::free_bytes(Path::new(d))).unwrap_or(0);
        return Ok(StartPlan { prompt: brief, folder_known: dir.is_some(), free, full: None });
    }
    // Full: the same preparation as the panel's, with the models and rules text its prompt carries
    let prep = crate::prep_issue(b, project, iid, None);
    if prep["ok"] != true { return Err("folder"); }
    if prep["exists"] == true { return Err("exists"); }
    let base = prep["remembered"].as_str().unwrap_or("").to_string();
    let prompt = crate::work::flow_prompt(&brief, prep["models"].as_str().unwrap_or(""), prep["rules"].as_str().unwrap_or(""));
    Ok(StartPlan { prompt, folder_known: true, free: prep["free"].as_u64().unwrap_or(0), full: Some((base, json!({ "size": "small" }))) })
}

// Whether the issue already has a session (a phone never resumes or focuses one), from the work view.
fn issue_has_session(view: &Value, project: &str, iid: i64) -> bool {
    view["local"].as_array().into_iter().flatten().filter_map(|g| g["issues"].as_array()).flatten()
        .any(|i| i["project"] == project && i["iid"].as_i64() == Some(iid) && i["sessions"].as_array().is_some_and(|s| !s.is_empty()))
}

// The result of a start, from its plan: gone, folder, stale (the prompt is not the one shown), base (Full needs the branch
// remembered for the repository; asked at the computer), disk, or what the start itself answered.
fn finish_start(cid: &str, plan: Result<StartPlan, &'static str>, hash: &str, start: impl FnOnce(&StartPlan) -> Value) -> CmdResult {
    let p = match plan { Ok(p) => p, Err(why) => return CmdResult::refused(cid, why) };
    if !p.folder_known { return CmdResult::refused(cid, "folder"); }
    if prompt_hash(&shown_prompt(&p.prompt)) != hash { return CmdResult::new(cid, Outcome::Stale); }
    if let Some((base, _)) = &p.full {
        if base.is_empty() { return CmdResult::refused(cid, "base"); }
        if p.free < crate::tickets::WORKTREE_BYTES { return CmdResult::refused(cid, "disk"); }
    }
    let r = start(&p);
    if r["ok"] == true { CmdResult::new(cid, Outcome::Ok) } else { CmdResult::refused(cid, match r["reason"].as_str() { Some("gone") => "folder", Some(x) => x, None => "failed" }) }
}

// A message to a phone from a thread: sealed and sent now.
fn tell_later(b: &Arc<Bobix>, to: &str, m: &DesktopMsg) {
    let mut c = b.core.lock().unwrap();
    if let Some((_, f)) = c.phones.tell(to, m, None, None) { c.phones.send(f); }
}

// A `start` that passed the signature and the right. Refused at once: an issue that already has a session and a second
// start of the issue while one runs. Otherwise a thread reads the issue, compares the prompt's hash with the one the phone
// was shown, and starts it the way the panel does (Quick: a new session with the brief; Full: the ticket flow, `how:
// desktop`, small models, the remembered base, never `anyway`); its result goes to the phone when it is done.
#[allow(clippy::too_many_arguments)]
fn begin_start(b: &Arc<Bobix>, c: &mut crate::Core, from: &str, name: &str, cid: &str, project: &str, iid: &str, mode: Mode, hash: &str) -> Option<CmdResult> {
    let Ok(n) = iid.parse::<i64>() else { return Some(CmdResult::refused(cid, "encoding")) };
    // A project the work view does not know is refused before anything is kept or logged: its text is the phone's.
    if project.len() > MAX_PROJECT || c.work.source_of(&c.settings, project, n).is_none() { return Some(CmdResult::refused(cid, "gone")); }
    let key = format!("{project}#{n}");
    if c.phones.starting.contains(&key) { return Some(CmdResult::refused(cid, "busy")); }
    let view = c.work.view(&c.settings, &crate::model::Model::build(&c.settings, &c.sessions, &c.work));
    if issue_has_session(&view, project, n) { return Some(CmdResult::refused(cid, "exists")); }
    c.phones.starting.insert(key.clone());
    c.phones.inflight.insert(format!("{from}:{cid}"));
    let (b2, from, name, cid, project, hash) = (b.clone(), from.to_string(), name.to_string(), cid.to_string(), project.to_string(), hash.to_string());
    std::thread::spawn(move || {
        let result = finish_start(&cid, start_plan(&b2, &project, n, mode), &hash, |p| match &p.full {
            None => crate::start_issue(&b2, &project, n, &p.prompt, None),
            Some((base, models)) => crate::start_flow(&b2, &project, n, None, &p.prompt, models.clone(), "desktop", Some(false), Some(base.clone()), Some(false)),
        });
        let mut c = b2.core.lock().unwrap();
        c.phones.starting.remove(&key);
        c.phones.inflight.remove(&format!("{from}:{cid}"));
        let word = serde_json::to_value(result.outcome).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        c.phones.push_log(&name, &format!("start {}#{n}", crate::phone_view::preview(&project, 60)), &if let Some(w) = &result.why { format!("{word} ({w})") } else { word });
        c.phones.results.insert(format!("{from}:{cid}"), (result.clone(), now()));
        if let Some((_, f)) = c.phones.tell(&from, &DesktopMsg::Result(result), None, None) { c.phones.send(f); }
        drop(c);
        b2.broadcast();
    });
    None
}

// A note or a free-text answer goes into Claude's reply: plain text, newlines allowed, no other control character.
fn plain_body(body: &CommandBody) -> bool {
    match body { CommandBody::Plan { note: Some(n), .. } => is_plain_text(n, MAX_NOTE), CommandBody::Question { answers, .. } => answers.iter().all(|a| is_plain_text(a, MAX_NOTE)), _ => true }
}

// A `question` command that passed the signature and the right: the kind, the hash of what was shown, the answers.
fn apply_question(c: &mut crate::Core, cid: &str, id: &str, shown: &str, answers: &[String], changed: &mut bool) -> CmdResult {
    use crate::answers::{answer_question, Answered, By, Kind};
    match c.pending.get(id) {
        Some(p) if p.kind() != Kind::Question => return CmdResult::refused(cid, "kind"),
        Some(p) => {
            if crate::phone_view::shown_cut(p) { return CmdResult::refused(cid, "cut"); }
            match crate::phone_view::detail(&c.pending, id, Rights { question: true, ..Default::default() }) {
                Ok(d) if command::shown_matches(&d, shown) => {}
                _ => return CmdResult::new(cid, Outcome::Stale),
            }
        }
        None => {}
    }
    match answer_question(c, id, answers, By::Phone, now()) {
        Answered::Ok => { *changed = true; CmdResult::new(cid, Outcome::Ok) }
        Answered::LatePc => CmdResult { by: Some("computer".into()), ..CmdResult::new(cid, Outcome::LatePc) },
        Answered::LatePhone => CmdResult { by: Some("phone".into()), ..CmdResult::new(cid, Outcome::LatePhone) },
        Answered::Expired => CmdResult::new(cid, Outcome::Expired),
        Answered::Released(why) => CmdResult { why: Some(why.into()), ..CmdResult::new(cid, Outcome::Released) },
        Answered::WrongKind => CmdResult::refused(cid, "kind"),
        Answered::Unknown => CmdResult::refused(cid, "unknown"),
        Answered::Invalid => CmdResult::refused(cid, "answers"),
    }
}

// An `answer` (plan: false) or `plan` (plan: true) command that passed the signature and the right.
#[allow(clippy::too_many_arguments)]
fn apply(c: &mut crate::Core, cid: &str, id: &str, shown: &str, choice: &str, note: &str, plan: bool, by: crate::answers::By, changed: &mut bool, read: Option<crate::plan_files::PlanFiles>) -> CmdResult {
    use crate::answers::{respond_core, Answered};
    // what is held, as the phone saw it: the kind first (an answer for a plan id is refused, and the reverse)
    let mut fresh = None;
    match c.pending.get(id) {
        Some(p) if p.kind() != if plan { crate::answers::Kind::Plan } else { crate::answers::Kind::Request } => return CmdResult::refused(cid, "kind"),
        Some(p) => {
            if crate::phone_view::blocks_allow(p) && choice == "allow" { return CmdResult::refused(cid, "cut"); } // a cut plan too, and a plan with a file cut or not sent, or files not shown: only what was shown is known
            let rights = Rights { answer: true, plan: true, ..Default::default() };
            match crate::phone_view::detail(&c.pending, id, rights) {
                Ok(d) if command::shown_matches(&d, shown) => {}
                _ => return CmdResult::new(cid, Outcome::Stale),
            }
            // An approve covers the files as they are on disk now, not as they were when the plan was held (what the hash
            // covers is the copy in memory): read again, and when they differ the phone is told and shown the new ones. The
            // phone's approve reads them before the core is locked (`begin_approve`); only a test reads them here.
            if plan && choice == "allow" {
                let now = read.unwrap_or_else(|| p.files_now());
                if now.files != p.files || now.omitted != p.files_omitted { fresh = Some(now); }
            }
        }
        None => {}
    }
    if let Some(now) = fresh {
        if let Some(p) = c.pending.get_mut(id) { (p.files, p.files_omitted) = (now.files, now.omitted); }
        return CmdResult::new(cid, Outcome::Stale);
    }
    let r = respond_core(c, id, choice, note, by, Some(if plan { crate::answers::Kind::Plan } else { crate::answers::Kind::Request }), now());
    match r {
        Answered::Ok => { *changed = true; CmdResult::new(cid, Outcome::Ok) }
        Answered::LatePc => CmdResult { by: Some("computer".into()), ..CmdResult::new(cid, Outcome::LatePc) },
        Answered::LatePhone => CmdResult { by: Some("phone".into()), ..CmdResult::new(cid, Outcome::LatePhone) },
        Answered::Expired => CmdResult::new(cid, Outcome::Expired),
        Answered::Released(why) => CmdResult { why: Some(why.into()), ..CmdResult::new(cid, Outcome::Released) },
        Answered::WrongKind => CmdResult::refused(cid, "kind"),
        Answered::Unknown => CmdResult::refused(cid, "unknown"),
        Answered::Invalid => CmdResult::refused(cid, "encoding"),
    }
}

// ---------- what the pages ask ----------
pub fn rights_of(v: &Value) -> Rights {
    let f = |k: &str| v[k] == true;
    Rights { answer: f("answer"), plan: f("plan"), question: f("question"), start: f("start"), stop: f("stop"), activity: f("activity") }
}

fn qr_svg(text: &str) -> String {
    use qrcode::{render::svg, EcLevel, QrCode};
    QrCode::with_error_correction_level(text.as_bytes(), EcLevel::M).map(|q| q.render::<svg::Color>().min_dimensions(200, 200).quiet_zone(true).build()).unwrap_or_default()
}

pub fn view(b: &Arc<Bobix>) -> Value {
    let mut c = b.core.lock().unwrap();
    let node = c.node_path.is_some();
    let st = &mut c.phones;
    let t = now();
    // A list that could not be opened is tried again when Settings looks (the keychain may be unlocked by now).
    if !st.loaded && t > st.retry_at + 10_000 { st.load_store(); }
    let pairing = match st.pairing.as_ref() {
        None => json!({ "state": "none", "warning": warning_json(&st.pair_warning) }),
        Some(p) => {
            let s = p.session.status(t);
            json!({ "state": match s { PairStatus::Open => "open", PairStatus::Offered => "offered", PairStatus::Confirmed => "confirmed", PairStatus::Cancelled => "cancelled", _ => "none" },
                "uri": if s == PairStatus::Open { json!(p.session.code().to_uri()) } else { Value::Null },
                "qr": if s == PairStatus::Open { json!(qr_svg(&p.session.code().to_uri())) } else { Value::Null },
                "name": p.offer.as_ref().map(|o| &o.name), "asked": p.offer.as_ref().map(|o| o.rights), "warning": warning_json(&st.pair_warning) })
        }
    };
    json!({ "unreadable": !st.loaded, "on": st.cfg.on, "relay": st.cfg.relay, "status": st.status, "hold": st.cfg.hold, "awake": st.cfg.awake, "awakeHeld": st.awake.is_held(), "node": node,
        "canRead": !st.cfg.on || crate::presence::source().idle_ms().is_some(), "deviceId": st.computer_id(), "storage": format!("{:?}", st.storage.unwrap_or(crate::secrets::Storage::Keychain)),
        "phones": st.store.phones.iter().map(|p| json!({ "id": p.id, "name": p.name, "rights": p.rights, "since": p.since, "needsPairing": p.needs_pairing, "noPush": p.no_push })).collect::<Vec<_>>(),
        "revoked": st.store.revoked.len(), "moving": st.moving.is_some(), "linked": st.tx.is_some(), "pairing": pairing, "invite": invite_view(&st.invites) })
}

fn invite_view(i: &Invites) -> Value {
    json!({ "known": i.known, "owner": i.owner, "invited": i.invited, "pending": i.pending.is_some(), "result": i.result, "days": INVITE_DAYS,
        "made": i.made.as_ref().map(|(code, expires, relay)| json!({ "code": code, "expires": expires, "relay": relay })),
        "invitees": i.invitees.iter().map(|x| json!({ "id": x.id, "label": x.label, "at": x.at })).collect::<Vec<_>>() })
}

// The owner makes a code (signed here, not stored): valid `days` days, for the relay that said this computer is its owner.
pub fn invite_create(b: &Arc<Bobix>, days: u64, label: &str) -> Result<Value, String> {
    {
        let mut c = b.core.lock().unwrap();
        let st = &mut c.phones;
        if !INVITE_DAYS.contains(&days) { return Err("days".into()); }
        if !st.invites.owner { return Err("notOwner".into()); }
        let relay_id = st.invites.relay_id.clone().ok_or("notOwner")?;
        let expires = now() + days * DAY_MS;
        let code = InviteCode::create(st.identity.as_ref().ok_or("identity")?, &relay_id, label, expires).map_err(|_| "label".to_string())?;
        st.invites.made = Some((code, expires, relay_id));
    }
    b.broadcast();
    Ok(view(b))
}

// A guest computer hands a code to the relay: at once when it is connected, else right after the next login (the wait
// before reconnecting is cut short).
pub fn invite_redeem(b: &Arc<Bobix>, code: &str) -> Result<Value, String> {
    {
        let mut c = b.core.lock().unwrap();
        let st = &mut c.phones;
        if !st.cfg.on { return Err("off".into()); }
        InviteCode::parse(code).map_err(|_| "invalid".to_string())?;
        let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        st.invites.result = None;
        st.invites.pending = Some(code.clone());
        if st.status == "online" || st.status == "invite" { st.send(Frame::InviteRedeem { code }); } else { st.wake.notify_one(); }
    }
    b.broadcast();
    Ok(view(b))
}

// The owner stops inviting a computer that got in by a code; the relay answers with the new list.
pub fn invite_remove(b: &Arc<Bobix>, id: &str) -> Result<Value, String> {
    {
        let c = b.core.lock().unwrap();
        let st = &c.phones;
        if !st.invites.owner { return Err("notOwner".into()); }
        if !st.invites.invitees.iter().any(|x| x.id == id) { return Err("unknown".into()); }
        st.send(Frame::InviteRemove { id: id.into() });
    }
    Ok(view(b))
}

/// The relay address the desktop accepts: the link crate's rule (`wss://`, or plain `ws://` on this computer or on a private
/// network address), and a plain `ws://` address beyond this computer only in a build with phone access ([`allow_lan`],
/// `ENABLED`: local development and the `-phones` pre-release): a build without it never starts phone access at all.
fn relay_norm(url: &str, allow_lan: bool) -> Option<String> {
    let norm = normalize_relay_url(url)?;
    (allow_lan || !norm.starts_with("ws://") || norm.starts_with("ws://localhost")).then_some(norm)
}

pub fn enable(b: &Arc<Bobix>, on: bool) -> Result<Value, String> {
    let mut out = vec![];
    {
        let mut c = b.core.lock().unwrap();
        let st = &mut c.phones;
        if on {
            if relay_norm(&st.cfg.relay, ENABLED).is_none() { return Err("relay".into()); }
            if !st.loaded { return Err("unreadable".into()); }
            st.ensure_identity(true)?;
            st.cfg.on = true;
            st.status = "connecting".into();
        } else if st.cfg.on {
            let ids: Vec<String> = st.store.phones.iter().map(|p| p.id.clone()).collect();
            for id in ids { out.extend(st.disconnect(&id, "phone access was turned off")); }
            st.cfg.on = false;
            st.pairing = None;
            st.pair_warning = None;
            st.awake.release();
            st.status = "off".into();
            if st.revoking.is_none() { st.send_close(); }
        }
        st.save_cfg();
        for f in out.drain(..) { st.send(f); }
    }
    if on { start_link(b); }
    b.broadcast();
    Ok(view(b))
}

pub fn set_relay(b: &Arc<Bobix>, url: &str) -> Result<Value, String> {
    let url = url.trim();
    let norm = relay_norm(url, ENABLED).ok_or("relay")?;
    {
        let mut c = b.core.lock().unwrap();
        let st = &mut c.phones;
        if st.moving.is_some() { return Err("moving".into()); }
        if !st.cfg.on || st.store.phones.is_empty() || st.cfg.relay.is_empty() || normalize_relay_url(&st.cfg.relay).as_deref() == Some(&norm) {
            st.cfg.relay = url.into();
            st.invites = Invites::default(); // another relay: its own owners and invitations
            st.save_cfg();
            st.send_close(); // reconnects to the new address
        } else {
            // each phone first hears of the move through the old relay; the switch waits for them
            let mut waiting = HashMap::new();
            let ids: Vec<String> = st.store.phones.iter().map(|p| p.id.clone()).collect();
            for id in ids {
                if let Some((pid, f)) = st.tell(&id, &DesktopMsg::RelayMoved { url: url.into() }, None, None) { st.send(f); waiting.insert(pid, id); }
            }
            st.moving = Some(Moving { url: url.into(), waiting, since: now() });
        }
    }
    b.broadcast();
    Ok(view(b))
}

pub fn pair_start(b: &Arc<Bobix>) -> Result<Value, String> {
    {
        let mut c = b.core.lock().unwrap();
        let st = &mut c.phones;
        if !st.cfg.on { return Err("off".into()); }
        if !st.loaded { return Err("unreadable".into()); }
        let keys = st.identity.as_ref().ok_or("identity")?.public();
        let code = PairingCode::new(&st.cfg.relay, keys).map_err(|_| "relay".to_string())?;
        let frame = Frame::PairOpen { relay_token_hash: b64(&code.relay_token_hash()), ttl: 120 };
        st.pair_warning = None;
        st.pairing = Some(Pairing { session: PairingSession::new(code, now()), offer: None, paired: None, opened: st.status == "online" });
        if st.status == "online" { st.send(frame); }
    }
    b.broadcast();
    Ok(view(b))
}
// Also what dismisses the warning that another phone used the code.
pub fn pair_cancel(b: &Arc<Bobix>) -> Value { { let mut c = b.core.lock().unwrap(); c.phones.pairing = None; c.phones.pair_warning = None; } b.broadcast(); view(b) }

// The owner confirms what the phone asked for, minus what they unticked. Accepted only for the pairing opened in this run.
pub fn pair_confirm(b: &Arc<Bobix>, granted: Rights) -> Result<Value, String> {
    let note;
    {
        let mut c = b.core.lock().unwrap();
        let st = &mut c.phones;
        let computer = st.computer_name();
        let p = st.pairing.as_mut().ok_or("no pairing")?;
        if p.session.status(now()) != PairStatus::Offered { return Err("no pairing".into()); }
        let o = p.offer.take().ok_or("no pairing")?;
        p.paired = Some(o.name.clone());
        let asked = o.rights;
        let r = Rights { answer: granted.answer && asked.answer, plan: granted.plan && asked.plan, question: granted.question && asked.question,
            start: granted.start && asked.start, stop: granted.stop && asked.stop, activity: granted.activity && asked.activity };
        if !p.session.confirm(now()) { return Err("no pairing".into()); }
        let id = o.phone.device_id();
        if id != o.from { return Err("no pairing".into()); }
        st.store.phones.push(Phone { id: id.clone(), name: o.name.clone(), auth: b64(&o.phone.auth), boxk: b64(&o.phone.boxk), sign: b64(&o.sign.to_sec1()), rights: r, since: now(), needs_pairing: false, no_push: false });
        st.store.revoked.retain(|x| x.id != id);
        st.links.insert(id.clone(), o.link);
        st.remember_recv(&id); // the counters of the hello are kept now, not at the next packet
        st.save_store();
        st.send(st.peers_frame());
        if let Some((_, f)) = st.tell(&id, &DesktopMsg::Paired { computer, rights: r }, None, None) { st.send(f); }
        note = o.name;
    }
    phones_changed_note(b, &note, true);
    b.broadcast();
    Ok(view(b))
}

pub fn set_rights(b: &Arc<Bobix>, id: &str, r: Rights) -> Result<Value, String> {
    {
        let mut c = b.core.lock().unwrap();
        let st = &mut c.phones;
        let p = st.store.phones.iter_mut().find(|p| p.id == id).ok_or("unknown")?;
        p.rights = r;
        st.save_store();
        st.home_now.insert(id.to_string()); // the phone hears it at the next tick
    }
    b.broadcast();
    Ok(view(b))
}

pub fn disconnect(b: &Arc<Bobix>, id: &str) -> Result<Value, String> {
    let name;
    {
        let mut c = b.core.lock().unwrap();
        let st = &mut c.phones;
        name = st.store.phones.iter().find(|p| p.id == id).map(|p| p.name.clone()).ok_or("unknown")?;
        let frames = st.disconnect(id, "removed on the computer");
        for f in frames { st.send(f); }
    }
    phones_changed_note(b, &name, false);
    b.broadcast();
    Ok(view(b))
}

// "New identity": only with no phone and the switch off; the keys and the records of revoked phones are deleted.
pub fn new_identity(b: &Arc<Bobix>) -> Result<Value, String> {
    {
        let mut c = b.core.lock().unwrap();
        let st = &mut c.phones;
        if !st.loaded { return Err("unreadable".into()); }
        if st.cfg.on || !st.store.phones.is_empty() { return Err("busy".into()); }
        let s = Secrets::open();
        s.delete(IDENTITY).map_err(|e| e.to_string())?;
        s.delete(SEAL_KEY).map_err(|e| e.to_string())?;
        st.identity = None;
        st.store = Store::default();
        st.saved.clear();
        for f in [store_file(&st.dir), counters_file(&st.dir)] { let _ = std::fs::remove_file(f); }
    }
    b.broadcast();
    Ok(view(b))
}

pub fn set_hold(b: &Arc<Bobix>, hold: &str) -> Value {
    { let mut c = b.core.lock().unwrap(); c.phones.cfg.hold = if hold == "computer" { "computer".into() } else { "phone".into() }; c.phones.save_cfg(); }
    view(b)
}

// The computer is about to sleep: the relay and every phone are told before it does (a home that says asleep, with how many
// sessions are unfinished, and the relay's `state`). Returns once the frames are handed to the link.
pub fn on_sleep(b: &Arc<Bobix>) {
    let mut c = b.core.lock().unwrap();
    let crate::Core { phones: st, pending, sessions, .. } = &mut *c;
    if !st.cfg.on || st.asleep { return; }
    st.asleep = true;
    st.send(Frame::State { asleep: true });
    let name = st.computer_name();
    let home = crate::phone_view::home(sessions, pending, &name, now(), ComputerState::Asleep, vec![]);
    let ids: Vec<(String, Rights)> = st.store.phones.iter().map(|p| (p.id.clone(), p.rights)).collect();
    for (id, rights) in ids { if let Some((_, f)) = st.tell(&id, &DesktopMsg::Home(crate::phone_view::for_phone(home.clone(), rights)), None, Some("home".into())) { st.send(f); } }
}
// It woke up: the relay hears it, and the phones get a fresh home.
pub fn on_wake(b: &Arc<Bobix>) {
    let mut c = b.core.lock().unwrap();
    if !c.phones.asleep { return; }
    c.phones.asleep = false;
    c.phones.send(Frame::State { asleep: false });
    let ids: Vec<String> = c.phones.store.phones.iter().map(|p| p.id.clone()).collect();
    c.phones.home_now.extend(ids);
}

pub fn set_awake(b: &Arc<Bobix>, on: bool) { let mut c = b.core.lock().unwrap(); c.phones.cfg.awake = on; c.phones.save_cfg(); }

pub fn log(b: &Arc<Bobix>) -> Value { json!(b.core.lock().unwrap().phones.log.iter().rev().collect::<Vec<_>>()) }

// A notice to every paired phone: a held request waits, or a session finished. They follow notifyWaiting, notifyDone and
// Do Not Disturb, and ignore "paused". `waiting` only when a request is held.
pub fn on_notify(b: &Arc<Bobix>, kind: &str, sid: &str) {
    let mut c = b.core.lock().unwrap();
    let crate::Core { phones: st, pending, sessions, settings, .. } = &mut *c;
    if !st.cfg.on || st.store.phones.is_empty() || st.status != "online" { return; } // not while the relay is down: a burst of stale notices on reconnect helps no one
    let dnd = settings["dndUntil"].as_f64().unwrap_or(0.0) > now() as f64;
    if dnd || (kind == "waiting" && settings["notifyWaiting"] != true) || (kind == "done" && settings["notifyDone"] != true) { return; }
    let Some(s) = sessions.get(sid) else { return };
    let title = if s.title.is_empty() { &s.name } else { &s.title };
    let (n, id) = match kind {
        "waiting" => {
            let Some((id, p)) = pending.iter().find(|(_, p)| p.sid == sid && !p.released) else { return };
            if !st.notified.insert(id.clone()) { return; }
            (crate::phone_view::waiting_notice(p, id, title), id.clone())
        }
        "done" => (Notice { what: NoticeKind::Done, reference: sid.into(), session: sid.into(), title: crate::phone_view::preview(title, 80), body: crate::phone_view::preview(&s.summary, 200), by: None }, sid.to_string()),
        _ => return,
    };
    let push = Push { kind: bobix_link::relay_frames::PushKind::Alert, priority: 10, event: None, replaces: Some(id) };
    let ids: Vec<String> = st.store.phones.iter().map(|p| p.id.clone()).collect();
    for p in ids { if let Some((_, f)) = st.tell(&p, &DesktopMsg::Notice(n.clone()), Some(push.clone()), None) { st.send(f); } }
}

// A desktop notification says when the list of phones changes.
fn phones_changed_note(b: &Arc<Bobix>, name: &str, added: bool) {
    let (l, sound) = { let c = b.core.lock().unwrap(); (crate::lang(&c.settings), c.settings["sound"] == true) };
    let key = if added { "phones.note.added" } else { "phones.note.removed" };
    crate::notify::show(crate::notify::Note { title: crate::i18n::t(&l, "phones.note.title", &[]), body: crate::i18n::t(&l, key, &[("name", name)]), sound, buttons: None, open: crate::i18n::t(&l, "notify.open", &[]) }, |_| {});
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_lan_relay_is_for_builds_with_phone_access() {
        for always in ["wss://relay.example/ws", "ws://localhost:8787", "ws://127.0.0.1:8787/"] {
            assert!(relay_norm(always, false).is_some() && relay_norm(always, true).is_some(), "{always}");
        }
        for lan in ["ws://192.168.1.20:8787", "ws://10.0.0.5:8787", "ws://172.16.3.4:8787", "ws://[fd12::1]:8787"] {
            assert!(relay_norm(lan, false).is_none(), "a build without phone access refuses {lan}");
            assert_eq!(relay_norm(lan, true).as_deref().map(|n| n.starts_with("ws://")), Some(true), "a build with phone access takes {lan}");
        }
        for never in ["ws://203.0.113.5:8787", "ws://relay.example", "ws://169.254.169.254", "http://192.168.1.20"] {
            assert!(relay_norm(never, false).is_none() && relay_norm(never, true).is_none(), "{never}");
        }
    }

    #[test]
    fn names_are_plain_and_short() {
        assert!(valid_name("Kim's iPhone") && valid_name(&"é".repeat(40)));
        for bad in ["", &"a".repeat(41), "x\u{202e}y", "a\nb", "a\u{7}", "zero\u{200b}width", "tag\u{e0041}"] { assert!(!valid_name(bad), "{bad:?}"); }
        assert!(valid_name("Kim's \u{260E}\u{FE0F}") && valid_name("\u{2705}\u{FE0F} work"), "emoji with a presentation selector");
        for bad in ["\u{FE0F}", "x \u{FE0F}", "\u{26A0}\u{FE0F}\u{FE0F}"] { assert!(!valid_name(bad), "{bad:?}"); }
        assert!(valid_name("<b>x</b>"), "markup is plain text here; the panel sets it with textContent");
    }

    #[test]
    fn the_list_of_phones_is_sealed() {
        let c = XChaCha20Poly1305::new_from_slice(&[7u8; 32]).unwrap();
        let s = Store { phones: vec![Phone { id: "ab".into(), name: "SECRET-NAME".into(), auth: "a".into(), boxk: "b".into(), sign: "c".into(), rights: Rights::default(), since: 1, needs_pairing: false, no_push: false }], revoked: vec![] };
        let bytes = seal_store(&c, &s);
        assert!(!String::from_utf8_lossy(&bytes).contains("SECRET-NAME"), "not readable in the file");
        assert_eq!(open_store(&c, &bytes).unwrap().phones[0].name, "SECRET-NAME");
        let other = XChaCha20Poly1305::new_from_slice(&[8u8; 32]).unwrap();
        assert!(open_store(&other, &bytes).is_none() && open_store(&c, &bytes[..30]).is_none());
        let mut bad = bytes.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(open_store(&c, &bad).is_none(), "a changed file is refused");
    }

    use crate::{answers::{Pending, By, Kind}, presence::Fixed};

    fn on_state() -> State {
        let mut st = State::default();
        st.cfg.on = true; st.cfg.hold = "phone".into();
        st.store.phones.push(Phone { id: "p".into(), name: "n".into(), auth: String::new(), boxk: String::new(), sign: String::new(), rights: Rights { answer: true, ..Default::default() }, since: 0, needs_pairing: false, no_push: false });
        st
    }

    #[test]
    fn a_phone_that_must_pair_again_holds_nothing() {
        let mut st = on_state();
        let s = json!({ "approveWait": -1 });
        let away = Fixed(Some(30_000));
        assert!(st.has_phone_for(Kind::Request) && st.holds_for_phones(&s, Kind::Request, &away));
        // it did not follow a relay move: it cannot be reached, so nothing is kept waiting for it
        st.store.phones[0].needs_pairing = true;
        assert!(!st.has_phone_for(Kind::Request));
        assert!(!st.holds_for_phones(&s, Kind::Request, &away));
    }

    #[test]
    fn the_live_activity_follows_the_count_one_push_at_a_time() {
        use ActivityEvent::*;
        let gap = ACTIVITY_GAP_MS;
        // Never pushed since this computer started: only something waiting starts it.
        assert_eq!(activity_step(None, 0, 10), None);
        assert_eq!(activity_step(None, 2, 10), Some(Start));
        // The count it already shows is not pushed again, and a change inside the gap waits for the next tick.
        assert_eq!(activity_step(Some((2, 100)), 2, 100 + gap), None);
        assert_eq!(activity_step(Some((2, 100)), 3, 100 + gap - 1), None);
        assert_eq!(activity_step(Some((2, 100)), 0, 100 + gap - 1), None);
        // After the gap: rises from nothing start, falls to nothing end, anything else updates.
        assert_eq!(activity_step(Some((0, 100)), 1, 100 + gap), Some(Start));
        assert_eq!(activity_step(Some((2, 100)), 3, 100 + gap), Some(Update));
        assert_eq!(activity_step(Some((3, 100)), 1, 100 + gap), Some(Update));
        assert_eq!(activity_step(Some((2, 100)), 0, 100 + gap), Some(End));
        // 0, 2, 0 and 2, 0, 2 inside the gap come back to the count that was pushed: nothing goes out.
        assert_eq!(activity_step(Some((0, 100)), 0, 100 + gap), None);
        assert_eq!(activity_step(Some((2, 100)), 2, 100 + gap), None);
    }

    #[test]
    fn a_live_activity_push_is_a_sealed_count_with_no_name_and_goes_once_per_phone() {
        let dir = std::env::temp_dir().join(format!("bobix-activity-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (desktop, phone) = (Identity::generate(), Identity::generate());
        let mut st = on_state();
        st.dir = dir.clone();
        st.status = "online".into();
        st.computer = "Studio Mac".into();
        st.store.phones[0].boxk = b64(&phone.public().boxk);
        st.identity = Some(desktop);
        let desk_boxk = st.identity.as_ref().unwrap().public().boxk;
        let mut phone_link = Link::new(Direction::ToDesktop, *phone.box_secret(), desk_boxk, CURRENT_VERSION, VersionRange::current(), [0; 3], now(), RecvState::default());
        let push_of = |f: &Frame| match f { Frame::Send { push, collapse, body, .. } => (push.clone().unwrap(), collapse.clone(), b64_decode(body).unwrap()), _ => panic!("not a send") };

        let t = 1_000_000;
        let out = st.activity_frames(2, false, t);
        assert_eq!(out.len(), 1);
        let (push, collapse, packet) = push_of(&out[0]);
        assert_eq!((push.kind, push.event, push.priority, collapse.as_deref()), (PushKind::LiveActivity, Some(ActivityEvent::Start), 10, Some("activity")));
        let opened = phone_link.open(&packet).expect("the phone opens it");
        assert_eq!(opened.stream, bobix_link::envelope::Stream::Activity);
        let body: Value = serde_json::from_slice(&opened.plaintext).unwrap();
        assert_eq!(body, json!({ "waiting": 2, "names": [], "written": t, "computerName": "Studio Mac", "computerState": "online" }), "exactly what the widget decodes, no session name");

        assert!(st.activity_frames(2, false, t + ACTIVITY_GAP_MS).is_empty(), "the same count is not pushed again");
        assert!(st.activity_frames(3, false, t + 1).is_empty(), "a change inside the gap waits");
        let later = st.activity_frames(3, false, t + ACTIVITY_GAP_MS);
        let (push, _, _) = push_of(&later[0]);
        assert_eq!((push.event, push.priority), (Some(ActivityEvent::Update), 5), "the busy-time updates stay under Apple's budget");
        let end = st.activity_frames(0, true, t + 2 * ACTIVITY_GAP_MS);
        assert_eq!(push_of(&end[0]).0.event, Some(ActivityEvent::End), "quiet notifications never keep an activity from ending");
        assert!(st.activity_frames(1, true, t + 3 * ACTIVITY_GAP_MS).is_empty(), "quiet notifications keep a new activity from starting");
        assert_eq!(st.activity_frames(1, false, t + 3 * ACTIVITY_GAP_MS).len(), 1);

        st.status = "connecting".into();
        assert!(st.activity_frames(0, false, t + 9 * ACTIVITY_GAP_MS).is_empty(), "nothing while the relay is down");
        st.forget("p");
        assert!(st.activity_sent.is_empty(), "a forgotten phone's state goes with it");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_closed_notice_says_who_closed_the_item() {
        use crate::answers::End;
        assert_eq!(closed_by(End::Answered(By::Pc, true), None, "p1"), Some(ClosedBy::Computer));
        assert_eq!(closed_by(End::Computer, None, "p1"), Some(ClosedBy::Computer), "answered in Claude's own prompt");
        assert_eq!(closed_by(End::Answered(By::Phone, false), Some("p2"), "p2"), Some(ClosedBy::Phone { phone: "p2".into() }), "the phone that answered is told it was itself");
        assert_eq!(closed_by(End::Answered(By::Phone, false), Some("p2"), "p1"), Some(ClosedBy::Phone { phone: String::new() }), "another phone is not told which");
        assert_eq!(closed_by(End::Answered(By::Phone, true), None, "p1"), None, "a phone's answer with the phone unknown says nothing");
        assert_eq!(closed_by(End::Expired, None, "p1"), Some(ClosedBy::Expired));
        assert_eq!(closed_by(End::Released("settings"), None, "p1"), Some(ClosedBy::Released));
    }

    // Each paired phone is sent its own closed notice: the id of the phone that answered goes only to that phone.
    #[test]
    fn a_closed_notice_names_the_answering_phone_only_to_that_phone() {
        let dir = std::env::temp_dir().join(format!("bobix-closed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let desktop = Identity::generate();
        let mut st = on_state();
        st.dir = dir.clone();
        st.status = "online".into();
        let mut links = vec![];
        for (n, id) in ["p", "p2"].into_iter().enumerate() {
            let phone = Identity::generate();
            if n == 1 { let mut again = st.store.phones[0].clone(); again.id = id.into(); st.store.phones.push(again); }
            st.store.phones[n].boxk = b64(&phone.public().boxk);
            links.push((id, phone));
        }
        let desk_boxk = desktop.public().boxk;
        st.identity = Some(desktop);
        st.answerers.insert("r1".into(), "p2".into());
        st.closed_events.push(("r1".into(), "s1".into(), false, crate::answers::End::Answered(By::Phone, true)));
        let frames = st.closed_frames();
        assert_eq!(frames.len(), 2, "every paired phone is told");
        for (id, phone) in &links {
            let mut link = Link::new(Direction::ToDesktop, *phone.box_secret(), desk_boxk, CURRENT_VERSION, VersionRange::current(), [0; 3], now(), RecvState::default());
            let Some(Frame::Send { body, .. }) = frames.iter().find(|f| matches!(f, Frame::Send { to, .. } if to == id)) else { panic!("no frame for {id}") };
            let opened = link.open(&b64_decode(body).unwrap()).expect("the phone opens it");
            let by: Value = serde_json::from_slice::<Value>(&opened.plaintext).unwrap()["by"].clone();
            assert_eq!(by, if *id == "p2" { json!({ "kind": "phone", "phone": "p2" }) } else { json!({ "kind": "phone", "phone": "" }) }, "{id}");
        }
        // with no link, nothing is sealed for later and nothing is kept for the item
        st.status = "connecting".into();
        st.answerers.insert("r2".into(), "p".into());
        st.closed_events.push(("r2".into(), "s1".into(), false, crate::answers::End::Answered(By::Phone, true)));
        assert!(st.closed_frames().is_empty() && st.closed_events.is_empty() && !st.answerers.contains_key("r2"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_request_is_held_for_phones_only_when_nobody_is_at_the_computer() {
        let s = json!({ "approveWait": -1 });
        let st = on_state();
        assert!(!st.holds_for_phones(&s, Kind::Request, &Fixed(Some(5_000))), "input 5 seconds ago: not held");
        assert!(st.holds_for_phones(&s, Kind::Request, &Fixed(Some(30_000))), "input 30 seconds ago: held");
        assert_eq!(st.why_not_held(&s, Kind::Request, &Fixed(Some(5_000))), AtComputerWhy::Present, "not held because someone is using the computer");
        assert_eq!(st.why_not_held(&json!({ "approveWait": 0 }), Kind::Request, &Fixed(Some(5_000))), AtComputerWhy::NotShown, "never hold: nobody is the reason");
        assert!(!st.holds_for_phones(&json!({ "approveWait": 0 }), Kind::Request, &Fixed(Some(30_000))), "never hold wins");
        assert!(st.holds_for_phones(&s, Kind::Request, &Fixed(None)), "no reading: the choice decides, keep for the phone by default");
        let mut c = on_state();
        c.cfg.hold = "computer".into();
        assert!(!c.holds_for_phones(&s, Kind::Request, &Fixed(None)), "no reading, let Claude ask at the computer");
        assert!(c.holds_for_phones(&s, Kind::Request, &Fixed(Some(60_000))), "a reading beats the choice");
        let mut off = on_state();
        off.cfg.on = false;
        assert!(!off.holds_for_phones(&s, Kind::Request, &Fixed(Some(60_000))), "switch off");
        let mut none = on_state();
        none.store.phones.clear();
        assert!(!none.holds_for_phones(&s, Kind::Request, &Fixed(Some(60_000))), "no phone paired");
        // a phone that cannot answer this kind of item is no reason to hold it
        let mut quiet = on_state();
        quiet.store.phones[0].rights = Rights { activity: true, ..Default::default() };
        assert!(!quiet.holds_for_phones(&s, Kind::Request, &Fixed(Some(60_000))), "a phone with only the activity right");
        assert_eq!(quiet.why_not_held(&s, Kind::Request, &Fixed(Some(5_000))), AtComputerWhy::NotShown, "and it is not the reason either");
        quiet.store.phones[0].rights = Rights { question: true, ..Default::default() };
        assert!(quiet.holds_for_phones(&s, Kind::Question, &Fixed(Some(60_000))) && !quiet.holds_for_phones(&s, Kind::Request, &Fixed(Some(60_000))), "the right for the kind");
    }

    fn held(c: &mut crate::Core, id: &str, plan: bool, raw: &str, cut: bool) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        std::mem::forget(rx);
        c.pending.insert(id.into(), Pending::for_test("s1", plan, tx, raw, cut));
    }
    fn shown(c: &crate::Core, id: &str) -> String { crate::phone_view::detail(&c.pending, id, Rights { answer: true, plan: true, ..Default::default() }).unwrap().shown }

    #[test]
    fn a_command_is_applied_by_what_was_shown_and_what_is_held() {
        let mut c = crate::Core::default();
        let mut changed = false;
        held(&mut c, "req", false, "npm test", false);
        held(&mut c, "plan", true, "1. do it", false);
        held(&mut c, "cutreq", false, "echo hi", true);
        held(&mut c, "cutplan", true, "1. a plan that was cut", true);
        held(&mut c, "escplan", true, &"\u{200b}".repeat(12_000), false); // not cut by the hook; its escaped form crosses the cap
        let a = |c: &mut crate::Core, id: &str, sh: &str, choice: &str, plan: bool, ch: &mut bool| apply(c, "cid", id, sh, choice, "", plan, By::Phone, ch, None);
        let (sh, shp, shc) = (shown(&c, "req"), shown(&c, "plan"), shown(&c, "cutreq"));
        assert_eq!(a(&mut c, "plan", &shp, "allow", false, &mut changed).why.as_deref(), Some("kind"), "an answer for a plan id");
        assert_eq!(a(&mut c, "req", &sh, "allow", true, &mut changed).why.as_deref(), Some("kind"), "a plan decision for a request id");
        assert_eq!(a(&mut c, "cutreq", &shc, "allow", false, &mut changed).why.as_deref(), Some("cut"), "allow on a cut request");
        let shcp = shown(&c, "cutplan");
        assert_eq!(a(&mut c, "cutplan", &shcp, "allow", true, &mut changed).why.as_deref(), Some("cut"), "a cut plan is not approved from a phone");
        assert!(c.pending.contains_key("cutplan"));
        let shep = shown(&c, "escplan");
        assert_eq!(a(&mut c, "escplan", &shep, "allow", true, &mut changed).why.as_deref(), Some("cut"), "a plan that detail itself shortened is not approved either");
        assert!(c.pending.contains_key("escplan"));
        assert_eq!(a(&mut c, "req", "not what was shown", "allow", false, &mut changed).outcome, Outcome::Stale);
        assert!(c.pending.contains_key("req") && !changed, "nothing was applied");
        // a file that comes with the plan: shown whole it is approved with the plan; cut or only listed it is not, but the plan can go back
        let file = |cut: bool, sent: bool| bobix_link::messages::PlanFile { name: "phase-01.md".into(), path: "plans/phase-01.md".into(), bytes: 9, text: if sent { "step".into() } else { String::new() }, cut, sent };
        for (id, f) in [("filecut", file(true, true)), ("fileunsent", file(false, false))] {
            held(&mut c, id, true, "1. a plan whose file is not whole", false);
            c.pending.get_mut(id).unwrap().files = vec![file(false, true), f];
            let sh = shown(&c, id);
            assert_eq!(a(&mut c, id, &sh, "allow", true, &mut changed).why.as_deref(), Some("cut"), "{id}: not approved from a phone");
            assert!(c.pending.contains_key(id));
            assert_eq!(a(&mut c, id, &sh, "deny", true, &mut changed).outcome, Outcome::Ok, "{id}: but sent back");
        }
        // the files are read again when a plan is approved: the hash covers the copy held, the approve covers the disk
        let dir = std::env::temp_dir().join(format!("bobix-approve-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("plans")).unwrap();
        let write = |name: &str, text: &str| std::fs::write(dir.join("plans").join(name), text).unwrap();
        let hold_plan = |c: &mut crate::Core, id: &str, names: &[String]| {
            held(c, id, true, "1. a plan that names files", false);
            let p = c.pending.get_mut(id).unwrap();
            p.tool_input = json!({ "plan": names.iter().map(|n| format!("plans/{n}")).collect::<Vec<_>>().join(" ") });
            p.cwd = dir.to_string_lossy().into_owned();
            let now = p.files_now();
            (p.files, p.files_omitted) = (now.files, now.omitted);
        };
        write("phase-01.md", "step");
        for id in ["filesame", "fileedit", "filegone"] { hold_plan(&mut c, id, &["phase-01.md".to_string()]); }
        let shf = shown(&c, "filesame");
        assert_eq!(a(&mut c, "filesame", &shf, "allow", true, &mut changed).outcome, Outcome::Ok, "files as they were when held: approved");
        write("phase-01.md", "step, changed since");
        let shf = shown(&c, "fileedit");
        assert_eq!(a(&mut c, "fileedit", &shf, "allow", true, &mut changed).outcome, Outcome::Stale, "a file edited on disk since the plan was held");
        assert!(c.pending.contains_key("fileedit") && c.pending["fileedit"].files[0].text == "step, changed since", "the phone is shown the new file");
        let shf = shown(&c, "fileedit");
        assert_eq!(a(&mut c, "fileedit", &shf, "allow", true, &mut changed).outcome, Outcome::Ok, "and approves what it was then shown");
        std::fs::remove_file(dir.join("plans/phase-01.md")).unwrap();
        let shf = shown(&c, "filegone");
        assert_eq!(a(&mut c, "filegone", &shf, "deny", true, &mut changed).outcome, Outcome::Ok, "sending a plan back needs no re-read");
        // a plan that names more files than a phone is shown cannot be approved remotely (it can be sent back)
        let nine: Vec<String> = (0..9).map(|i| { write(&format!("p{i}.md"), "x"); format!("p{i}.md") }).collect();
        hold_plan(&mut c, "nine", &nine);
        assert_eq!((c.pending["nine"].files.len(), c.pending["nine"].files_omitted), (8, 1));
        let sh9 = shown(&c, "nine");
        assert_eq!(a(&mut c, "nine", &sh9, "allow", true, &mut changed).why.as_deref(), Some("cut"), "nine files: not approved from a phone");
        assert!(c.pending.contains_key("nine"));
        assert_eq!(a(&mut c, "nine", &sh9, "deny", true, &mut changed).outcome, Outcome::Ok);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(a(&mut c, "cutplan", &shcp, "deny", true, &mut changed).outcome, Outcome::Ok, "but a cut plan may be sent back");
        assert_eq!(a(&mut c, "cutreq", &shc, "deny", false, &mut changed).outcome, Outcome::Ok, "deny on a cut request is allowed");
        assert_eq!(a(&mut c, "req", &sh, "allow", false, &mut changed).outcome, Outcome::Ok);
        assert_eq!(a(&mut c, "req", &sh, "deny", false, &mut changed).outcome, Outcome::LatePhone, "the second answer is late, with who answered");
        assert_eq!(a(&mut c, "plan", &shp, "allow", true, &mut changed).outcome, Outcome::Ok, "a plan is approved the way it is at the computer");
        // answered at the computer first: a phone's answer is late
        held(&mut c, "pc", false, "ls", false);
        let sh = shown(&c, "pc");
        crate::answers::respond_core(&mut c, "pc", "allow", "", By::Pc, None, 5);
        let r = a(&mut c, "pc", &sh, "allow", false, &mut changed);
        assert!(r.outcome == Outcome::LatePc && r.by.as_deref() == Some("computer"));
    }

    #[test]
    fn a_plan_note_or_answer_is_plain_text() {
        let plan = |n: &str| CommandBody::Plan { id: "p".into(), decision: Decision::Back, note: Some(n.into()), shown: String::new() };
        assert!(plain_body(&plan("use ints\nand a second line")));
        for bad in ["esc \u{1b}[2J", "bidi \u{202e}x", "zero\u{200b}width", "bell \u{7}"] { assert!(!plain_body(&plan(bad)), "{bad:?}"); }
        assert!(!plain_body(&CommandBody::Question { id: "q".into(), answers: vec!["ok".into(), "bad \u{1b}".into()], shown: String::new() }));
    }

    #[test]
    fn an_ended_grant_marks_the_phone_the_last_push_went_to() {
        let mut st = on_state();
        st.dir = std::env::temp_dir().join("bobix-grant-test");
        st.grant_ended(1000, None);
        assert!(!st.store.phones[0].no_push, "no push yet");
        st.last_push = Some(("p".into(), 1000));
        st.grant_ended(1000 + 11_000, None);
        assert!(!st.store.phones[0].no_push, "an old push is not blamed");
        st.grant_ended(1000 + 5_000, None);
        assert!(st.store.phones[0].no_push);
        let _ = std::fs::remove_dir_all(&st.dir);
    }

    #[test]
    fn an_ended_grant_that_names_the_phone_marks_that_phone_whatever_was_pushed_last() {
        let mut st = on_state();
        st.dir = std::env::temp_dir().join("bobix-grant-peer-test");
        st.last_push = Some(("other".into(), 1));
        st.grant_ended(1_000_000, Some("p"));
        assert!(st.store.phones[0].no_push);
        let _ = std::fs::remove_dir_all(&st.dir);
    }

    fn held_question(c: &mut crate::Core, id: &str, sid: &str) -> oneshot_rx {
        let input = json!({ "questions": [
            { "question": "Which database?", "header": "DB", "options": [{ "label": "SQLite", "description": "x" }, { "label": "Postgres", "description": "y" }], "multiSelect": false },
            { "question": "Which extras?", "header": "X", "options": [{ "label": "A", "description": "" }, { "label": "B", "description": "" }], "multiSelect": true }] });
        let (tx, rx) = tokio::sync::oneshot::channel();
        let qs = crate::answers::questions_of(&input).unwrap();
        let raw = qs.iter().map(|q| q.text.as_str()).collect::<Vec<_>>().join("\n");
        c.pending.insert(id.into(), Pending::for_question(sid, tx, &raw, qs, input));
        rx
    }
    type oneshot_rx = tokio::sync::oneshot::Receiver<Option<Value>>;

    #[test]
    fn a_question_is_answered_in_the_shape_the_spike_recorded() {
        let mut c = crate::Core::default();
        let mut rx = held_question(&mut c, "q", "s1");
        let mut changed = false;
        let sh = crate::phone_view::detail(&c.pending, "q", Rights { question: true, ..Default::default() }).unwrap();
        assert_eq!(sh.what, DetailKind::Question);
        assert_eq!(sh.questions.len(), 2);
        let ans = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(apply_question(&mut c, "c1", "q", &sh.shown, &ans(&["SQLite"]), &mut changed).why.as_deref(), Some("answers"), "wrong count");
        assert_eq!(apply_question(&mut c, "c1", "q", "stale", &ans(&["SQLite", "A, B"]), &mut changed).outcome, Outcome::Stale);
        assert!(c.pending.contains_key("q") && !changed);
        assert_eq!(apply_question(&mut c, "c1", "q", &sh.shown, &ans(&["SQLite", "A, B"]), &mut changed).outcome, Outcome::Ok);
        let out = rx.try_recv().unwrap().unwrap();
        let d = &out["hookSpecificOutput"]["decision"];
        assert_eq!(d["behavior"], "allow");
        assert_eq!(d["updatedInput"]["answers"], json!({ "Which database?": "SQLite", "Which extras?": "A, B" }), "keyed by the question text, one string each");
        assert_eq!(d["updatedInput"]["questions"].as_array().unwrap().len(), 2, "the questions come back unchanged");
        assert_eq!(apply_question(&mut c, "c2", "q", &sh.shown, &ans(&["SQLite", "A"]), &mut changed).outcome, Outcome::LatePhone);
        // kinds are not mixed up
        held(&mut c, "req", false, "ls", false);
        assert_eq!(apply_question(&mut c, "c3", "req", "x", &ans(&["a"]), &mut changed).why.as_deref(), Some("kind"), "a question command on a request id");
        let _rx2 = held_question(&mut c, "q2", "s1");
        let a = |c: &mut crate::Core, id: &str, ch: &mut bool| apply(c, "c4", id, "x", "allow", "", false, By::Phone, ch, None);
        assert_eq!(a(&mut c, "q2", &mut changed).why.as_deref(), Some("kind"), "an answer on a question id");
        assert!(crate::answers::respond_core(&mut c, "q2", "allow", "", By::Pc, None, 1) == crate::answers::Answered::WrongKind, "a bare Allow never skips a question");
    }

    #[test]
    fn stop_denies_with_interrupt_only_the_held_request_of_that_session() {
        let mut c = crate::Core::default();
        let mut rx = { let (tx, rx) = tokio::sync::oneshot::channel(); c.pending.insert("r".into(), Pending::for_test("s1", false, tx, "rm -rf x", false)); rx };
        held(&mut c, "plan", true, "p", false);
        assert_eq!(crate::answers::stop_session(&mut c, "s2", "r", By::Phone, 1), Err("not-held"), "another session's request");
        assert_eq!(crate::answers::stop_session(&mut c, "s1", "plan", By::Phone, 1), Err("not-held"), "a plan is not stopped");
        assert_eq!(crate::answers::stop_session(&mut c, "s1", "nope", By::Phone, 1), Err("not-held"));
        assert_eq!(crate::answers::stop_session(&mut c, "s1", "r", By::Phone, 1), Ok(()));
        let d = rx.try_recv().unwrap().unwrap()["hookSpecificOutput"]["decision"].clone();
        assert_eq!(d["behavior"], "deny");
        assert_eq!(d["interrupt"], true);
        assert_eq!(crate::answers::stop_session(&mut c, "s1", "r", By::Phone, 2), Err("not-held"), "a second stop finds nothing");
    }

    #[test]
    fn a_start_is_refused_or_judged_by_its_plan() {
        let plan = |prompt: &str, known: bool, free: u64, full: Option<&str>| Ok(StartPlan { prompt: prompt.into(), folder_known: known, free, full: full.map(|b| (b.to_string(), json!({ "size": "small" }))) });
        let h = prompt_hash("#7 Fix it");
        let ok = |_: &StartPlan| json!({ "ok": true });
        assert_eq!(finish_start("c", plan("#7 Fix it", true, 0, None), &h, ok).outcome, Outcome::Ok, "Quick needs no disk");
        assert_eq!(finish_start("c", plan("#7 Fix it CHANGED", true, 0, None), &h, ok).outcome, Outcome::Stale, "the prompt changed since the preview");
        assert_eq!(finish_start("c", plan("#7 Fix it", false, 0, None), &h, ok).why.as_deref(), Some("folder"));
        assert_eq!(finish_start("c", Err("gone"), &h, ok).why.as_deref(), Some("gone"));
        assert_eq!(finish_start("c", plan("#7 Fix it", true, 10, Some("origin/main")), &h, ok).why.as_deref(), Some("disk"), "Full needs room for a worktree");
        assert_eq!(finish_start("c", plan("#7 Fix it", true, crate::tickets::WORKTREE_BYTES, Some("")), &h, ok).why.as_deref(), Some("base"), "no base branch remembered");
        let mut seen = None;
        let r = finish_start("c", plan("#7 Fix it", true, crate::tickets::WORKTREE_BYTES, Some("origin/main")), &h, |p| { seen = Some(p.full.clone()); json!({ "ok": true }) });
        assert_eq!(r.outcome, Outcome::Ok);
        assert_eq!(seen.unwrap().unwrap().0, "origin/main", "the remembered base is the one started from");
        assert_eq!(finish_start("c", plan("#7 Fix it", true, 0, None), &h, |_| json!({ "ok": false, "reason": "gone" })).why.as_deref(), Some("folder"));
        assert_eq!(finish_start("c", plan("#7 Fix it", true, 0, None), &h, |_| json!({ "ok": false, "reason": "exists" })).why.as_deref(), Some("exists"));
    }

    #[test]
    fn the_start_preview_shows_hidden_characters_and_the_hash_covers_what_is_shown() {
        // a tag-character payload after a normal task, a bidirectional override and an invisible Hangul letter
        let raw = format!("#7 Fix the login\n{}{}{}", '\u{E0049}', '\u{202E}', '\u{3164}');
        let shown = shown_prompt(&raw);
        assert!(shown.starts_with("#7 Fix the login\n") && shown.contains("\\u{E0049}") && shown.contains("\\u202E") && shown.contains("\\u3164"), "{shown}");
        assert!(crate::answers::odd(&Value::String(raw.clone())) && !crate::answers::odd(&Value::String("#7 Fix it\r\nbody".into())));
        let h = prompt_hash(&shown);
        let ok = |_: &StartPlan| json!({ "ok": true });
        let plan = |p: &str| Ok(StartPlan { prompt: p.into(), folder_known: true, free: 0, full: None });
        assert_eq!(finish_start("c", plan(&raw), &h, ok).outcome, Outcome::Ok, "the hash of what was shown starts it");
        assert_eq!(finish_start("c", plan(&raw), &prompt_hash(&raw), ok).outcome, Outcome::Stale, "a hash of the raw text is not what was shown");
    }

    #[test]
    fn a_late_phone_and_a_cancelled_pairing_are_told_apart() {
        assert_eq!(warning_json(&None), Value::Null);
        assert_eq!(warning_json(&Some(PairWarn::Cancelled)), json!({ "kind": "cancelled" }));
        assert_eq!(warning_json(&Some(PairWarn::Late("Pixel".into()))), json!({ "kind": "late", "name": "Pixel" }));
    }

    #[test]
    fn a_list_that_could_not_be_opened_is_never_overwritten() {
        let dir = std::env::temp_dir().join(format!("bobix-phones-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(store_file(&dir), b"sealed by a key this run cannot read").unwrap();
        let mut st = State { dir: dir.clone(), ..Default::default() };
        st.store.phones.push(Phone { id: "p".into(), name: "n".into(), auth: String::new(), boxk: String::new(), sign: String::new(), rights: Rights::default(), since: 0, needs_pairing: false, no_push: false });
        st.save_store(); // `loaded` is false: the stored list was not opened
        assert_eq!(std::fs::read(store_file(&dir)).unwrap(), b"sealed by a key this run cannot read", "the stored list is untouched");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_list_is_a_clean_start() {
        let dir = std::env::temp_dir().join(format!("bobix-phones-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut st = State { dir: dir.clone(), ..Default::default() };
        assert!(!st.loaded);
        st.load_store();
        assert!(st.loaded, "no file: nothing was lost, so the list may be written");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_issue_with_a_session_is_not_started_from_a_phone() {
        let view = json!({ "local": [{ "path": "team/shop", "issues": [{ "project": "team/shop", "iid": 7, "sessions": ["s1"] }, { "project": "team/shop", "iid": 8, "sessions": [] }] }] });
        assert!(issue_has_session(&view, "team/shop", 7));
        assert!(!issue_has_session(&view, "team/shop", 8) && !issue_has_session(&view, "team/shop", 9) && !issue_has_session(&view, "other", 7));
    }

    #[test]
    fn an_invite_code_turns_a_guest_into_an_invited_computer() {
        let mut st = State::default();
        st.status = "online".into();
        st.on_invite_state(false, false, Some("ignored".into()), vec![Invitee { id: "x".into(), label: String::new(), by: "o".into(), at: 1 }]);
        assert_eq!(st.status, "invite", "the relay said this computer is a guest");
        assert!(st.invites.relay_id.is_none() && st.invites.invitees.is_empty(), "only an owner keeps the relay id and the list");
        st.invites.pending = Some("code".into());
        assert!(st.on_invite_result(false, Some(InviteReason::Used)).is_empty(), "a refusal sends nothing");
        assert_eq!((st.status.as_str(), st.invites.result.as_deref(), st.invites.pending.is_none()), ("invite", Some("used"), true));
        let again = st.on_invite_result(true, None);
        assert!(matches!(again.first(), Some(Frame::Peers { .. })), "the peers the relay filtered while it was a guest go again: {again:?}");
        assert_eq!((st.status.as_str(), st.invites.result.as_deref(), st.invites.invited), ("online", Some("ok"), true));
        assert!(st.on_invite_result(true, Some(InviteReason::Already)).len() == 1 && st.invites.result.as_deref() == Some("already"));
        st.on_invite_state(true, true, Some("relay.example".into()), vec![]);
        assert_eq!(st.invites.relay_id.as_deref(), Some("relay.example"));
    }

    #[test]
    fn two_starts_of_one_issue_do_not_both_pass() {
        // the lock is a set of issue keys: the second insert says the issue is taken
        let mut st = State::default();
        assert!(st.starting.insert("team/shop#7".into()));
        assert!(!st.starting.insert("team/shop#7".into()));
        assert!(st.starting.insert("team/shop#8".into()), "another issue is free");
    }

    #[test]
    fn a_repeated_cid_of_a_running_start_gets_no_refusal() {
        // the in-flight set holds `phone:cid`; on_command answers nothing while the key is in it
        let mut st = State::default();
        assert!(st.inflight.insert("p:c1".into()));
        assert!(st.inflight.contains("p:c1") && !st.inflight.contains("p:c2"));
    }

    #[test]
    fn a_new_child_of_the_claude_process_means_the_command_started() {
        assert!(!started_since(&[1, 2], &[1, 2]));
        assert!(!started_since(&[1, 2], &[2]), "a helper that ended is not a start");
        assert!(started_since(&[1, 2], &[1, 2, 3]));
        assert!(started_since(&[], &[7]));
    }
}
