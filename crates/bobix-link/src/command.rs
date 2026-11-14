//! Signed commands (phone → desktop).
//!
//! The phone signs, in its secure hardware, ECDSA P-256 with SHA-256, DER, over (every field prefixed with its
//! length, see `Fields`): `"bobix-cmd-v1"`, version, phone id, computer id, kind, target, cid, counter, `issuedAt`,
//! SHA-256 of the `body` string exactly as it travels. The body is a JSON text inside a string, so no re-encoding
//! can change the bytes that were hashed. This crate builds the bytes and verifies; it never holds the key.

use crate::keys::SignPublic;
use crate::messages::{CmdResult, CommandBody, Outcome, COMMAND_MAX_AGE_MS, COMMAND_MAX_AHEAD_MS};
use crate::{b64, b64_decode, sha256, unhex, Fields};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Command {
    /// 128 random bits, in hex. A repeated `cid` gets the stored result back without acting again.
    pub cid: String,
    /// The device id of the computer the phone meant.
    pub computer: String,
    /// Unix milliseconds on the phone's clock.
    pub issued_at: u64,
    /// DER signature, URL-safe base64.
    pub sig: String,
    /// A JSON text of a `CommandBody`.
    pub body: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The `cid`, the body or the signature is not in the documented encoding.
    Encoding,
    /// The command is for another computer.
    Computer,
    /// The signature does not match the phone's key and these bytes.
    Signature,
    /// `issuedAt` is more than 10 minutes behind or 2 minutes ahead of the receiver's clock.
    Clock,
}

impl Refusal {
    /// The `why` of the `refused` result.
    pub fn why(self) -> &'static str {
        match self {
            Refusal::Encoding => "encoding",
            Refusal::Computer => "computer",
            Refusal::Signature => "signature",
            Refusal::Clock => "clock",
        }
    }
}

pub fn new_cid() -> String {
    crate::hex(&crate::random::<16>())
}

/// The bytes the phone signs.
#[allow(clippy::too_many_arguments)]
pub fn command_bytes(
    version: u8,
    phone_id: &str,
    computer_id: &str,
    kind: &str,
    target: &str,
    cid: &str,
    counter: u64,
    issued_at: u64,
    body: &str,
) -> Vec<u8> {
    Fields::new("bobix-cmd-v1")
        .num(version as u64)
        .field(phone_id.as_bytes())
        .field(computer_id.as_bytes())
        .field(kind.as_bytes())
        .field(target.as_bytes())
        .field(cid.as_bytes())
        .num(counter)
        .num(issued_at)
        .field(&sha256(&[body.as_bytes()]))
        .done()
}

/// What `bye` signs with the `auth` key: the phone says goodbye to one computer.
pub fn bye_bytes(version: u8, phone_id: &str, computer_id: &str, counter: u64) -> Vec<u8> {
    Fields::new("bobix-bye-v1").num(version as u64).field(phone_id.as_bytes()).field(computer_id.as_bytes()).num(counter).done()
}

/// What the relay's `auth` frame signs: `"bobix-auth-v2" | normalised relay URL | challenge`, so a signature made
/// for one relay is useless at another (a hostile relay cannot pass its challenge on to log in elsewhere as the
/// device). `relay_url` is the address the device is connected to (`pairing::normalize_relay_url` is applied here);
/// the relay passes its own configured public URL. None when the URL is not an acceptable relay address.
pub fn auth_bytes(relay_url: &str, challenge: &str) -> Option<Vec<u8>> {
    let url = crate::pairing::normalize_relay_url(relay_url)?;
    Some(Fields::new("bobix-auth-v2").field(url.as_bytes()).field(challenge.as_bytes()).done())
}

/// Why a relay refuses an `auth` frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthRefusal {
    /// The key or signature is malformed, the id is not the hash of the key, or the signature does not verify.
    Invalid,
    /// The signature is valid, but for another relay's address: the relay's configured public URL is wrong, or the
    /// device is talking to a relay it was not meant to.
    OtherRelay,
}

impl AuthRefusal {
    pub fn code(self) -> crate::relay_frames::ErrorCode {
        match self {
            AuthRefusal::Invalid => crate::relay_frames::ErrorCode::Auth,
            AuthRefusal::OtherRelay => crate::relay_frames::ErrorCode::RelayUrl,
        }
    }
}

/// What a relay calls on an `auth` frame: `own_public_url` is its configured public address, `challenge` the nonce it
/// sent, the rest are the frame's fields. Returns the device's `auth` public key.
pub fn verify_auth_frame(
    own_public_url: &str,
    challenge: &str,
    id: &str,
    key: &str,
    claimed_relay: &str,
    sig: &str,
) -> Result<[u8; 32], AuthRefusal> {
    let key: [u8; 32] = b64_decode(key).and_then(|k| k.try_into().ok()).ok_or(AuthRefusal::Invalid)?;
    let sig = b64_decode(sig).ok_or(AuthRefusal::Invalid)?;
    if crate::keys::device_id(&key) != id {
        return Err(AuthRefusal::Invalid);
    }
    // The signature must hold for the address the device says it signed; only then is that address believed.
    let bytes = auth_bytes(claimed_relay, challenge).ok_or(AuthRefusal::Invalid)?;
    if !crate::keys::verify_auth(&key, &bytes, &sig) {
        return Err(AuthRefusal::Invalid);
    }
    let own = crate::pairing::normalize_relay_url(own_public_url);
    if own != crate::pairing::normalize_relay_url(claimed_relay) {
        return Err(AuthRefusal::OtherRelay);
    }
    Ok(key)
}

