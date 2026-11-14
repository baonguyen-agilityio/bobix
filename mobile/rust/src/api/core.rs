//! What the phone app calls in the shared `bobix-link` crate, as plain bytes and strings.
//!
//! No logic of its own: every function forwards to the crate. The command-signing key never passes through here: the
//! secure hardware makes the signature, so there is no function that takes or returns it (the Dart test
//! `rust_api_test.dart` lists every exported function and fails when one is added without being looked at).
//! Binary values are `Vec<u8>` (`Uint8List` in Dart); keys are 32 bytes and a wrong length is `CoreError::InvalidKey`.

use bobix_link::envelope::{self, Direction, OpenError, Update, VersionRange};
use bobix_link::{command, counters, keys, pairing};
use crypto_box::{PublicKey, SecretKey};

/// Why a call failed. A person never sees these names; the app maps them to its own words.
#[derive(Debug, PartialEq, Eq)]
pub enum CoreError {
    /// A key or other fixed-size value has the wrong length or is not acceptable.
    InvalidKey,
    /// The pairing code or a stored state does not parse.
    Malformed,
    /// A relay address that is not `wss://` (or `ws://` on localhost).
    InsecureRelay,
    Expired,
    Mac,
    Hello,
    /// A packet shorter than a nonce and a tag.
    Short,
    /// A packet that did not authenticate (wrong key, tampered).
    Decrypt,
    /// The lane byte of a packet is unknown.
    Lane,
    /// A packet in the receiver's own direction.
    Reflected,
    /// A counter that breaks the rule of its stream (replayed, too old).
    Dropped,
    /// The packet's version is outside what this app understands, and the peer is the one to update.
    PeerMustUpdate,
    /// The packet's version is outside what this app understands, and this app is the one to update.
    WeMustUpdate,
}

impl From<OpenError> for CoreError {
    fn from(e: OpenError) -> Self {
        match e {
            OpenError::Short => CoreError::Short,
            OpenError::Decrypt => CoreError::Decrypt,
            OpenError::Lane => CoreError::Lane,
            OpenError::Reflected => CoreError::Reflected,
            OpenError::Dropped => CoreError::Dropped,
            OpenError::Version(Update::Peer) => CoreError::PeerMustUpdate,
            OpenError::Version(Update::Us) => CoreError::WeMustUpdate,
        }
    }
}

