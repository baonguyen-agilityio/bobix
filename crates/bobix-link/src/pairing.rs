//! Pairing: the QR payload, the `hello` a phone sends, and the desktop's side of one pairing attempt.

use crate::envelope::VersionRange;
use crate::keys::{PublicKeys, SignPublic};
use crate::messages::{is_plain_text, MAX_NAME};
use crate::{b64, b64_decode, random, sha256, Fields};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

/// Only the desktop enforces this: a code older than 2 minutes is not answered.
pub const CODE_LIFETIME_MS: u64 = 2 * 60 * 1000;
/// A pairing the phone did not acknowledge is dropped after this long.
pub const ACK_LIFETIME_MS: u64 = 10 * 60 * 1000;

#[derive(Debug, PartialEq, Eq)]
pub enum PairError {
    Malformed,
    /// `r` must be `wss://`; plain `ws://` only for a relay on localhost (`relay --dev`).
    InsecureRelay,
    Expired,
    /// The MAC does not match: the sender does not hold the one-time secret.
    Mac,
    /// The name, rights or keys of the `hello` are not acceptable.
    Hello,
}

/// `bobix://pair?r=<relay url>&a=<desktop auth key>&b=<desktop box key>&t=<one-time secret>`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairingCode {
    pub relay: String,
    pub desktop: PublicKeys,
    /// 32 random bytes, used once.
    pub secret: [u8; 32],
}

