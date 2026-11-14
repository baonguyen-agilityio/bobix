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
}

pub fn now() -> u64 { crate::state::now() }

// ---------- files ----------
fn cfg_file(d: &Path) -> PathBuf { d.join("phones.json") }
fn store_file(d: &Path) -> PathBuf { d.join("phones.sealed") }
fn counters_file(d: &Path) -> PathBuf { d.join("phone-counters.json") }
fn log_file(d: &Path) -> PathBuf { d.join("phone-log.json") }

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
    fn save_cfg(&self) { write(&cfg_file(&self.dir), &serde_json::to_vec_pretty(&self.cfg).unwrap_or_default()); }
    fn save_store(&self) {
        let Ok(c) = cipher(&Secrets::open()) else { return };
        write(&store_file(&self.dir), &seal_store(&c, &self.store));
    }
    fn save_counters(&self) { write(&counters_file(&self.dir), &serde_json::to_vec(&self.saved).unwrap_or_default()); }
    fn save_log(&self) { write(&log_file(&self.dir), &serde_json::to_vec(&self.log).unwrap_or_default()); }
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
        let sealed = self.link_of(id)?.seal(stream, &m.to_json());
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

    // Disconnects a phone on the computer's side: tells it, tells the relay, and drops it from the peers.
    fn disconnect(&mut self, id: &str, why: &str) -> Vec<Frame> {
        let told = self.tell(id, &DesktopMsg::Revoked { why: why.into() }, None, None);
        let mut out = vec![];
        if let Some((pid, f)) = told { out.push(f); self.revoking.get_or_insert_with(|| Revoking { pids: HashSet::new(), since: now() }).pids.insert(pid); }
        if self.forget(id).is_some() {
            out.push(Frame::Revoke { id: id.into() });
            out.push(self.peers_frame());
        }
        out
    }
}

// ---------- validation of what a phone sends ----------
// A name is 1 to 40 characters with no control or format character (it arrives before the owner confirms, and the panel builds HTML).
pub fn valid_name(s: &str) -> bool { !s.is_empty() && s.chars().count() <= MAX_PHONE_NAME && !s.chars().any(is_odd_char) }

// ---------- startup ----------
pub fn init(b: &Arc<Bobix>) {
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
    st.status = if st.cfg.on { "connecting".into() } else { "off".into() };
    let want = st.cfg.on;
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
    let crate::Core { phones: st, pending, sessions, .. } = &mut *c;
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
    // requests that ended: the phones that were told of them are told they are gone
    let mut out = vec![];
    for (id, sid, _) in std::mem::take(&mut st.closed_events) {
        if !st.notified.remove(&id) { continue; }
        let n = Notice { what: NoticeKind::Closed, reference: id.clone(), session: sid, title: String::new(), body: String::new() };
        let ids: Vec<String> = st.store.phones.iter().map(|p| p.id.clone()).collect();
        for p in ids { if let Some((_, f)) = st.tell(&p, &DesktopMsg::Notice(n.clone()), Some(Push { kind: bobix_link::relay_frames::PushKind::Background, priority: 5, replaces: Some(id.clone()) }), None) { out.push(f); } }
    }
    // homes: when something changed, once a minute, and for a phone that just came online or paired
    if st.cfg.on && !st.store.phones.is_empty() {
        let minute = t >= st.last_home_at + 60_000;
        let ids: Vec<String> = st.store.phones.iter().map(|p| p.id.clone()).filter(|id| st.online.contains(id) || st.home_now.contains(id)).collect();
        if !ids.is_empty() && (dirty || minute || !st.home_now.is_empty()) {
            let name = st.computer_name();
            let home = crate::phone_view::home(sessions, pending, &name, t);
            let mut key = home.clone();
            key.sent_at = 0;
            let key = serde_json::to_string(&key).unwrap_or_default();
            for id in ids {
                if !(minute || st.home_now.contains(&id) || st.last_home.get(&id) != Some(&key)) { continue; }
                if let Some((_, f)) = st.tell(&id, &DesktopMsg::Home(home.clone()), None, Some("home".into())) { out.push(f); st.last_home.insert(id, key.clone()); }
            }
            if minute { st.last_home_at = t; }
            st.home_now.clear();
        }
    }
    out
}
impl State { fn send_close(&self) { if let Some(tx) = &self.tx { let _ = tx.send(Out::Close); } } }

