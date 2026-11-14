//! The fake phone: pairs from a QR string, prints what it receives, sends signed commands as a real phone would
//! (signing with a software P-256 key). It may be paired with several computers.

use super::{new_link, now_ms, pid, Conn, Recv};
use bobix_link::command;
use bobix_link::envelope::{Direction, OpenError, Stream, CURRENT_VERSION, VersionRange};
use bobix_link::keys::{soft::SoftSigner, Identity};
use bobix_link::link::Link;
use bobix_link::messages::{CommandBody, DesktopMsg, PhoneMsg};
use bobix_link::pairing::{hello_body, Hello, PairingCode, Rights};
use bobix_link::relay_frames::{ErrorCode, Frame};
use bobix_link::{b64, b64_decode};
use std::collections::HashMap;
use std::time::Duration;

pub struct Phone {
    pub name: String,
    /// The relay address this phone is connected to (and signs in its `auth` frame).
    pub relay: String,
    pub id: Identity,
    pub signer: SoftSigner,
    conn: Conn,
    links: HashMap<String, Link>,
    /// Everything opened so far: the computer's id and the message.
    pub log: Vec<(String, DesktopMsg)>,
    /// Relay frames that are not packets: presence, delivered, errors.
    pub frames: Vec<Frame>,
    /// Packets that were dropped or refused, and why. A drop never ends a pairing.
    pub drops: Vec<OpenError>,
    pub print: bool,
    /// Everything this phone would print.
    pub lines: Vec<String>,
}

impl Phone {
    pub async fn connect(relay: &str, name: &str) -> Result<Phone, String> {
        let mut conn = Conn::connect(relay).await?;
        let id = Identity::generate();
        conn.login(&id).await?;
        Ok(Phone { name: name.into(), relay: relay.into(), id, signer: SoftSigner::generate(), conn, links: HashMap::new(), log: vec![], frames: vec![], drops: vec![], print: false, lines: vec![] })
    }

    fn say(&mut self, line: String) {
        if self.print {
            println!("{line}");
        }
        self.lines.push(line);
    }

    /// Reads one frame, handles it, and says whether anything arrived.
    pub async fn pump(&mut self, wait: Duration) -> bool {
        let Recv::Frame(f) = self.conn.recv(wait).await else { return false };
        match f {
            Frame::Msg { pid, from, body } => {
                let _ = self.conn.send(&Frame::Ack { pid }).await;
                let Some(link) = self.links.get_mut(&from) else {
                    self.say(format!("packet from {from}, a computer this phone is not paired with: ignored"));
                    return true;
                };
                match b64_decode(&body).ok_or(OpenError::Short).and_then(|p| link.open(&p)) {
                    Ok(o) => match DesktopMsg::from_json(&o.plaintext) {
                        Ok(m) => {
                            self.say(format!("{from} {:?} #{}: {}", o.stream, o.counter, String::from_utf8_lossy(&o.plaintext)));
                            self.log.push((from, m));
                        }
                        Err(e) => self.say(format!("unreadable message: {e}")),
                    },
                    Err(e) => {
                        self.say(format!("packet dropped: {e:?}"));
                        self.drops.push(e);
                    }
                }
            }
            other => {
                self.say(format!("relay: {}", other.to_text().unwrap_or_default()));
                self.frames.push(other);
            }
        }
        true
    }

    /// Pumps until a received desktop message satisfies `want`, or `wait` runs out.
    pub async fn wait_for(&mut self, wait: Duration, want: impl Fn(&DesktopMsg) -> bool) -> Option<DesktopMsg> {
        let end = tokio::time::Instant::now() + wait;
        let mut seen = 0;
        loop {
            if let Some((_, m)) = self.log[seen.min(self.log.len())..].iter().find(|(_, m)| want(m)) {
                return Some(m.clone());
            }
            seen = self.log.len();
            let left = end.saturating_duration_since(tokio::time::Instant::now());
            if left.is_zero() || !self.pump(left).await {
                return None;
            }
        }
    }

    /// Pumps until nothing arrives for `quiet`.
    pub async fn settle(&mut self, quiet: Duration) {
        while self.pump(quiet).await {}
    }