/// A device's `auth` frame for the relay it is connected to at `relay_url` (the address from its settings or from the
/// pairing code). None when the URL is not an acceptable relay address.
pub fn auth_frame(
    id: &crate::keys::Identity,
    relay_url: &str,
    challenge: &str,
    versions: crate::envelope::VersionRange,
) -> Option<crate::relay_frames::Frame> {
    let sig = id.sign_auth(&auth_bytes(relay_url, challenge)?);
    Some(crate::relay_frames::Frame::Auth {
        id: id.device_id(),
        key: b64(&id.public().auth),
        relay: relay_url.into(),
        sig: b64(&sig),
        versions,
    })
}

/// Builds a command. `counter` is the packet counter the command will travel with (take it from
/// `Link::reserve_counter`); `sign` is the phone's signer (secure hardware, or a software key in tests).
#[allow(clippy::too_many_arguments)]
pub fn build(
    version: u8,
    phone_id: &str,
    computer_id: &str,
    counter: u64,
    now_ms: u64,
    body: &CommandBody,
    sign: impl FnOnce(&[u8]) -> Vec<u8>,
) -> Command {
    build_with_cid(new_cid(), version, phone_id, computer_id, counter, now_ms, body, sign)
}

/// `build` with the `cid` given. For test vectors; a real `cid` is always fresh.
#[allow(clippy::too_many_arguments)]
pub fn build_with_cid(
    cid: String,
    version: u8,
    phone_id: &str,
    computer_id: &str,
    counter: u64,
    now_ms: u64,
    body: &CommandBody,
    sign: impl FnOnce(&[u8]) -> Vec<u8>,
) -> Command {
    let body_text = serde_json::to_string(body).expect("a command body always serializes");
    let bytes = command_bytes(version, phone_id, computer_id, body.kind(), &body.target(), &cid, counter, now_ms, &body_text);
    Command { cid, computer: computer_id.into(), issued_at: now_ms, sig: b64(&sign(&bytes)), body: body_text }
}

/// What the desktop knows about the packet a command arrived in.
pub struct VerifyCtx<'a> {
    pub version: u8,
    pub phone_id: &'a str,
    pub computer_id: &'a str,
    pub sign_key: &'a SignPublic,
    /// The packet counter the command arrived with.
    pub counter: u64,
    pub now_ms: u64,
}

/// Checks a command and returns its body. Order: encoding, computer, signature, clock.
pub fn verify(ctx: &VerifyCtx, cmd: &Command) -> Result<CommandBody, Refusal> {
    if cmd.cid.len() != 32 || unhex(&cmd.cid).is_none() {
        return Err(Refusal::Encoding);
    }
    let body: CommandBody = serde_json::from_str(&cmd.body).map_err(|_| Refusal::Encoding)?;
    body.validate().map_err(|_| Refusal::Encoding)?;
    if cmd.computer != ctx.computer_id {
        return Err(Refusal::Computer);
    }
    let sig = b64_decode(&cmd.sig).ok_or(Refusal::Encoding)?;
    let bytes = command_bytes(
        ctx.version, ctx.phone_id, ctx.computer_id, body.kind(), &body.target(), &cmd.cid, ctx.counter, cmd.issued_at, &cmd.body,
    );
    ctx.sign_key.verify(&bytes, &sig).map_err(|e| match e {
        crate::keys::SignError::Encoding => Refusal::Encoding,
        crate::keys::SignError::Invalid => Refusal::Signature,
    })?;
    let now = ctx.now_ms;
    if cmd.issued_at.saturating_add(COMMAND_MAX_AGE_MS) < now || cmd.issued_at > now.saturating_add(COMMAND_MAX_AHEAD_MS) {
        return Err(Refusal::Clock);
    }
    Ok(body)
}

/// False when what the phone says it was shown differs from what the desktop holds now: the answer is `stale`.
pub fn shown_matches(held: &crate::messages::Detail, shown: &str) -> bool {
    held.shown_now() == shown
}

/// The desktop's reply to `sync { waiting }`: the stored result of each command it knows, and `unknown` for the
/// others (never received; they may still arrive within 10 minutes, or be withdrawn).
pub fn sync_results(waiting: &[String], stored: impl Fn(&str) -> Option<CmdResult>) -> Vec<CmdResult> {
    waiting.iter().map(|cid| stored(cid).unwrap_or_else(|| CmdResult::new(cid, Outcome::Unknown))).collect()
}
