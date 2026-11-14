//! Everything the relay remembers, behind one lock: who is connected, who may send to whom, the mailboxes, the
//! notices waiting for a sender who is offline, the open pairings and the registered push tokens. Nothing here is
//! held across an await, and nothing here can open a packet.

use crate::config::Config;
use crate::mailbox::{Mailbox, Packet, Put};
use crate::push::{token_kind_for, verify_grant, PushRequest};
use crate::revoked::Revoked;
use bobix_link::relay_frames::{ApnsEnv, ErrorCode, Frame, Platform, PresenceState, Push, TokenKind};
use bobix_link::messages::{Who, MAX_PUSH_BODY};
use bobix_link::{b64, b64_decode, hex, random, sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::mpsc::UnboundedSender;

pub enum Out {
    Frame(Frame),
    Ping,
    Close,
}

struct Conn {
    id: u64,
    tx: UnboundedSender<Out>,
}

struct Pair {
    desktop: String,
    until: Instant,
    used: u8,
}

#[derive(Clone)]
struct TokenReg {
    platform: Platform,
    env: ApnsEnv,
    kind: TokenKind,
    r#ref: Option<String>,
    token: String,
    grant: String,
    expires_ms: u64,
}

/// A push the caller sends after it let go of the lock.
pub struct PushJob {
    pub to: String,
    pub req: PushRequest,
}

/// The list of invited computers, re-read when the file changes.
struct Invites {
    path: std::path::PathBuf,
    seen: Option<SystemTime>,
    ids: HashSet<String>,
}

impl Invites {
    fn contains(&mut self, id: &str) -> bool {
        let modified = std::fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        if modified != self.seen || modified.is_none() {
            self.seen = modified;
            let text = std::fs::read_to_string(&self.path).unwrap_or_default();
            self.ids = text.lines().map(|l| l.split('#').next().unwrap_or("").trim()).filter(|l| !l.is_empty()).map(str::to_owned).collect();
        }
        self.ids.contains(id)
    }
}

pub struct Hub {
    cfg: Config,
    next_conn: u64,
    online: HashMap<String, Conn>,
    asleep: HashSet<String>,
    /// `allow[x]` are the devices that may send to `x`.
    allow: HashMap<String, HashSet<String>>,
    /// Per desktop: every device it ever allowed or that answered one of its pairings. Only these can be revoked.
    known: HashMap<String, HashSet<String>>,
    boxes: HashMap<String, Mailbox>,
    notices: HashMap<String, VecDeque<(Instant, Frame)>>,
    pairs: HashMap<String, Pair>,
    tokens: HashMap<String, Vec<TokenReg>>,
    invites: Option<Invites>,
    pub revoked: Revoked,
}

pub fn now_ms() -> u64 {
    crate::log::now_ms()
}

impl Hub {
    pub fn new(cfg: Config) -> Hub {
        let invites = cfg.invite_file.clone().map(|path| Invites { path, seen: None, ids: HashSet::new() });
        let revoked = Revoked::load(cfg.revoked_dir.clone(), cfg.revoked_days, now_ms());
        Hub {
            cfg,
            next_conn: 0,
            online: HashMap::new(),
            asleep: HashSet::new(),
            allow: HashMap::new(),
            known: HashMap::new(),
            boxes: HashMap::new(),
            notices: HashMap::new(),
            pairs: HashMap::new(),
            tokens: HashMap::new(),
            invites,
            revoked,
        }
    }

    // ---- small helpers -------------------------------------------------------------------------------------------

    fn send_to(&self, id: &str, f: Frame) -> bool {
        self.online.get(id).is_some_and(|c| c.tx.send(Out::Frame(f)).is_ok())
    }

    fn err(&self, to: &str, code: ErrorCode, who: Option<Who>) {
        self.send_to(to, Frame::Error { code, who });
    }

    /// A notice for a sender: now if it is connected, otherwise when it returns (for as long as a packet can live).
    fn notify(&mut self, id: &str, f: Frame) {
        if self.send_to(id, f.clone()) {
            return;
        }
        let q = self.notices.entry(id.to_owned()).or_default();
        if q.len() < 256 {
            q.push_back((Instant::now() + Duration::from_secs(self.cfg.max_ttl_secs as u64), f));
        }
    }

    pub fn state_of(&self, id: &str) -> PresenceState {
        match (self.online.contains_key(id), self.asleep.contains(id)) {
            (_, true) => PresenceState::Asleep,
            (true, false) => PresenceState::Online,
            (false, false) => PresenceState::Offline,
        }
    }

    /// Tells the devices that may send to `id` how it is doing.
    fn announce(&self, id: &str) {
        let state = self.state_of(id);
        for p in self.allow.get(id).into_iter().flatten() {
            self.send_to(p, Frame::Presence { peer: id.to_owned(), state });
        }
    }

    /// With invitations on, a device that is not invited is a guest: it is admitted through an invited desktop.
    pub fn is_guest(&mut self, id: &str) -> bool {
        self.invites.as_mut().is_some_and(|i| !i.contains(id))
    }

    fn invited(&mut self, id: &str) -> bool {
        self.invites.as_mut().map_or(true, |i| i.contains(id))
    }

    // ---- connection life -----------------------------------------------------------------------------------------

    pub fn connect(&mut self, id: &str, tx: UnboundedSender<Out>) -> u64 {
        self.next_conn += 1;
        let conn = self.next_conn;
        if let Some(old) = self.online.insert(id.to_owned(), Conn { id: conn, tx: tx.clone() }) {
            let _ = old.tx.send(Out::Close);
        }
        self.asleep.remove(id);
        self.announce(id);
        // What this device missed: how its peers are, then its notices, then its packets.
        let watching: Vec<String> = self.allow.iter().filter(|(_, a)| a.contains(id)).map(|(y, _)| y.clone()).collect();
        for y in watching {
            self.send_to(id, Frame::Presence { peer: y.clone(), state: self.state_of(&y) });
        }
        let now = Instant::now();
        for (until, f) in self.notices.remove(id).unwrap_or_default() {
            if until > now {
                self.send_to(id, f);
            }
        }
        for p in self.boxes.get(id).into_iter().flat_map(|b| b.iter()) {
            self.send_to(id, Frame::Msg { pid: p.pid.clone(), from: p.from.clone(), body: p.body.clone() });
        }
        conn
    }

    pub fn disconnect(&mut self, id: &str, conn: u64) {
        if self.online.get(id).is_some_and(|c| c.id == conn) {
            self.online.remove(id);
            self.announce(id);
        }
    }

    pub fn online_count(&self) -> usize {
        self.online.len()
    }

    /// Run on a timer: ends packets past their ttl (their senders are told) and forgets what expired.
    pub fn sweep(&mut self) {
        let now = Instant::now();
        let mut dropped = Vec::new();
        for b in self.boxes.values_mut() {
            dropped.extend(b.drain_where(|p| p.expires <= now));
        }
        self.boxes.retain(|_, b| !b.is_empty());
        for p in dropped {
            self.notify(&p.from.clone(), Frame::Dropped { pid: p.pid });
        }
        for q in self.notices.values_mut() {
            q.retain(|(until, _)| *until > now);
        }
        self.notices.retain(|_, q| !q.is_empty());
        self.pairs.retain(|_, p| p.until > now);
        self.revoked.prune(now_ms());
    }

    // ---- frames from a device ------------------------------------------------------------------------------------

    pub fn frame(&mut self, me: &str, guest: bool, f: Frame) -> Vec<PushJob> {
        let mut jobs = Vec::new();
        // A guest may only do what a phone does: say who may send to it, answer a pairing, acknowledge, withdraw,
        // send to a device that allowed it, and register a push token.
        let desktop_only = matches!(f, Frame::PairOpen { .. } | Frame::Revoke { .. } | Frame::State { .. });
        if guest && desktop_only {
            self.err(me, ErrorCode::Invite, None);
            return jobs;
        }
        match f {
            Frame::Peers { allow } => self.peers(me, guest, allow),
            Frame::Revoke { id } => self.revoke(me, &id),
            Frame::State { asleep } => {
                if asleep { self.asleep.insert(me.to_owned()); } else { self.asleep.remove(me); }
                self.announce(me);
            }
            Frame::Send { pid, to, ttl, collapse, push, body } => jobs = self.send(me, pid, to, ttl, collapse, push, body),
            Frame::Ack { pid } => {
                if let Some(p) = self.boxes.get_mut(me).and_then(|b| b.ack(&pid)) {
                    self.notify(&p.from, Frame::Delivered { pid });
                }
            }
            Frame::Withdraw { pid } => {
                let found = self.boxes.values_mut().any(|b| b.withdraw(me, &pid).is_some());
                self.send_to(me, if found { Frame::Withdrawn { pid } } else { Frame::TooLate { pid } });
            }
            Frame::PairOpen { relay_token_hash, ttl } => self.pair_open(me, relay_token_hash, ttl),
            Frame::PairSend { relay_token, body } => self.pair_send(me, &relay_token, body),
            Frame::PushToken { platform, env, kind, r#ref, token, grant } => {
                self.push_token(me, TokenReg { platform, env, kind, r#ref, token, grant, expires_ms: 0 })
            }
            _ => self.err(me, ErrorCode::Malformed, None),
        }
        jobs
    }

    fn peers(&mut self, me: &str, guest: bool, allow: Vec<String>) {
        let allow: HashSet<String> = allow.into_iter().filter(|a| a != me).take(256).collect();
        if guest && !allow.iter().any(|a| self.invited(a)) {
            return self.err(me, ErrorCode::Invite, None);
        }
        let old = self.allow.insert(me.to_owned(), allow.clone()).unwrap_or_default();
        self.known.entry(me.to_owned()).or_default().extend(allow.iter().cloned());
        // Delivery from an id that was dropped stops at once, also for what it already queued.
        let gone = self.boxes.get_mut(me).map(|b| b.drain_where(|p| !allow.contains(&p.from))).unwrap_or_default();
        for p in gone {
            self.notify(&p.from.clone(), Frame::Dropped { pid: p.pid });
        }
        for n in allow.difference(&old) {
            self.send_to(n, Frame::Presence { peer: me.to_owned(), state: self.state_of(me) });
        }
    }

    fn revoke(&mut self, me: &str, id: &str) {
        if !self.known.get(me).is_some_and(|k| k.contains(id)) {
            return self.err(me, ErrorCode::NoPeer, None);
        }
        self.revoked.add(me, id, now_ms());
        if let Some(a) = self.allow.get_mut(me) {
            a.remove(id);
        }
        if let Some(b) = self.boxes.get_mut(me) {
            b.drain_where(|p| p.from == id);
        }
        if let Some(c) = self.online.get(id) {
            let _ = c.tx.send(Out::Frame(Frame::Error { code: ErrorCode::Revoked, who: None }));
            let _ = c.tx.send(Out::Close);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn send(&mut self, me: &str, pid: String, to: String, ttl: u32, collapse: Option<String>, push: Option<Push>, body: String) -> Vec<PushJob> {
        // A packet to oneself would be a reflection; a packet to a device that did not allow the sender is refused.
        if to == me || !self.allow.get(&to).is_some_and(|a| a.contains(me)) {
            self.err(me, ErrorCode::NoPeer, None);
            self.send_to(me, Frame::Dropped { pid });
            return vec![];
        }
        let ttl = ttl.clamp(1, self.cfg.max_ttl_secs);
        let packet = Packet { pid: pid.clone(), from: me.to_owned(), body: body.clone(), expires: Instant::now() + Duration::from_secs(ttl as u64), collapse };
        let (mp, mb) = (self.cfg.mailbox_packets, self.cfg.mailbox_bytes);
        match self.boxes.entry(to.clone()).or_insert_with(|| Mailbox::new(mp, mb)).put(packet) {
            Put::Refused => {
                self.send_to(me, Frame::Dropped { pid });
                vec![]
            }
            Put::Again => {
                self.send_to(me, Frame::Queued { pid });
                vec![]
            }
            Put::Queued { replaced } => {
                for old in replaced {
                    self.send_to(me, Frame::Dropped { pid: old });
                }
                self.send_to(me, Frame::Queued { pid: pid.clone() });
                self.send_to(&to, Frame::Msg { pid, from: me.to_owned(), body: body.clone() });
                self.push_for(me, &to, push, body)
            }
        }
    }

    // ---- pairing -------------------------------------------------------------------------------------------------

    fn pair_open(&mut self, me: &str, hash: String, ttl: u32) {
        let now = Instant::now();
        self.pairs.retain(|_, p| p.until > now);
        let open = self.pairs.values().filter(|p| p.desktop == me).count();
        if open >= self.cfg.max_open_pairs {
            return self.err(me, ErrorCode::Other, Some(Who::Relay));
        }
        let ttl = ttl.clamp(1, self.cfg.pair_ttl_secs) as u64;
        self.pairs.insert(hash, Pair { desktop: me.to_owned(), until: now + Duration::from_secs(ttl), used: 0 });
    }

    fn pair_send(&mut self, me: &str, token: &str, body: String) {
        let hash = b64_decode(token).map(|t| b64(&sha256(&[&t]))).unwrap_or_default();
        let Some(p) = self.pairs.get_mut(&hash) else { return self.err(me, ErrorCode::NoPeer, None) };
        if p.until <= Instant::now() || p.used >= self.cfg.max_pair_uses {
            return self.err(me, ErrorCode::PairExpired, None);
        }
        let desktop = p.desktop.clone();
        if !self.online.contains_key(&desktop) {
            return self.err(me, ErrorCode::NoPeer, None);
        }
        self.pairs.get_mut(&hash).expect("checked above").used += 1;
        self.known.entry(desktop.clone()).or_default().insert(me.to_owned());
        self.send_to(&desktop, Frame::Msg { pid: hex(&random::<8>()), from: me.to_owned(), body });
    }

    // ---- push ----------------------------------------------------------------------------------------------------

    fn push_token(&mut self, me: &str, mut reg: TokenReg) {
        let relay_id = self.cfg.relay_id.clone();
        let own_key = match verify_grant(&reg.grant, &reg.token, &relay_id, now_ms()) {
            Ok((signer, exp)) if signer == me && !reg.token.is_empty() && reg.token.len() <= 512 => {
                reg.expires_ms = exp;
                true
            }
            _ => false,
        };
        if !own_key {
            return self.err(me, ErrorCode::Grant, Some(Who::Phone));
        }
        let list = self.tokens.entry(me.to_owned()).or_default();
        list.retain(|t| !(t.kind == reg.kind && t.r#ref == reg.r#ref));
        if list.len() < 8 {
            list.push(reg);
        }
    }

    fn push_for(&mut self, from: &str, to: &str, push: Option<Push>, body: String) -> Vec<PushJob> {
        let Some(push) = push else { return vec![] };
        if body.len() > MAX_PUSH_BODY {
            return vec![]; // too big for a push; it still reaches the phone over its socket
        }
        let kind = token_kind_for(push.kind);
        let now = now_ms();
        let Some(reg) = self.tokens.get(to).and_then(|l| l.iter().rev().find(|t| t.kind == kind)).cloned() else { return vec![] };
        if reg.expires_ms <= now {
            // The grant ended: say so to the sender (the error follows the `queued` of this very packet).
            self.err(from, ErrorCode::Grant, Some(Who::Phone));
            return vec![];
        }
        let req = PushRequest {
            platform: reg.platform,
            env: reg.env,
            kind: push.kind,
            token: reg.token,
            priority: push.priority,
            replaces: push.replaces,
            body,
            grant: reg.grant,
        };
        vec![PushJob { to: to.to_owned(), req }]
    }

    /// The push service said this token is dead.
    pub fn forget_token(&mut self, id: &str, token: &str) {
        if let Some(l) = self.tokens.get_mut(id) {
            l.retain(|t| t.token != token);
        }
    }
}