    /// Pairs from a QR string. Returns the computer's device id.
    pub async fn pair(&mut self, qr: &str, rights: Rights) -> Result<String, String> {
        let code = PairingCode::parse(qr).map_err(|e| format!("this QR code cannot be used: {e:?}"))?;
        if bobix_link::pairing::normalize_relay_url(&code.relay) != bobix_link::pairing::normalize_relay_url(&self.relay) {
            return Err(format!("this code is for another relay ({}), this phone is connected to {}", code.relay, self.relay));
        }
        let computer = code.desktop.device_id();
        let mut link = new_link(Direction::ToDesktop, &self.id, code.desktop.boxk);
        let hello = Hello::new(&code, &self.id.public(), &self.signer.public(), &self.name, rights, VersionRange::current());
        let packet = link.seal(Stream::Event, &PhoneMsg::Hello(hello).to_json()).expect("a peer key of low order was refused earlier").packet;
        let body = hello_body(&self.id.public().boxk, &packet);
        self.links.insert(computer.clone(), link);
        self.conn.send(&Frame::Peers { allow: self.links.keys().cloned().collect() }).await?;
        self.conn.send(&Frame::PairSend { relay_token: b64(&code.relay_token()), body: b64(&body) }).await?;
        let end = tokio::time::Instant::now() + Duration::from_secs(5);
        let paired = loop {
            let found = self.log.iter().find(|(c, m)| *c == computer && matches!(m, DesktopMsg::Paired { .. } | DesktopMsg::Revoked { .. }));
            let refused = self.frames.iter().any(|f| matches!(f, Frame::Error { code: ErrorCode::PairExpired | ErrorCode::NoPeer, .. }));
            if found.is_some() || refused || tokio::time::Instant::now() > end {
                break found.map(|(_, m)| m.clone());
            }
            self.pump(Duration::from_millis(100)).await;
        };
        match paired {
            Some(DesktopMsg::Paired { .. }) => {
                self.send(&computer, &PhoneMsg::PairedAck).await?;
                Ok(computer)
            }
            Some(DesktopMsg::Revoked { why }) => {
                self.links.remove(&computer);
                Err(why)
            }
            _ => {
                self.links.remove(&computer);
                let expired = self.frames.iter().any(|f| matches!(f, Frame::Error { code: ErrorCode::PairExpired, .. }));
                Err(if expired { "the pairing code expired".into() } else { "the computer did not answer".into() })
            }
        }
    }

    pub async fn send(&mut self, computer: &str, m: &PhoneMsg) -> Result<(), String> {
        let link = self.links.get_mut(computer).ok_or("not paired with that computer")?;
        let sealed = link.seal(Stream::Event, &m.to_json()).expect("a peer key of low order was refused earlier");
        self.conn
            .send(&Frame::Send { pid: pid(), to: computer.into(), ttl: 600, collapse: None, push: None, body: b64(&sealed.packet) })
            .await
    }

    /// Sends a signed command to `computer`. Returns its `cid`.
    pub async fn command(&mut self, computer: &str, body: CommandBody) -> Result<String, String> {
        self.command_addressed(computer, computer, body).await
    }

    /// Sends through the link to `via` a command signed for `addressed`. For testing that a computer refuses a
    /// command that was meant for another one.
    pub async fn command_addressed(&mut self, via: &str, addressed: &str, body: CommandBody) -> Result<String, String> {
        let link = self.links.get_mut(via).ok_or("not paired with that computer")?;
        let (counter, _) = link.reserve_counter(Stream::Event);
        let cmd = command::build(CURRENT_VERSION, &self.id.device_id(), addressed, counter, now_ms(), &body, |m| self.signer.sign(m));
        let cid = cmd.cid.clone();
        let packet = link.seal_with(Stream::Event, counter, &PhoneMsg::Cmd(cmd).to_json()).expect("a peer key of low order was refused earlier");
        self.conn
            .send(&Frame::Send { pid: pid(), to: via.into(), ttl: 600, collapse: None, push: None, body: b64(&packet) })
            .await?;
        Ok(cid)
    }

    /// Sends a command with a clock `skew_ms` off (negative: in the past).
    pub async fn command_at(&mut self, computer: &str, body: CommandBody, skew_ms: i64) -> Result<String, String> {
        let link = self.links.get_mut(computer).ok_or("not paired with that computer")?;
        let (counter, _) = link.reserve_counter(Stream::Event);
        let at = (now_ms() as i64 + skew_ms) as u64;
        let cmd = command::build(CURRENT_VERSION, &self.id.device_id(), computer, counter, at, &body, |m| self.signer.sign(m));
        let cid = cmd.cid.clone();
        let packet = link.seal_with(Stream::Event, counter, &PhoneMsg::Cmd(cmd).to_json()).expect("a peer key of low order was refused earlier");
        self.conn
            .send(&Frame::Send { pid: pid(), to: computer.into(), ttl: 600, collapse: None, push: None, body: b64(&packet) })
            .await?;
        Ok(cid)
    }

    /// The `shown` hash of the latest `detail` this phone received for `id`.
    pub fn shown_for(&self, id: &str) -> Option<String> {
        self.log.iter().rev().find_map(|(_, m)| match m {
            DesktopMsg::Detail(d) if d.id == id => Some(d.shown.clone()),
            _ => None,
        })
    }

    /// The prompt hash of the latest `startPrompt` received for a ticket.
    pub fn prompt_hash_for(&self, project: &str, iid: &str) -> Option<String> {
        self.log.iter().rev().find_map(|(_, m)| match m {
            DesktopMsg::StartPrompt(p) if p.project == project && p.iid == iid => Some(p.prompt_hash.clone()),
            _ => None,
        })
    }

    pub async fn bye(&mut self, computer: &str) -> Result<(), String> {
        let link = self.links.get_mut(computer).ok_or("not paired with that computer")?;
        let (counter, _) = link.reserve_counter(Stream::Event);
        let sig = self.id.sign_auth(&command::bye_bytes(CURRENT_VERSION, &self.id.device_id(), computer, counter));
        let packet = link.seal_with(Stream::Event, counter, &PhoneMsg::Bye { sig: b64(&sig) }.to_json()).expect("a peer key of low order was refused earlier");
        self.conn
            .send(&Frame::Send { pid: pid(), to: computer.into(), ttl: 600, collapse: None, push: None, body: b64(&packet) })
            .await
    }
}