// ---------- frames from the relay ----------
pub fn on_frame(b: &Arc<Bobix>, f: Frame) -> Vec<Frame> {
    let mut out = vec![];
    let mut changed = false;
    {
        let mut c = b.core.lock().unwrap();
        let crate::Core { phones: st, pending, .. } = &mut *c;
        match f {
            Frame::Msg { pid, from, body } => {
                out.push(Frame::Ack { pid });
                if let Some(packet) = b64_decode(&body) {
                    if st.store.phones.iter().any(|p| p.id == from) { out.extend(on_packet(st, pending, &from, &packet)); }
                    else { changed = on_hello(st, &from, &packet); }
                }
            }
            Frame::Delivered { pid } => {
                if let Some(m) = st.moving.as_mut() { m.waiting.remove(&pid); }
                if let Some(r) = st.revoking.as_mut() { r.pids.remove(&pid); }
            }
            Frame::Presence { peer, state } => {
                if state == bobix_link::relay_frames::PresenceState::Online { if st.online.insert(peer.clone()) { st.home_now.insert(peer); } } else { st.online.remove(&peer); }
            }
            Frame::Error { code, .. } => match code {
                ErrorCode::Invite => st.status = "invite".into(),
                ErrorCode::Update => st.status = "update".into(),
                ErrorCode::RelayUrl => st.status = "url".into(),
                ErrorCode::Auth => st.status = "auth".into(),
                _ => {}
            },
            _ => {}
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

fn on_packet(st: &mut State, pending: &HashMap<String, crate::answers::Pending>, from: &str, packet: &[u8]) -> Vec<Frame> {
    let Some(link) = st.link_of(from) else { return vec![] };
    let opened = match link.open(packet) { Ok(o) => o, Err(_) => return vec![] };
    st.remember_recv(from);
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
            let m = match crate::phone_view::detail(pending, &id, rights) {
                Ok(d) => DesktopMsg::Detail(d),
                Err(code) => DesktopMsg::Error { code: code.into(), who: Some(Who::Desktop) },
            };
            if let Some((_, f)) = st.tell(from, &m, None, None) { out.push(f); }
        }
        PhoneMsg::Sync { waiting, .. } => {
            let results = command::sync_results(&waiting, |cid| st.results.get(cid).map(|r| r.0.clone()));
            for r in results { if let Some((_, f)) = st.tell(from, &DesktopMsg::Result(r), None, None) { out.push(f); } }
        }
        _ => {}
    }
    out
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
    json!({ "on": st.cfg.on, "relay": st.cfg.relay, "status": st.status, "hold": st.cfg.hold, "node": node,
        "deviceId": st.computer_id(), "storage": format!("{:?}", st.storage.unwrap_or(crate::secrets::Storage::Keychain)),
        "phones": st.store.phones.iter().map(|p| json!({ "id": p.id, "name": p.name, "rights": p.rights, "since": p.since, "needsPairing": p.needs_pairing })).collect::<Vec<_>>(),
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
        st.store.phones.push(Phone { id: id.clone(), name: o.name.clone(), auth: b64(&o.phone.auth), boxk: b64(&o.phone.boxk), sign: b64(&o.sign.to_sec1()), rights: r, since: now(), needs_pairing: false });
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

pub fn log(b: &Arc<Bobix>) -> Value { json!(b.core.lock().unwrap().phones.log.iter().rev().collect::<Vec<_>>()) }

// A notice to every paired phone: a held request waits, or a session finished. They follow notifyWaiting, notifyDone and
// Do Not Disturb, and ignore "paused". `waiting` only when a request is held.
pub fn on_notify(b: &Arc<Bobix>, kind: &str, sid: &str) {
    let mut c = b.core.lock().unwrap();
    let crate::Core { phones: st, pending, sessions, settings, .. } = &mut *c;
    if !st.cfg.on || st.store.phones.is_empty() { return; }
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
    let key = if added { "phone.note.added" } else { "phone.note.removed" };
    crate::notify::show(crate::notify::Note { title: crate::i18n::t(&l, "phone.note.title", &[]), body: crate::i18n::t(&l, key, &[("name", name)]), sound, buttons: None, open: crate::i18n::t(&l, "notify.open", &[]) }, |_| {});
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
        let s = Store { phones: vec![Phone { id: "ab".into(), name: "SECRET-NAME".into(), auth: "a".into(), boxk: "b".into(), sign: "c".into(), rights: Rights::default(), since: 1, needs_pairing: false }], revoked: vec![] };
        let bytes = seal_store(&c, &s);
        assert!(!String::from_utf8_lossy(&bytes).contains("SECRET-NAME"), "not readable in the file");
        assert_eq!(open_store(&c, &bytes).unwrap().phones[0].name, "SECRET-NAME");
        let other = XChaCha20Poly1305::new_from_slice(&[8u8; 32]).unwrap();
        assert!(open_store(&other, &bytes).is_none() && open_store(&c, &bytes[..30]).is_none());
        let mut bad = bytes.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(open_store(&c, &bad).is_none(), "a changed file is refused");
    }
}
