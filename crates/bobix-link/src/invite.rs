//! Invite codes: the relay owner's computer signs one with its `auth` key (the key it logs in to the relay with); a
//! tester's computer hands it to the relay over its own authenticated socket, and the relay admits that computer.
//! Nothing in a code is secret: it works once, for one relay, until it expires.
//!
//! Text form: `bobix-invite:` + URL-safe base64 of
//! `version (1) | relay id (1 + n) | owner auth public key (32) | nonce (16) | expires unix ms (8, BE) | label (1 + n) | signature (64)`.
//! The signature is Ed25519 over `Fields("bobix-invite-v1") | version | relay id | owner key | nonce | expires | label`,
//! a domain of its own, so a login signature (`bobix-auth-v2`) is never a valid invite and the reverse.

use crate::keys::{device_id, verify_auth, Identity};
use crate::util::{b64, b64_decode, random, Fields};

pub const PREFIX: &str = "bobix-invite:";
pub const VERSION: u8 = 1;
/// A label is at most this many characters (at most 160 bytes of UTF-8, so its length fits the one-byte prefix).
pub const MAX_LABEL: usize = 40;
pub const MAX_RELAY_ID: usize = 253;
pub const DAY_MS: u64 = 86_400_000;
/// The relay refuses a code that expires further ahead than this.
pub const MAX_VALIDITY_MS: u64 = 30 * DAY_MS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InviteError {
    /// Not a code: wrong prefix, bad base64, wrong lengths, an unknown version.
    Malformed,
    /// The label is too long or holds a control character.
    Label,
    /// The relay id is empty, too long or not printable ASCII.
    RelayId,
    /// The signature does not verify.
    Signature,
}

/// A code whose signature verified (`parse` checks it; there is no unverified value of this type).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InviteCode {
    pub relay_id: String,
    pub owner: [u8; 32],
    pub nonce: [u8; 16],
    pub expires_ms: u64,
    pub label: String,
}

pub fn valid_label(s: &str) -> bool {
    s.chars().count() <= MAX_LABEL && !s.chars().any(char::is_control)
}

fn valid_relay_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= MAX_RELAY_ID && s.bytes().all(|b| b.is_ascii_graphic())
}

fn signed_bytes(relay_id: &str, owner: &[u8; 32], nonce: &[u8; 16], expires_ms: u64, label: &str) -> Vec<u8> {
    Fields::new("bobix-invite-v1")
        .num(VERSION as u64)
        .field(relay_id.as_bytes())
        .field(owner)
        .field(nonce)
        .num(expires_ms)
        .field(label.as_bytes())
        .done()
}

impl InviteCode {
    /// A new code from the owner's identity, for the relay whose `RELAY_ID` is `relay_id`.
    pub fn create(id: &Identity, relay_id: &str, label: &str, expires_ms: u64) -> Result<String, InviteError> {
        let label = label.trim();
        if !valid_relay_id(relay_id) {
            return Err(InviteError::RelayId);
        }
        if !valid_label(label) {
            return Err(InviteError::Label);
        }
        let owner = id.public().auth;
        let nonce = random::<16>();
        let sig = id.sign_auth(&signed_bytes(relay_id, &owner, &nonce, expires_ms, label));
        let mut b = vec![VERSION, relay_id.len() as u8];
        b.extend_from_slice(relay_id.as_bytes());
        b.extend_from_slice(&owner);
        b.extend_from_slice(&nonce);
        b.extend_from_slice(&expires_ms.to_be_bytes());
        b.push(label.len() as u8);
        b.extend_from_slice(label.as_bytes());
        b.extend_from_slice(&sig);
        Ok(format!("{PREFIX}{}", b64(&b)))
    }

    /// Reads a pasted code (spaces and line breaks around and inside it are ignored) and checks its signature. Whether
    /// the owner, the relay and the expiry are acceptable is the relay's question.
    pub fn parse(text: &str) -> Result<InviteCode, InviteError> {
        let text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let body = text.strip_prefix(PREFIX).ok_or(InviteError::Malformed)?;
        let b = b64_decode(body).ok_or(InviteError::Malformed)?;
        let mut r = Reader(&b);
        if r.take(1)?[0] != VERSION {
            return Err(InviteError::Malformed);
        }
        let n = r.take(1)?[0] as usize;
        let relay_id = std::str::from_utf8(r.take(n)?).map_err(|_| InviteError::RelayId)?.to_owned();
        let owner: [u8; 32] = r.take(32)?.try_into().unwrap();
        let nonce: [u8; 16] = r.take(16)?.try_into().unwrap();
        let expires_ms = u64::from_be_bytes(r.take(8)?.try_into().unwrap());
        let n = r.take(1)?[0] as usize;
        let label = std::str::from_utf8(r.take(n)?).map_err(|_| InviteError::Label)?.to_owned();
        let sig = r.take(64)?;
        if !r.0.is_empty() {
            return Err(InviteError::Malformed);
        }
        if !valid_relay_id(&relay_id) {
            return Err(InviteError::RelayId);
        }
        if !valid_label(&label) {
            return Err(InviteError::Label);
        }
        if !verify_auth(&owner, &signed_bytes(&relay_id, &owner, &nonce, expires_ms, &label), sig) {
            return Err(InviteError::Signature);
        }
        Ok(InviteCode { relay_id, owner, nonce, expires_ms, label })
    }

