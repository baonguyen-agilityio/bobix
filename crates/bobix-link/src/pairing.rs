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
#[derive(Clone, PartialEq, Eq)]
pub struct PairingCode {
    pub relay: String,
    pub desktop: PublicKeys,
    /// 32 random bytes, used once.
    pub secret: [u8; 32],
}

struct RelayUrl {
    scheme: &'static str,
    host: String,
    port: Option<u16>,
    /// Path and query, without fragment, as given.
    tail: String,
    local: bool,
}

/// Splits a relay address; None when it could hide another host (userinfo, backslashes, spaces) or is not `ws(s)://`.
fn parse_relay(url: &str) -> Option<RelayUrl> {
    let (scheme, rest) = url.split_once("://")?;
    let scheme = match scheme.to_ascii_lowercase().as_str() {
        "ws" => "ws",
        "wss" => "wss",
        _ => return None,
    };
    // Control characters (a tab or newline that a URL parser would strip) and non-ASCII hosts are refused outright,
    // so two parsers cannot read one address differently.
    if url.chars().any(char::is_control) {
        return None;
    }
    let rest = rest.split('#').next()?;
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    if authority.is_empty() || authority.contains(['@', '\\', ' ']) || tail.contains(['\\', ' ']) {
        return None;
    }
    let (host, port) = if let Some(r) = authority.strip_prefix('[') {
        let (h, after) = r.split_once(']')?;
        // A bracketed host is an IPv6 address: hex digits, colons, dots.
        if !h.contains(':') || !h.chars().all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.') {
            return None;
        }
        (h, after.strip_prefix(':').or(Some(after).filter(|a| a.is_empty()))?)
    } else {
        authority.rsplit_once(':').unwrap_or((authority, ""))
    };
    // Digits only: `u16::from_str` would take a leading `+`.
    if !port.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let port = if port.is_empty() { None } else { Some(port.parse::<u16>().ok()?) };
    // Outside brackets a host has no `:` (an unbracketed IPv6 address would be misread as host and port) and is ASCII.
    let bracketed = authority.starts_with('[');
    if host.is_empty() || host.contains(['[', ']', '/']) || (!bracketed && (host.contains(':') || !host.is_ascii())) {
        return None;
    }
    let host = host.to_ascii_lowercase();
    let local = matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1");
    Some(RelayUrl { scheme, host, port, tail: tail.to_string(), local })
}

/// Whether a relay address may appear in a pairing code or in `relayMoved`: `wss://`, or plain `ws://` for a relay on
/// localhost only. An address with userinfo (`ws://localhost@evil.example`) is refused.
pub fn relay_url_ok(url: &str) -> bool {
    parse_relay(url).is_some_and(|u| u.scheme == "wss" || u.local)
}

/// The one form of a relay address that both sides sign and compare (see `command::auth_bytes`), so that they agree
/// byte for byte. None when `relay_url_ok` refuses it. Rules:
/// - the scheme and the host are lowercase;
/// - the default port (80 for `ws`, 443 for `wss`) is dropped, any other port is written without leading zeros;
/// - the fragment is dropped; the path keeps its case, and trailing `/` characters are dropped (`/ws/` is `/ws`);
///   a query is kept as given;
/// - a plain `ws://` loopback host (`localhost`, `127.0.0.1`, `[::1]`) is written `localhost`, so a development
///   relay that derives its address from its bind address matches a client that typed `localhost`;
/// - an IPv6 host keeps its brackets. Nothing else is rewritten (no percent-decoding, no punycode).
pub fn normalize_relay_url(url: &str) -> Option<String> {
    let u = parse_relay(url).filter(|u| u.scheme == "wss" || u.local)?;
    let host = if u.scheme == "ws" && u.local { "localhost".to_string() } else if u.host.contains(':') { format!("[{}]", u.host) } else { u.host };
    let port = match (u.scheme, u.port) {
        ("ws", Some(80)) | ("wss", Some(443)) | (_, None) => String::new(),
        (_, Some(p)) => format!(":{p}"),
    };
    let (path, query) = u.tail.split_once('?').map_or((u.tail.as_str(), ""), |(p, q)| (p, q));
    let path = path.trim_end_matches('/');
    let query = if query.is_empty() { String::new() } else { format!("?{query}") };
    Some(format!("{}://{host}{port}{path}{query}", u.scheme))
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

/// The one-time secret is left out: a log line must not carry it.
impl std::fmt::Debug for PairingCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairingCode").field("relay", &self.relay).field("desktop", &self.desktop).field("secret", &"<hidden>").finish()
    }
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
        if crate::keys::is_low_order(&desktop.boxk) {
            return Err(PairError::Malformed);
        }
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
    /// Whether a phone with these rights may send `body`. `stop` denies a held request, so it also needs `answer`.
    pub fn allows(&self, body: &crate::messages::CommandBody) -> bool {
        use crate::messages::CommandBody as C;
        match body {
            C::Answer { .. } => self.answer,
            C::Plan { .. } => self.plan,
            C::Question { .. } => self.question,
            C::Start { .. } => self.start,
            C::Stop { .. } => self.stop && self.answer,
        }
    }

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

/// The bytes the `hello` MAC covers (also in the test vectors).
pub fn hello_mac_input(desktop: &PublicKeys, phone: &PublicKeys, sign: &[u8], name: &str, rights: Rights) -> Vec<u8> {
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
/// needs the key to open the packet; `PairingSession::on_hello` refuses a `hello` whose own (MACed) `boxk` differs.
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
        m.update(&hello_mac_input(&code.desktop, phone, &sign, name, rights));
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
        m.update(&hello_mac_input(&code.desktop, &phone, &sign, &self.name, self.rights));
        m.verify_slice(&mac).map_err(|_| PairError::Mac)?;
        let sign = SignPublic::from_sec1(&sign).map_err(|_| PairError::Hello)?;
        if crate::keys::is_low_order(&boxk) {
            return Err(PairError::Hello);
        }
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

    /// `hello` is what was opened from the pairing body, `outer_box` the key in front of it (the one the packet was
    /// opened with), `from_id` the device id the relay says sent it. A `hello` whose keys do not hash to `from_id`,
    /// whose own `boxk` is not `outer_box`, or whose MAC fails, is `Err` and changes nothing: the desktop keeps one
    /// phone box key.
    pub fn on_hello(&mut self, hello: &Hello, outer_box: &[u8; 32], from_id: &str, now_ms: u64) -> Result<Offer, PairError> {
        if matches!(self.status(now_ms), PairStatus::Expired | PairStatus::Cancelled | PairStatus::Done | PairStatus::Confirmed) {
            return Err(PairError::Expired);
        }
        let (phone, sign) = hello.verify(&self.code)?;
        if phone.device_id() != from_id || &phone.boxk != outer_box {
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