impl From<pairing::PairError> for CoreError {
    fn from(e: pairing::PairError) -> Self {
        match e {
            pairing::PairError::Malformed => CoreError::Malformed,
            pairing::PairError::InsecureRelay => CoreError::InsecureRelay,
            pairing::PairError::Expired => CoreError::Expired,
            pairing::PairError::Mac => CoreError::Mac,
            pairing::PairError::Hello => CoreError::Hello,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkDirection {
    ToPhone,
    ToDesktop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkStream {
    State,
    Event,
    Activity,
}

impl From<LinkDirection> for Direction {
    fn from(d: LinkDirection) -> Self {
        match d {
            LinkDirection::ToPhone => Direction::ToPhone,
            LinkDirection::ToDesktop => Direction::ToDesktop,
        }
    }
}

impl From<Direction> for LinkDirection {
    fn from(d: Direction) -> Self {
        match d {
            Direction::ToPhone => LinkDirection::ToPhone,
            Direction::ToDesktop => LinkDirection::ToDesktop,
        }
    }
}

impl From<LinkStream> for envelope::Stream {
    fn from(s: LinkStream) -> Self {
        match s {
            LinkStream::State => envelope::Stream::State,
            LinkStream::Event => envelope::Stream::Event,
            LinkStream::Activity => envelope::Stream::Activity,
        }
    }
}

impl From<envelope::Stream> for LinkStream {
    fn from(s: envelope::Stream) -> Self {
        match s {
            envelope::Stream::State => LinkStream::State,
            envelope::Stream::Event => LinkStream::Event,
            envelope::Stream::Activity => LinkStream::Activity,
        }
    }
}

fn key32(bytes: &[u8]) -> Result<[u8; 32], CoreError> {
    bytes.try_into().map_err(|_| CoreError::InvalidKey)
}

// ---- the `auth` and `box` keys ----

/// The two device keys the phone keeps in the Keychain (`auth`, `box`) and what is public of them.
pub struct DeviceIdentity {
    /// Ed25519 seed (secret): stored as the `auth` key.
    pub auth_seed: Vec<u8>,
    /// X25519 secret: stored as the `box` key.
    pub box_secret: Vec<u8>,
    pub auth_public: Vec<u8>,
    pub box_public: Vec<u8>,
    pub device_id: String,
}

fn identity_of(id: &keys::Identity) -> DeviceIdentity {
    let p = id.public();
    DeviceIdentity {
        auth_seed: id.auth_seed().to_vec(),
        box_secret: id.box_secret().to_vec(),
        auth_public: p.auth.to_vec(),
        box_public: p.boxk.to_vec(),
        device_id: p.device_id(),
    }
}

/// Makes a new `auth` and `box` key pair from the system's random source.
pub fn generate_identity() -> DeviceIdentity {
    identity_of(&keys::Identity::generate())
}

/// The public half and the id of stored secrets.
pub fn identity_from_secrets(auth_seed: Vec<u8>, box_secret: Vec<u8>) -> Result<DeviceIdentity, CoreError> {
    Ok(identity_of(&keys::Identity::from_secrets(key32(&auth_seed)?, key32(&box_secret)?)))
}

/// The id of a device: the first 16 bytes of the SHA-256 of its `auth` public key, in hex.
pub fn device_id(auth_public: Vec<u8>) -> Result<String, CoreError> {
    Ok(keys::device_id(&key32(&auth_public)?))
}

/// Signs `message` with the `auth` key (the relay's challenge, `bye`).
pub fn sign_with_auth(auth_seed: Vec<u8>, box_secret: Vec<u8>, message: Vec<u8>) -> Result<Vec<u8>, CoreError> {
    Ok(keys::Identity::from_secrets(key32(&auth_seed)?, key32(&box_secret)?).sign_auth(&message).to_vec())
}

pub fn verify_auth_signature(auth_public: Vec<u8>, message: Vec<u8>, signature: Vec<u8>) -> Result<bool, CoreError> {
    Ok(keys::verify_auth(&key32(&auth_public)?, &message, &signature))
}

/// Verifies an ECDSA P-256 signature made by the secure hardware (DER) against the public key (SEC1, compressed or not).
pub fn verify_p256(public_key: Vec<u8>, message: Vec<u8>, signature_der: Vec<u8>) -> bool {
    keys::SignPublic::from_sec1(&public_key).and_then(|k| k.verify(&message, &signature_der)).is_ok()
}

/// A P-256 public key (as the secure hardware exports it) as the 33-byte compressed point the protocol carries.
pub fn compress_sign_public(public_key: Vec<u8>) -> Result<Vec<u8>, CoreError> {
    keys::SignPublic::from_sec1(&public_key).map(|k| k.to_sec1().to_vec()).map_err(|_| CoreError::InvalidKey)
}

// ---- the pairing code ----

/// What a scanned code says. The one-time secret stays in Rust; the keys derived from it are returned.
pub struct PairingInfo {
    pub relay: String,
    pub desktop_auth_public: Vec<u8>,
    pub desktop_box_public: Vec<u8>,
    pub desktop_device_id: String,
    /// Given to the relay so it can match this phone to the open code.
    pub relay_token: Vec<u8>,
    /// Never leaves the two devices: the MAC key of the `hello`.
    pub mac_key: Vec<u8>,
}

pub fn parse_pairing_code(uri: String) -> Result<PairingInfo, CoreError> {
    let code = pairing::PairingCode::parse(&uri)?;
    Ok(PairingInfo {
        relay: code.relay.clone(),
        desktop_auth_public: code.desktop.auth.to_vec(),
        desktop_box_public: code.desktop.boxk.to_vec(),
        desktop_device_id: code.desktop.device_id(),
        relay_token: code.relay_token().to_vec(),
        mac_key: code.mac_key().to_vec(),
    })
}

/// The canonical form of a relay address, or None when it is not acceptable.
pub fn normalize_relay_url(url: String) -> Option<String> {
    pairing::normalize_relay_url(&url)
}

pub fn relay_url_ok(url: String) -> bool {
    pairing::relay_url_ok(&url)
}

// ---- envelopes ----

pub struct PacketHeader {
    pub version: u8,
    pub direction: LinkDirection,
    pub stream: LinkStream,
    pub counter: u64,
}

/// Seals `plaintext` from this device to the other (the nonce carries the version, lane and counter).
pub fn seal_packet(
    version: u8,
    direction: LinkDirection,
    stream: LinkStream,
    counter: u64,
    sender_box_secret: Vec<u8>,
    recipient_box_public: Vec<u8>,
    plaintext: Vec<u8>,
) -> Result<Vec<u8>, CoreError> {
    let sender = SecretKey::from(key32(&sender_box_secret)?);
    let recipient = keys::box_public(key32(&recipient_box_public)?).ok_or(CoreError::InvalidKey)?;
    let lane = envelope::Lane { dir: direction.into(), stream: stream.into() };
    Ok(envelope::seal(version, lane, counter, &sender, &recipient, &plaintext))
}

/// Reads the cleartext part of a packet. `receiver_sends` is the direction this device sends in: a packet that travels in
/// it is `Reflected`.
pub fn peek_packet(receiver_sends: LinkDirection, packet: Vec<u8>) -> Result<PacketHeader, CoreError> {
    let h = envelope::peek(receiver_sends.into(), &packet)?;
    Ok(PacketHeader { version: h.version, direction: h.lane.dir.into(), stream: h.lane.stream.into(), counter: h.counter })
}

pub fn open_packet(receiver_box_secret: Vec<u8>, sender_box_public: Vec<u8>, packet: Vec<u8>) -> Result<Vec<u8>, CoreError> {
    let receiver = SecretKey::from(key32(&receiver_box_secret)?);
    let sender = PublicKey::from(key32(&sender_box_public)?);
    Ok(envelope::open(&receiver, &sender, &packet)?)
}

/// For a packet that authenticated: is its version one this app understands?
pub fn check_version(min: u8, max: u8, version: u8) -> Result<(), CoreError> {
    Ok(envelope::check_version(&VersionRange { min, max }, version)?)
}

// ---- counters ----

/// The receive state of one direction (one window per stream) as JSON, to be stored by the caller.
pub fn recv_state_new() -> String {
    serde_json::to_string(&counters::RecvState::default()).expect("a state serialises")
}

fn recv_state(json: &str) -> Result<counters::RecvState, CoreError> {
    serde_json::from_str(json).map_err(|_| CoreError::Malformed)
}

/// Whether a packet with this counter would be accepted. Nothing changes.
pub fn recv_check(state: String, stream: LinkStream, counter: u64) -> Result<bool, CoreError> {
    Ok(recv_state(&state)?.check(stream.into(), counter))
}

/// The state after accepting this counter (call it only for a packet that opened).
pub fn recv_commit(state: String, stream: LinkStream, counter: u64) -> Result<String, CoreError> {
    let mut s = recv_state(&state)?;
    s.commit(stream.into(), counter);
    Ok(serde_json::to_string(&s).expect("a state serialises"))
}

/// A send counter: hands out counters that never go backwards, even after a restart.
#[flutter_rust_bridge::frb(opaque)]
pub struct SendCounter(counters::SendCounter);

impl SendCounter {
    /// After a restart: `stored_reservation` is the last value `take` asked to store, `now_unix_ms` the clock.
    #[flutter_rust_bridge::frb(sync)]
    pub fn resume(stored_reservation: u64, now_unix_ms: u64) -> SendCounter {
        SendCounter(counters::SendCounter::resume(stored_reservation, now_unix_ms))
    }

    /// The counter to use, and a reservation to store before the packet leaves (when the second is `Some`).
    #[flutter_rust_bridge::frb(sync)]
    pub fn take(&mut self) -> CounterTake {
        let (counter, reserve) = self.0.take();
        CounterTake { counter, reserve }
    }
}

pub struct CounterTake {
    pub counter: u64,
    pub reserve: Option<u64>,
}

// ---- commands ----

/// The bytes the secure hardware signs for a command.
#[allow(clippy::too_many_arguments)]
pub fn command_bytes(
    version: u8,
    phone_id: String,
    computer_id: String,
    kind: String,
    target: String,
    cid: String,
    counter: u64,
    issued_at: u64,
    body: String,
) -> Vec<u8> {
    command::command_bytes(version, &phone_id, &computer_id, &kind, &target, &cid, counter, issued_at, &body)
}

/// The bytes the `auth` key signs when the phone says goodbye to one computer.
pub fn bye_bytes(version: u8, phone_id: String, computer_id: String, counter: u64) -> Vec<u8> {
    command::bye_bytes(version, &phone_id, &computer_id, counter)
}

/// The bytes the `auth` key signs to answer a relay's challenge; None when the relay address is not acceptable.
pub fn auth_bytes(relay_url: String, challenge: String) -> Option<Vec<u8>> {
    command::auth_bytes(&relay_url, &challenge)
}

/// A new command id: 16 random bytes in hex. A command is never sent twice under one id.
pub fn new_command_id() -> String {
    command::new_cid()
}