    /// The device id of the computer that signed the code.
    pub fn owner_id(&self) -> String {
        device_id(&self.owner)
    }

    /// The nonce as the relay records it once the code is spent.
    pub fn nonce_key(&self) -> String {
        b64(&self.nonce)
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], InviteError> {
        if self.0.len() < n {
            return Err(InviteError::Malformed);
        }
        let (a, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_round_trips_and_names_its_owner() {
        let owner = Identity::generate();
        let text = InviteCode::create(&owner, "relay.example", " Minh's laptop ", 1_000).unwrap();
        assert!(text.starts_with(PREFIX));
        // A code pasted from a chat may arrive wrapped over lines.
        let wrapped = format!("  {}\n{}  ", &text[..30], &text[30..]);
        let c = InviteCode::parse(&wrapped).unwrap();
        assert_eq!((c.relay_id.as_str(), c.expires_ms, c.label.as_str()), ("relay.example", 1_000, "Minh's laptop"));
        assert_eq!(c.owner_id(), owner.device_id());
        assert_ne!(InviteCode::parse(&InviteCode::create(&owner, "relay.example", "", 1_000).unwrap()).unwrap().nonce, c.nonce, "every code has its own nonce");
    }

    #[test]
    fn a_changed_byte_or_another_key_is_refused() {
        let owner = Identity::generate();
        let text = InviteCode::create(&owner, "relay.example", "x", 5).unwrap();
        let mut b = b64_decode(text.strip_prefix(PREFIX).unwrap()).unwrap();
        for i in [2usize, 20, 40, 60] {
            let mut m = b.clone();
            m[i] ^= 1;
            assert!(InviteCode::parse(&format!("{PREFIX}{}", b64(&m))).is_err(), "byte {i}");
        }
        // The key swapped for another one's: the signature no longer holds.
        let other = Identity::generate().public().auth;
        b[15..47].copy_from_slice(&other);
        assert_eq!(InviteCode::parse(&format!("{PREFIX}{}", b64(&b))), Err(InviteError::Signature));
    }

    #[test]
    fn lengths_prefix_and_version_are_strict() {
        let owner = Identity::generate();
        let text = InviteCode::create(&owner, "r", "", 5).unwrap();
        let b = b64_decode(text.strip_prefix(PREFIX).unwrap()).unwrap();
        let mut longer = b.clone();
        longer.push(0);
        assert_eq!(InviteCode::parse(&format!("{PREFIX}{}", b64(&longer))), Err(InviteError::Malformed), "trailing bytes");
        assert_eq!(InviteCode::parse(&format!("{PREFIX}{}", b64(&b[..b.len() - 1]))), Err(InviteError::Malformed), "cut short");
        let mut v2 = b.clone();
        v2[0] = 2;
        assert_eq!(InviteCode::parse(&format!("{PREFIX}{}", b64(&v2))), Err(InviteError::Malformed), "unknown version");
        assert_eq!(InviteCode::parse(text.strip_prefix(PREFIX).unwrap()), Err(InviteError::Malformed), "no prefix");
        assert_eq!(InviteCode::parse("bobix-invite:!!"), Err(InviteError::Malformed));
        assert_eq!(InviteCode::create(&owner, "r", &"x".repeat(MAX_LABEL + 1), 5), Err(InviteError::Label));
        assert_eq!(InviteCode::create(&owner, "r", "a\u{7}b", 5), Err(InviteError::Label));
        assert!(InviteCode::parse(&InviteCode::create(&owner, "r", &"\u{1F600}".repeat(MAX_LABEL), 5).unwrap()).is_ok(), "the bound is in characters, and 40 of the widest fit");
        assert_eq!(InviteCode::create(&owner, "", "", 5), Err(InviteError::RelayId));
        assert_eq!(InviteCode::create(&owner, "a b", "", 5), Err(InviteError::RelayId));
    }

    #[test]
    fn a_login_signature_is_never_an_invite() {
        // The owner signs a relay's login challenge; a relay that kept it cannot pass it off as an invite.
        let owner = Identity::generate();
        let code = InviteCode::parse(&InviteCode::create(&owner, "relay.example", "", 9).unwrap()).unwrap();
        let login = crate::command::auth_bytes("wss://relay.example", "challenge").unwrap();
        let login_sig = owner.sign_auth(&login);
        let invite = signed_bytes(&code.relay_id, &code.owner, &code.nonce, code.expires_ms, &code.label);
        assert!(!verify_auth(&code.owner, &invite, &login_sig));
    }
}
