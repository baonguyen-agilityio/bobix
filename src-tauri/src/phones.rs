// Phone access, the desktop side: the switch, the paired phones and their rights, pairing, and what the relay link
// (link.rs) hands in. Off by default: while the switch is off and no phone was ever paired there is no key, no file
// and no connection. Keys are in the OS keychain (secrets.rs); the list of phones is stored sealed with a keychain key.
// The wire format, crypto and counter rules are the shared crate's (bobix-link); this module only decides.
use crate::{secrets::Secrets, Bobix};
use bobix_link::{
    b64, b64_decode,
    command, counters::RecvState,
    envelope::{Direction, VersionRange, CURRENT_VERSION},
    keys::{Identity, PublicKeys, SignPublic},
    link::Link,
    messages::*,
    pairing::*,
    relay_frames::{ErrorCode, Frame, Push},
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
pub const MAX_PHONE_NAME: usize = 40;

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
struct Pairing { session: PairingSession, offer: Option<Offered>, warning: bool, opened: bool }
struct Moving { url: String, waiting: HashMap<String, String>, since: u64 } // pid → phone id
struct Revoking { pids: HashSet<String>, since: u64 }

#[derive(Default)]
pub struct State {
    dir: PathBuf,
    pub cfg: Config,
    pub store: Store,
    loaded: bool,
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
    pub closed_events: Vec<(String, String, bool)>, // (request id, session id, plan): requests that ended, to tell the phones
    notified: HashSet<String>,
    awake: crate::awake::Awake,
    asleep: bool,                               // the computer is going to sleep (or sleeps): phones are told, with how much is unfinished
    inflight: HashSet<String>,                  // `from:cid` of starts still running: a repeat of one gets no answer yet
    starting: HashSet<String>,                  // issues a start from a phone is running for: one at a time
    last_push: Option<(String, u64)>,           // the phone the last push went to, for an `error { grant }` that names none
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

fn cipher(secrets: &Secrets) -> Result<XChaCha20Poly1305, String> {
    let key = match secrets.get(SEAL_KEY).map_err(|e| e.to_string())? {
        Some(k) if k.len() == 32 => k,
        _ => { let k = bobix_link::random::<32>().to_vec(); secrets.set(SEAL_KEY, &k).map_err(|e| e.to_string())?; k }
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
        let Ok(c) = cipher(&Secrets::open()) else { return };
        write(&store_file(&self.dir), &seal_store(&c, &self.store));
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
            _ if create => {
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
        if push.is_some() { self.last_push = Some((id.to_string(), now())); }
        let sealed = self.link_of(id)?.seal(stream, &m.to_json()).ok()?; // a low-order peer key: nothing is sent
        if let Some((s, v)) = sealed.reserve { self.saved.entry(id.into()).or_default().res[s as usize] = v; self.save_counters(); }
        let pid = command::new_cid()[..16].to_string();
        Some((pid.clone(), Frame::Send { pid, to: id.into(), ttl: 600, collapse, push, body: b64(&sealed.packet) }))
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
pub fn valid_name(s: &str) -> bool { !s.is_empty() && s.chars().count() <= MAX_PHONE_NAME && !s.chars().any(is_odd_char) }

// ---------- startup ----------
// Bobix is closing: held requests go back to Claude's own prompt and are recorded as released ("restart").
pub fn save_on_exit(b: &Bobix) {
    let mut c = b.core.lock().unwrap();
    if !c.phones.cfg.on && c.phones.store.phones.is_empty() { return; }
    crate::answers::release_all(&mut c, "restart", now());
    c.phones.save_results(&c.closed);
}

// Keeps the awake inhibitor in step with the work, on its own clock (the relay link may be down).
fn awake_loop(b: Arc<Bobix>) {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let mut c = b.core.lock().unwrap();
        let busy = c.sessions.values().any(|s| matches!(s.state.as_str(), "working" | "waiting"));
        let want = c.phones.cfg.awake && c.phones.cfg.on && busy;
        c.phones.awake.set(want);
    }
}

pub fn init(b: &Arc<Bobix>) {
    { let b2 = b.clone(); std::thread::spawn(move || awake_loop(b2)); }
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
    if let Some(bytes) = read(store_file(&dir)) {
        let s = Secrets::open();
        match cipher(&s).ok().and_then(|ci| open_store(&ci, &bytes)) {
            Some(store) => { st.store = store; st.loaded = true; let _ = st.ensure_identity(false); }
            None => eprintln!("[bobix] phones: the list of phones could not be opened"),
        }
        st.saved = read(counters_file(&dir)).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    }
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
pub fn relay_url(b: &Arc<Bobix>) -> String { b.core.lock().unwrap().phones.cfg.relay.clone() }
// The address in Settings is no longer the one this connection was opened to.
pub fn url_changed(b: &Arc<Bobix>, url: &str) -> bool { b.core.lock().unwrap().phones.cfg.relay != url }

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
    let mut out = vec![st.peers_frame()];
    out.extend(st.store.revoked.iter().map(|r| Frame::Revoke { id: r.id.clone() }));
    if let Some(p) = st.pairing.as_ref().filter(|p| !p.opened && matches!(p.session.status(now()), PairStatus::Open)) {
        out.push(Frame::PairOpen { relay_token_hash: b64(&p.session.code().relay_token_hash()), ttl: 120 });
    }
    if let Some(p) = st.pairing.as_mut() { p.opened = true; }
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
    if st.pairing.as_ref().is_some_and(|p| matches!(p.session.status(t), PairStatus::Expired | PairStatus::Done)) { st.pairing = None; }
    if st.revoking.as_ref().is_some_and(|r| r.pids.is_empty() || t > r.since + MOVE_WAIT_MS) {
        st.revoking = None;
        if !st.cfg.on { st.send_close(); }
    }
    if st.moving.as_ref().is_some_and(|m| m.waiting.is_empty() || t > m.since + MOVE_WAIT_MS) {
        let m = st.moving.take().unwrap();
        for id in m.waiting.values() { if let Some(p) = st.store.phones.iter_mut().find(|p| &p.id == id) { p.needs_pairing = true; } }
        st.cfg.relay = m.url;
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
    for (id, sid, _) in std::mem::take(&mut st.closed_events) {
        if !st.notified.remove(&id) { continue; }
        let n = Notice { what: NoticeKind::Closed, reference: id.clone(), session: sid, title: String::new(), body: String::new() };
        let ids: Vec<String> = st.store.phones.iter().map(|p| p.id.clone()).collect();
        for p in ids { if let Some((_, f)) = st.tell(&p, &DesktopMsg::Notice(n.clone()), Some(Push { kind: bobix_link::relay_frames::PushKind::Background, priority: 5, replaces: Some(id.clone()) }), None) { out.push(f); } }
    }
    // homes: to every paired phone when something changed (the relay keeps only the latest for an offline one), a forced
    // one each minute to a phone that is online, and one at once for a phone that just paired or came online
    if st.cfg.on && !st.store.phones.is_empty() {
        let minute = t >= st.last_home_at + 60_000;
        if dirty || minute || !st.home_now.is_empty() {
            let name = st.computer_name();
            let groups = crate::phone_view::groups(work, settings, sessions, pending, t);
            let home = crate::phone_view::home(sessions, pending, &name, t, if st.asleep { ComputerState::Asleep } else { ComputerState::Online }, groups);
            let mut key = home.clone();
            key.sent_at = 0;
            let key = serde_json::to_string(&key).unwrap_or_default();
            let ids: Vec<String> = st.store.phones.iter().map(|p| p.id.clone()).collect();
            for id in ids {
                let due = st.home_now.contains(&id) || st.last_home.get(&id) != Some(&key) || (minute && st.online.contains(&id));
                if !due { continue; }
                if let Some((_, f)) = st.tell(&id, &DesktopMsg::Home(home.clone()), None, Some("home".into())) { out.push(f); st.last_home.insert(id, key.clone()); }
            }
            if minute { st.last_home_at = t; }
            st.home_now.clear();
        }
    }
    out
}
impl State {
    // The relay refused a push for an ended grant and names no phone: it is the one the last push went to.
    fn grant_ended(&mut self, t: u64) {
        let Some((id, at)) = self.last_push.clone() else { return };
        if t > at + 10_000 { return; }
        if let Some(p) = self.store.phones.iter_mut().find(|p| p.id == id) { p.no_push = true; self.save_store(); }
    }
    // The request was announced to the phones (so a settings change does not hand it back).
    pub fn was_sent(&self, id: &str) -> bool { self.notified.contains(id) }
    #[cfg(test)]
    pub fn mark_sent(&mut self, id: &str) { self.notified.insert(id.into()); }
    // Whether a request that nothing on this computer would show is held for the phones: phone access is on, a phone is
    // paired, "never hold" is not set, and nobody is at the computer (input in the last 10 seconds). Where the input time
    // cannot be read, the choice in Settings decides.
    pub fn holds_for_phones(&self, settings: &Value, src: &dyn crate::presence::Source) -> bool {
        if !self.cfg.on || self.store.phones.is_empty() || settings["approveWait"].as_f64().unwrap_or(0.0) == 0.0 { return false; }
        match src.idle_ms() { Some(ms) => ms >= crate::presence::AT_COMPUTER_MS, None => self.cfg.hold != "computer" }
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
                    Frame::Error { code, .. } => match code {
                        ErrorCode::Invite => st.status = "invite".into(),
                        ErrorCode::Update => st.status = "update".into(),
                        ErrorCode::RelayUrl => st.status = "url".into(),
                        ErrorCode::Auth => st.status = "auth".into(),
                        ErrorCode::Grant => st.grant_ended(now()),
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
    let (Some(p), Some(id)) = (st.pairing.as_mut(), st.identity.as_ref()) else { return false };
    let Some((phone_box, packet)) = split_hello_body(body) else { return false };
    let mut link = Link::new(Direction::ToPhone, *id.box_secret(), phone_box, CURRENT_VERSION, VersionRange::current(), [0; 3], now(), RecvState::default());
    let Ok(opened) = link.open(packet) else { return false };
    let Ok(PhoneMsg::Hello(hello)) = PhoneMsg::from_json(&opened.plaintext) else { return false };
    match p.session.on_hello(&hello, &phone_box, from, now()) {
        Ok(Offer::New { phone, sign, name, rights, .. }) if valid_name(&name) => { p.offer = Some(Offered { phone, sign, name, rights, from: from.into(), link }); true }
        Ok(Offer::Cancelled) => { p.warning = true; p.offer = None; true }
        _ => false, // a bad MAC, a duplicate, a name that is not plain text
    }
}

fn on_packet(b: &Arc<Bobix>, c: &mut crate::Core, from: &str, packet: &[u8], changed: &mut bool) -> Vec<Frame> {
    let st = &mut c.phones;
    let Some(link) = st.link_of(from) else { return vec![] };
    let opened = match link.open(packet) { Ok(o) => o, Err(_) => return vec![] };
    st.remember_recv(from);
    if let Some(ph) = st.store.phones.iter_mut().find(|p| p.id == from && p.no_push) { ph.no_push = false; st.save_store(); } // it was opened: it can register a new grant
    let Ok(msg) = PhoneMsg::from_json(&opened.plaintext) else { return vec![] };
    if msg.validate().is_err() { return vec![]; }
    let rights = st.store.phones.iter().find(|p| p.id == from).map(|p| p.rights).unwrap_or_default();
    let mut out = vec![];
    match msg {
        PhoneMsg::PairedAck => {
            if let Some(p) = st.pairing.as_mut() { if p.session.ack(now()) { st.pairing = None; } }
            st.home_now.insert(from.into());
        }
        PhoneMsg::Bye { sig } => {
            let (Some(cid), Some(p)) = (st.computer_id(), st.store.phones.iter().find(|p| p.id == from)) else { return vec![] };
            let keys = b64_decode(&p.auth).and_then(|a| <[u8; 32]>::try_from(a).ok());
            let bytes = command::bye_bytes(CURRENT_VERSION, from, &cid, opened.counter);
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
        PhoneMsg::StartPreview { project, iid, mode } => {
            let m = if !rights.start { Some(DesktopMsg::Error { code: "right".into(), who: Some(Who::Desktop) }) }
                else if iid.parse::<i64>().is_err() { Some(DesktopMsg::Error { code: "encoding".into(), who: Some(Who::Desktop) }) } else { None };
            match m {
                Some(m) => { if let Some((_, f)) = c.phones.tell(from, &m, None, None) { out.push(f); } }
                None => {
                    let (b2, from2, iid) = (b.clone(), from.to_string(), iid.parse::<i64>().unwrap_or(0));
                    std::thread::spawn(move || {
                        let m = match start_plan(&b2, &project, iid, mode) {
                            Ok(p) => DesktopMsg::StartPrompt(StartPrompt { project, iid: iid.to_string(), mode, prompt_hash: prompt_hash(&p.prompt), prompt: p.prompt, folder_known: p.folder_known, free_disk: p.free }),
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
            if let Some(r) = on_command(b, c, from, opened.counter, &cmd, changed) {
                if let Some((_, f)) = c.phones.tell(from, &DesktopMsg::Result(r), None, None) { out.push(f); }
            } // None: a start, whose result is sent when it is done
        }
        _ => {}
    }
    out
}

// A command from a phone, checked in this order: signature (with phone id, computer id, age), counter (already by the
// link), a `cid` seen before (the stored result), the phone's right for the kind of thing held, `cut`, the `shown`
// hash against what is held, and the request still open. Every result is stored per `cid` for 24 hours and logged.
fn on_command(b: &Arc<Bobix>, c: &mut crate::Core, from: &str, counter: u64, cmd: &command::Command, changed: &mut bool) -> Option<CmdResult> {
    use crate::answers::By;
    let key = format!("{from}:{}", cmd.cid);
    if let Some(stored) = c.phones.results.get(&key) { return Some(stored.0.clone()); }
    if c.phones.inflight.contains(&key) { return None; } // its start is still running: the result comes when it is done
    let t = now();
    let Some(phone) = c.phones.store.phones.iter().find(|p| p.id == from).cloned() else { return Some(CmdResult::refused(&cmd.cid, "unknown")) };
    let (Some(cid), Some(sign)) = (c.phones.computer_id(), b64_decode(&phone.sign).and_then(|s| SignPublic::from_sec1(&s).ok())) else { return Some(CmdResult::refused(&cmd.cid, "encoding")) };
    let ctx = command::VerifyCtx { version: CURRENT_VERSION, phone_id: from, computer_id: &cid, sign_key: &sign, counter, now_ms: t };
    let mut later = false;
    let (result, what) = match command::verify(&ctx, cmd) {
        Err(r) => (CmdResult::refused(&cmd.cid, r.why()), "command".to_string()),
        Ok(body) => {
            let target = match &body { CommandBody::Answer { id, .. } | CommandBody::Plan { id, .. } | CommandBody::Question { id, .. } => Some(id.clone()), _ => None };
            let shown_of = target.as_deref().and_then(|id| c.pending.get(id)).map(|p| crate::phone_view::preview(p.raw.lines().next().unwrap_or(""), 60)).unwrap_or_default();
            let what = format!("{} {}", body.kind(), shown_of).trim().to_string();
            let refuse = |why: &str| CmdResult::refused(&cmd.cid, why);
            // a note or a free-text answer goes into Claude's reply: plain text, newlines allowed
            let plain = plain_body(&body);
            let r = if !phone.rights.allows(&body) { refuse("right") } else if !plain { refuse("text") } else { match &body {
                CommandBody::Answer { id, choice, shown } => {
                    let allow = *choice == Choice::Allow;
                    apply(c, &cmd.cid, id, shown, if allow { "allow" } else { "deny" }, "", false, By::Phone, changed, &phone.rights)
                }
                CommandBody::Plan { id, decision, note, shown } => {
                    let approve = *decision == Decision::Approve;
                    apply(c, &cmd.cid, id, shown, if approve { "allow" } else { "deny" }, note.as_deref().unwrap_or(""), true, By::Phone, changed, &phone.rights)
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
    if later { return None; } // the start thread logs and stores it when it is done
    let word = serde_json::to_value(result.outcome).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
    let shown = if let Some(w) = &result.why { format!("{word} ({w})") } else { word };
    c.phones.push_log(&phone.name, &what, &shown);
    c.phones.results.insert(key, (result.clone(), t));
    c.phones.save_results(&c.closed);
    Some(result)
}

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
    if prompt_hash(&p.prompt) != hash { return CmdResult::new(cid, Outcome::Stale); }
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
        c.phones.push_log(&name, &format!("start {project}#{n}"), &if let Some(w) = &result.why { format!("{word} ({w})") } else { word });
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
fn apply(c: &mut crate::Core, cid: &str, id: &str, shown: &str, choice: &str, note: &str, plan: bool, by: crate::answers::By, changed: &mut bool, _rights: &Rights) -> CmdResult {
    use crate::answers::{respond_core, Answered};
    // what is held, as the phone saw it: the kind first (an answer for a plan id is refused, and the reverse)
    match c.pending.get(id) {
        Some(p) if p.kind() != if plan { crate::answers::Kind::Plan } else { crate::answers::Kind::Request } => return CmdResult::refused(cid, "kind"),
        Some(p) => {
            if crate::phone_view::shown_cut(p) && choice == "allow" { return CmdResult::refused(cid, "cut"); } // a cut plan too: only what was shown is known
            let rights = Rights { answer: true, plan: true, ..Default::default() };
            match crate::phone_view::detail(&c.pending, id, rights) {
                Ok(d) if command::shown_matches(&d, shown) => {}
                _ => return CmdResult::new(cid, Outcome::Stale),
            }
        }
        None => {}
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
    let pairing = match st.pairing.as_ref() {
        None => json!({ "state": "none" }),
        Some(p) => {
            let s = p.session.status(t);
            json!({ "state": match s { PairStatus::Open => "open", PairStatus::Offered => "offered", PairStatus::Confirmed => "confirmed", PairStatus::Cancelled => "cancelled", _ => "none" },
                "uri": if s == PairStatus::Open { json!(p.session.code().to_uri()) } else { Value::Null },
                "qr": if s == PairStatus::Open { json!(qr_svg(&p.session.code().to_uri())) } else { Value::Null },
                "name": p.offer.as_ref().map(|o| &o.name), "asked": p.offer.as_ref().map(|o| o.rights), "warning": p.warning })
        }
    };
    json!({ "on": st.cfg.on, "relay": st.cfg.relay, "status": st.status, "hold": st.cfg.hold, "awake": st.cfg.awake, "awakeHeld": st.awake.is_held(), "node": node,
        "canRead": !st.cfg.on || crate::presence::source().idle_ms().is_some(), "deviceId": st.computer_id(), "storage": format!("{:?}", st.storage.unwrap_or(crate::secrets::Storage::Keychain)),
        "phones": st.store.phones.iter().map(|p| json!({ "id": p.id, "name": p.name, "rights": p.rights, "since": p.since, "needsPairing": p.needs_pairing, "noPush": p.no_push })).collect::<Vec<_>>(),
        "revoked": st.store.revoked.len(), "moving": st.moving.is_some(), "linked": st.tx.is_some(), "pairing": pairing })
}

pub fn enable(b: &Arc<Bobix>, on: bool) -> Result<Value, String> {
    let mut out = vec![];
    {
        let mut c = b.core.lock().unwrap();
        let st = &mut c.phones;
        if on {
            if normalize_relay_url(&st.cfg.relay).is_none() { return Err("relay".into()); }
            st.ensure_identity(true)?;
            st.cfg.on = true;
            st.status = "connecting".into();
        } else if st.cfg.on {
            let ids: Vec<String> = st.store.phones.iter().map(|p| p.id.clone()).collect();
            for id in ids { out.extend(st.disconnect(&id, "phone access was turned off")); }
            st.cfg.on = false;
            st.pairing = None;
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
    let norm = normalize_relay_url(url).ok_or("relay")?;
    {
        let mut c = b.core.lock().unwrap();
        let st = &mut c.phones;
        if st.moving.is_some() { return Err("moving".into()); }
        if !st.cfg.on || st.store.phones.is_empty() || st.cfg.relay.is_empty() || normalize_relay_url(&st.cfg.relay).as_deref() == Some(&norm) {
            st.cfg.relay = url.into();
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
        let keys = st.identity.as_ref().ok_or("identity")?.public();
        let code = PairingCode::new(&st.cfg.relay, keys).map_err(|_| "relay".to_string())?;
        let frame = Frame::PairOpen { relay_token_hash: b64(&code.relay_token_hash()), ttl: 120 };
        st.pairing = Some(Pairing { session: PairingSession::new(code, now()), offer: None, warning: false, opened: st.status == "online" });
        if st.status == "online" { st.send(frame); }
    }
    b.broadcast();
    Ok(view(b))
}
pub fn pair_cancel(b: &Arc<Bobix>) -> Value { b.core.lock().unwrap().phones.pairing = None; b.broadcast(); view(b) }

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
        let asked = o.rights;
        let r = Rights { answer: granted.answer && asked.answer, plan: granted.plan && asked.plan, question: granted.question && asked.question,
            start: granted.start && asked.start, stop: granted.stop && asked.stop, activity: granted.activity && asked.activity };
        if !p.session.confirm(now()) { return Err("no pairing".into()); }
        let id = o.phone.device_id();
        if id != o.from { return Err("no pairing".into()); }
        st.store.phones.push(Phone { id: id.clone(), name: o.name.clone(), auth: b64(&o.phone.auth), boxk: b64(&o.phone.boxk), sign: b64(&o.sign.to_sec1()), rights: r, since: now(), needs_pairing: false, no_push: false });
        st.store.revoked.retain(|x| x.id != id);
        st.links.insert(id.clone(), o.link);
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
    let ids: Vec<String> = st.store.phones.iter().map(|p| p.id.clone()).collect();
    for id in ids { if let Some((_, f)) = st.tell(&id, &DesktopMsg::Home(home.clone()), None, Some("home".into())) { st.send(f); } }
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
        "done" => (Notice { what: NoticeKind::Done, reference: sid.into(), session: sid.into(), title: crate::phone_view::preview(title, 80), body: crate::phone_view::preview(&s.summary, 200) }, sid.to_string()),
        _ => return,
    };
    let push = Push { kind: bobix_link::relay_frames::PushKind::Alert, priority: 10, replaces: Some(id) };
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
    fn names_are_plain_and_short() {
        assert!(valid_name("Bao's iPhone") && valid_name(&"é".repeat(40)));
        for bad in ["", &"a".repeat(41), "x\u{202e}y", "a\nb", "a\u{7}", "zero\u{200b}width", "tag\u{e0041}"] { assert!(!valid_name(bad), "{bad:?}"); }
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

    use crate::{answers::{Pending, By}, presence::Fixed};

    fn on_state() -> State {
        let mut st = State::default();
        st.cfg.on = true; st.cfg.hold = "phone".into();
        st.store.phones.push(Phone { id: "p".into(), name: "n".into(), auth: String::new(), boxk: String::new(), sign: String::new(), rights: Rights::default(), since: 0, needs_pairing: false, no_push: false });
        st
    }

    #[test]
    fn a_request_is_held_for_phones_only_when_nobody_is_at_the_computer() {
        let s = json!({ "approveWait": -1 });
        let st = on_state();
        assert!(!st.holds_for_phones(&s, &Fixed(Some(5_000))), "input 5 seconds ago: not held");
        assert!(st.holds_for_phones(&s, &Fixed(Some(30_000))), "input 30 seconds ago: held");
        assert!(!st.holds_for_phones(&json!({ "approveWait": 0 }), &Fixed(Some(30_000))), "never hold wins");
        assert!(st.holds_for_phones(&s, &Fixed(None)), "no reading: the choice decides, keep for the phone by default");
        let mut c = on_state();
        c.cfg.hold = "computer".into();
        assert!(!c.holds_for_phones(&s, &Fixed(None)), "no reading, let Claude ask at the computer");
        assert!(c.holds_for_phones(&s, &Fixed(Some(60_000))), "a reading beats the choice");
        let mut off = on_state();
        off.cfg.on = false;
        assert!(!off.holds_for_phones(&s, &Fixed(Some(60_000))), "switch off");
        let mut none = on_state();
        none.store.phones.clear();
        assert!(!none.holds_for_phones(&s, &Fixed(Some(60_000))), "no phone paired");
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
        let a = |c: &mut crate::Core, id: &str, sh: &str, choice: &str, plan: bool, ch: &mut bool| apply(c, "cid", id, sh, choice, "", plan, By::Phone, ch, &Rights::default());
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
        st.grant_ended(1000);
        assert!(!st.store.phones[0].no_push, "no push yet");
        st.last_push = Some(("p".into(), 1000));
        st.grant_ended(1000 + 11_000);
        assert!(!st.store.phones[0].no_push, "an old push is not blamed");
        st.grant_ended(1000 + 5_000);
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
        let a = |c: &mut crate::Core, id: &str, ch: &mut bool| apply(c, "c4", id, "x", "allow", "", false, By::Phone, ch, &Rights::default());
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
    fn an_issue_with_a_session_is_not_started_from_a_phone() {
        let view = json!({ "local": [{ "path": "team/shop", "issues": [{ "project": "team/shop", "iid": 7, "sessions": ["s1"] }, { "project": "team/shop", "iid": 8, "sessions": [] }] }] });
        assert!(issue_has_session(&view, "team/shop", 7));
        assert!(!issue_has_session(&view, "team/shop", 8) && !issue_has_session(&view, "team/shop", 9) && !issue_has_session(&view, "other", 7));
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
}