fn is_local(url: &str) -> bool {
    let host = url.strip_prefix("ws://").unwrap_or("");
    let host = host.split(['/', '?']).next().unwrap_or("");
    let host = if let Some(rest) = host.strip_prefix('[') { rest.split(']').next().unwrap_or("") } else { host.rsplit_once(':').map_or(host, |h| h.0) };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

/// Whether a relay address may appear in a pairing code or in `relayMoved`.
pub fn relay_url_ok(url: &str) -> bool {
    url.starts_with("wss://") && url.len() > 6 || is_local(url)
}

fn encode_component(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

fn decode_component(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            out.push(u8::from_str_radix(s.get(i + 1..i + 3)?, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

impl PairingCode {
    pub fn new(relay: &str, desktop: PublicKeys) -> Result<Self, PairError> {
        Self::with_secret(relay, desktop, random::<32>())
    }

    pub fn with_secret(relay: &str, desktop: PublicKeys, secret: [u8; 32]) -> Result<Self, PairError> {
        if !relay_url_ok(relay) {
            return Err(PairError::InsecureRelay);
        }
        Ok(PairingCode { relay: relay.into(), desktop, secret })
    }

    pub fn to_uri(&self) -> String {
        format!(
            "bobix://pair?r={}&a={}&b={}&t={}",
            encode_component(&self.relay),
            b64(&self.desktop.auth),
            b64(&self.desktop.boxk),
            b64(&self.secret)
        )
    }

    pub fn parse(uri: &str) -> Result<Self, PairError> {
        let q = uri.strip_prefix("bobix://pair?").ok_or(PairError::Malformed)?;
        let (mut r, mut a, mut b, mut t) = (None, None, None, None);
        for kv in q.split('&') {
            let (k, v) = kv.split_once('=').ok_or(PairError::Malformed)?;
            let slot = match k {
                "r" => &mut r,
                "a" => &mut a,
                "b" => &mut b,
                "t" => &mut t,
                _ => return Err(PairError::Malformed),
            };
            if slot.replace(v).is_some() {
                return Err(PairError::Malformed);
            }
        }
        let key = |v: Option<&str>| -> Result<[u8; 32], PairError> {
            b64_decode(v.ok_or(PairError::Malformed)?).and_then(|x| x.try_into().ok()).ok_or(PairError::Malformed)
        };
        let relay = decode_component(r.ok_or(PairError::Malformed)?).ok_or(PairError::Malformed)?;
        let desktop = PublicKeys { auth: key(a)?, boxk: key(b)? };
        PairingCode::with_secret(&relay, desktop, key(t)?)
    }

    /// Given to the relay (in `pairSend`) so it can match a phone to an open code.
    pub fn relay_token(&self) -> [u8; 32] {
        sha256(&[b"bobix-pair-relay", &self.secret])
    }

    /// What the desktop gives the relay in `pairOpen`: the relay never learns the token itself until a phone sends it.
    pub fn relay_token_hash(&self) -> [u8; 32] {
        sha256(&[&self.relay_token()])
    }

    /// Never leaves the two devices.
    pub fn mac_key(&self) -> [u8; 32] {
        sha256(&[b"bobix-pair-mac", &self.secret])
    }
}

/// What a phone may do once the computer confirms. The phone asks, the computer confirms each one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rights {
    pub answer: bool,
    pub plan: bool,
    pub question: bool,
    pub start: bool,
    pub stop: bool,
    /// "See session activity": a session's last lines.
    pub activity: bool,
}

impl Rights {
    pub fn to_mask(self) -> u8 {
        [self.answer, self.plan, self.question, self.start, self.stop, self.activity]
            .iter()
            .enumerate()
            .fold(0, |m, (i, on)| m | (*on as u8) << i)
    }
}

/// Sent by the phone, sealed to the desktop's box key (see `body`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    pub auth: String,
    pub boxk: String,
    /// Compressed SEC1 point.
    pub sign: String,
    pub name: String,
    pub rights: Rights,
    pub versions: VersionRange,
    /// HMAC-SHA-256 under the code's `macKey`.
    pub mac: String,
}

fn mac_input(desktop: &PublicKeys, phone: &PublicKeys, sign: &[u8], name: &str, rights: Rights) -> Vec<u8> {
    Fields::new("bobix-hello-v1")
        .field(&desktop.auth)
        .field(&desktop.boxk)
        .field(&phone.auth)
        .field(&phone.boxk)
        .field(sign)
        .field(name.as_bytes())
        .field(&[rights.to_mask()])
        .done()
}

fn hmac(key: &[u8; 32]) -> Hmac<Sha256> {
    Hmac::<Sha256>::new_from_slice(key).expect("HMAC takes a key of any length")
}

/// The first 32 bytes of a pairing body are the phone's box public key; the rest is the sealed packet. The desktop
/// needs the key to open the packet, and the MAC inside covers it.
pub fn hello_body(phone_box_public: &[u8; 32], packet: &[u8]) -> Vec<u8> {
    [phone_box_public.as_slice(), packet].concat()
}

pub fn split_hello_body(body: &[u8]) -> Option<([u8; 32], &[u8])> {
    let (k, rest) = body.split_at_checked(32)?;
    Some((k.try_into().ok()?, rest))
}

impl Hello {
    pub fn new(code: &PairingCode, phone: &PublicKeys, sign: &SignPublic, name: &str, rights: Rights, versions: VersionRange) -> Self {
        let sign = sign.to_sec1();
        let mut m = hmac(&code.mac_key());
        m.update(&mac_input(&code.desktop, phone, &sign, name, rights));
        Hello {
            auth: b64(&phone.auth),
            boxk: b64(&phone.boxk),
            sign: b64(&sign),
            name: name.into(),
            rights,
            versions,
            mac: b64(&m.finalize().into_bytes()),
        }
    }

    /// Checks the MAC first, and only then the contents. Returns the phone's keys.
    pub fn verify(&self, code: &PairingCode) -> Result<(PublicKeys, SignPublic), PairError> {
        let key = |s: &str| -> Option<[u8; 32]> { b64_decode(s)?.try_into().ok() };
        let (Some(auth), Some(boxk), Some(sign), Some(mac)) = (key(&self.auth), key(&self.boxk), b64_decode(&self.sign), b64_decode(&self.mac))
        else {
            return Err(PairError::Malformed);
        };
        let phone = PublicKeys { auth, boxk };
        let mut m = hmac(&code.mac_key());
        m.update(&mac_input(&code.desktop, &phone, &sign, &self.name, self.rights));
        m.verify_slice(&mac).map_err(|_| PairError::Mac)?;
        let sign = SignPublic::from_sec1(&sign).map_err(|_| PairError::Hello)?;
        if self.name.is_empty() || !is_plain_text(&self.name, MAX_NAME) || self.name.contains('\n') {
            return Err(PairError::Hello);
        }
        Ok((phone, sign))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairStatus {
    /// Waiting for a phone.
    Open,
    /// A valid `hello` arrived: the computer shows the phone's name and rights and waits for the owner.
    Offered,
    /// The owner confirmed and `paired` went out; waiting for `pairedAck`.
    Confirmed,
    Done,
    /// A second valid `hello` for one code, or the code is dead.
    Cancelled,
    Expired,
}

/// What a valid `hello` led to.
#[derive(Debug, PartialEq, Eq)]
pub enum Offer {
    /// The first valid `hello`: show this phone.
    New { phone: PublicKeys, sign: SignPublic, name: String, rights: Rights, versions: VersionRange },
    /// The same phone said it again.
    Duplicate,
    /// A second, different phone answered the same code: the pairing is cancelled and the owner is warned.
    Cancelled,
}

/// The desktop's side of one pairing attempt. Time is passed in (Unix milliseconds) so it is testable.
pub struct PairingSession {
    code: PairingCode,
    opened_ms: u64,
    /// When the first valid `hello` arrived; the owner has until `ACK_LIFETIME_MS` after it to confirm.
    offered_ms: Option<u64>,
    confirmed_ms: Option<u64>,
    first: Option<PublicKeys>,
    done: bool,
    cancelled: bool,
}

impl PairingSession {
    pub fn new(code: PairingCode, now_ms: u64) -> Self {
        PairingSession { code, opened_ms: now_ms, offered_ms: None, confirmed_ms: None, first: None, done: false, cancelled: false }
    }

    pub fn code(&self) -> &PairingCode {
        &self.code
    }

    pub fn status(&self, now_ms: u64) -> PairStatus {
        if self.done {
            PairStatus::Done
        } else if self.cancelled {
            PairStatus::Cancelled
        } else if let Some(c) = self.confirmed_ms {
            if now_ms > c + ACK_LIFETIME_MS { PairStatus::Expired } else { PairStatus::Confirmed }
        } else if let Some(o) = self.offered_ms {
            if now_ms > o + ACK_LIFETIME_MS { PairStatus::Expired } else { PairStatus::Offered }
        } else if now_ms > self.opened_ms + CODE_LIFETIME_MS {
            PairStatus::Expired
        } else {
            PairStatus::Open
        }
    }

    /// `hello` is what was opened from the pairing body, `from_id` the device id the relay says sent it. A `hello`
    /// whose keys do not hash to `from_id`, or whose MAC fails, is `Err` and changes nothing.
    pub fn on_hello(&mut self, hello: &Hello, from_id: &str, now_ms: u64) -> Result<Offer, PairError> {
        if matches!(self.status(now_ms), PairStatus::Expired | PairStatus::Cancelled | PairStatus::Done | PairStatus::Confirmed) {
            return Err(PairError::Expired);
        }
        let (phone, sign) = hello.verify(&self.code)?;
        if phone.device_id() != from_id {
            return Err(PairError::Hello);
        }
        match self.first {
            None => {
                self.first = Some(phone);
                self.offered_ms = Some(now_ms);
                Ok(Offer::New { phone, sign, name: hello.name.clone(), rights: hello.rights, versions: hello.versions })
            }
            Some(p) if p == phone => Ok(Offer::Duplicate),
            Some(_) => {
                self.cancelled = true;
                Ok(Offer::Cancelled)
            }
        }
    }

    /// The owner confirmed; the caller sends `paired`.
    pub fn confirm(&mut self, now_ms: u64) -> bool {
        if self.status(now_ms) == PairStatus::Offered {
            self.confirmed_ms = Some(now_ms);
            true
        } else {
            false
        }
    }

    /// `pairedAck` arrived.
    pub fn ack(&mut self, now_ms: u64) -> bool {
        if self.status(now_ms) == PairStatus::Confirmed {
            self.done = true;
            true
        } else {
            false
        }
    }
}
