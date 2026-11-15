//! The phone's push core, shared by the app and the iOS notification service extension (and, later, the Android push
//! service) through UniFFI. It adds no protocol of its own: every check is a call into `bobix-link`. What it adds is
//! the two-process part: the receive state lives in one file that both processes change under an exclusive lock.

mod store;

use bobix_link::counters::RecvState;
use bobix_link::envelope::{Direction, OpenError, Stream, VersionRange};
use bobix_link::keys::Identity;
use bobix_link::link::Link;
use bobix_link::messages::{DesktopMsg, NoticeKind};
use bobix_link::{b64, b64_decode, Fields};
use std::path::Path;
use store::{with_doc, Computer, StoreError};

uniffi::setup_scaffolding!();

/// Why nothing was opened. A caller shows the generic text for every one of them and changes nothing else.
#[derive(Debug, uniffi::Error)]
pub enum PushError {
    /// The packet authenticated for no computer, or is not a packet at all.
    Unreadable,
    /// The packet's counter was already seen (a replay, or a copy the app already took) or is older than the window.
    Replayed,
    /// The packet authenticated but its protocol version is outside the pair's range.
    Version,
    /// It opened but is not a notice (or breaks a limit). The receive state is left as it was.
    NotANotice,
    /// The input was malformed (a key of the wrong length, an unknown value).
    Invalid { message: String },
    /// The shared file could not be read or written.
    Store { message: String },
}

impl std::fmt::Display for PushError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PushError::Unreadable => write!(f, "unreadable"),
            PushError::Replayed => write!(f, "replayed"),
            PushError::Version => write!(f, "version"),
            PushError::NotANotice => write!(f, "not a notice"),
            PushError::Invalid { message } => write!(f, "invalid: {message}"),
            PushError::Store { message } => write!(f, "store: {message}"),
        }
    }
}

impl std::error::Error for PushError {}

impl From<StoreError> for PushError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::Io(m) | StoreError::Corrupt(m) => PushError::Store { message: m },
        }
    }
}

fn invalid(message: &str) -> PushError {
    PushError::Invalid { message: message.into() }
}

fn key32(v: &[u8], what: &str) -> Result<[u8; 32], PushError> {
    v.try_into().map_err(|_| invalid(&format!("{what} must be 32 bytes")))
}

/// A notice the extension may show. `what` is `waiting`, `closed` or `done`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct OpenedNotice {
    pub computer_id: String,
    pub what: String,
    pub reference: String,
    pub session: String,
    pub title: String,
    pub body: String,
    pub counter: u64,
}

fn open_notice_in(
    doc: &mut store::Doc,
    box_secret: [u8; 32],
    packet: &[u8],
) -> Result<OpenedNotice, PushError> {
    let mut replayed = false;
    let mut version = false;
    let mut not_a_notice = false;
    for (id, c) in doc.computers.clone() {
        let Some(peer) = b64_decode(&c.peer).and_then(|p| <[u8; 32]>::try_from(p).ok()) else { continue };
        // The phone sends toward the desktop, so a packet in that direction is a reflection and is refused.
        let mut link = Link::new(
            Direction::ToDesktop,
            box_secret,
            peer,
            c.version_max,
            VersionRange { min: c.version_min, max: c.version_max },
            [0; 3],
            0,
            c.recv.clone(),
        );
        match link.open(packet) {
            Err(OpenError::Dropped) => replayed = true,
            Err(OpenError::Version(_)) => version = true,
            Err(_) => {}
            Ok(opened) => {
                // Only a notice on the event stream is for this caller; anything else is left for the app to open.
                let notice = match (opened.stream, DesktopMsg::from_json(&opened.plaintext)) {
                    (Stream::Event, Ok(m)) if m.validate().is_ok() => match m {
                        DesktopMsg::Notice(n) => n,
                        _ => {
                            not_a_notice = true;
                            continue;
                        }
                    },
                    _ => {
                        not_a_notice = true;
                        continue;
                    }
                };
                // Committed only now, after the packet authenticated and proved to be a notice.
                doc.computers.get_mut(&id).expect("listed above").recv = link.recv_state().clone();
                let what = match notice.what {
                    NoticeKind::Waiting => "waiting",
                    NoticeKind::Closed => "closed",
                    NoticeKind::Done => "done",
                };
                return Ok(OpenedNotice {
                    computer_id: id,
                    what: what.into(),
                    reference: notice.reference,
                    session: notice.session,
                    title: notice.title,
                    body: notice.body,
                    counter: opened.counter,
                });
            }
        }
    }
    Err(if version {
        PushError::Version
    } else if not_a_notice {
        PushError::NotANotice
    } else if replayed {
        PushError::Replayed
    } else {
        PushError::Unreadable
    })
}

/// Opens the sealed notice of a push (`sealed`, URL-safe base64) with the `box` secret, against every computer in the
/// shared file at `store_path`. The receive state is changed only when a notice opened.
#[uniffi::export]
pub fn open_notice(store_path: String, box_secret: Vec<u8>, sealed: String) -> Result<OpenedNotice, PushError> {
    let secret = key32(&box_secret, "box secret")?;
    let packet = b64_decode(sealed.trim()).ok_or(PushError::Unreadable)?;
    with_doc(Path::new(&store_path), |doc| {
        let r = open_notice_in(doc, secret, &packet);
        let changed = r.is_ok();
        (r, changed)
    })?
}

/// What the app takes from a packet that is not a push notice.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct OpenedPacket {
    pub computer_id: String,
    /// `state`, `event` or `activity`.
    pub stream: String,
    pub counter: u64,
    pub plaintext: Vec<u8>,
}

/// The app's way to open any packet from a paired computer through the same shared state: the check, the open and the
/// commit happen under the lock, so what the extension already took is a replay here and the other way round.
#[uniffi::export]
pub fn open_packet(store_path: String, box_secret: Vec<u8>, packet: Vec<u8>) -> Result<OpenedPacket, PushError> {
    let secret = key32(&box_secret, "box secret")?;
    with_doc(Path::new(&store_path), |doc| {
        let mut replayed = false;
        let mut version = false;
        for (id, c) in doc.computers.clone() {
            let Some(peer) = b64_decode(&c.peer).and_then(|p| <[u8; 32]>::try_from(p).ok()) else { continue };
            let mut link = Link::new(
                Direction::ToDesktop,
                secret,
                peer,
                c.version_max,
                VersionRange { min: c.version_min, max: c.version_max },
                [0; 3],
                0,
                c.recv.clone(),
            );
            match link.open(&packet) {
                Err(OpenError::Dropped) => replayed = true,
                Err(OpenError::Version(_)) => version = true,
                Err(_) => {}
                Ok(o) => {
                    doc.computers.get_mut(&id).expect("listed above").recv = link.recv_state().clone();
                    let stream = match o.stream {
                        Stream::State => "state",
                        Stream::Event => "event",
                        Stream::Activity => "activity",
                    };
                    return (
                        Ok(OpenedPacket { computer_id: id, stream: stream.into(), counter: o.counter, plaintext: o.plaintext }),
                        true,
                    );
                }
            }
        }
        let e = if version {
            PushError::Version
        } else if replayed {
            PushError::Replayed
        } else {
            PushError::Unreadable
        };
        (Err(e), false)
    })?
}

/// Adds a paired computer, or updates its key and version range. The receive state of a computer already listed with
/// the same key is kept; a new key starts from a fresh state.
#[uniffi::export]
pub fn store_set_computer(
    store_path: String,
    computer_id: String,
    peer_box_public: Vec<u8>,
    version_min: u8,
    version_max: u8,
) -> Result<(), PushError> {
    let peer = b64(&key32(&peer_box_public, "peer box public key")?);
    if version_min == 0 || version_min > version_max {
        return Err(invalid("version range"));
    }
    with_doc(Path::new(&store_path), |doc| {
        let recv = match doc.computers.get(&computer_id) {
            Some(c) if c.peer == peer => c.recv.clone(),
            _ => RecvState::default(),
        };
        doc.computers.insert(computer_id, Computer { peer, version_min, version_max, recv });
        ((), true)
    })
    .map_err(Into::into)
}

/// Forgets a computer (disconnecting it). Others are untouched.
#[uniffi::export]
pub fn store_remove_computer(store_path: String, computer_id: String) -> Result<(), PushError> {
    with_doc(Path::new(&store_path), |doc| ((), doc.computers.remove(&computer_id).is_some())).map_err(Into::into)
}

#[uniffi::export]
pub fn store_set_hide_details(store_path: String, hide: bool) -> Result<(), PushError> {
    with_doc(Path::new(&store_path), |doc| {
        let changed = doc.hide_details != hide;
        doc.hide_details = hide;
        ((), changed)
    })
    .map_err(Into::into)
}

/// The "Hide details" switch. A missing file means off.
#[uniffi::export]
pub fn store_hide_details(store_path: String) -> Result<bool, PushError> {
    with_doc(Path::new(&store_path), |doc| (doc.hide_details, false)).map_err(Into::into)
}

/// The grant a phone hands a relay with its push token: `auth public key (32) ‖ expiry unix ms (8, big endian) ‖
/// Ed25519 signature (64)`, URL-safe base64, over the label, the token, the relay id and the expiry.
/// The relay and the push gateway check exactly these bytes (`relay/src/push.rs`).
#[uniffi::export]
pub fn make_grant(
    auth_seed: Vec<u8>,
    box_secret: Vec<u8>,
    token: String,
    relay_id: String,
    expires_unix_ms: u64,
) -> Result<String, PushError> {
    let id = Identity::from_secrets(key32(&auth_seed, "auth seed")?, key32(&box_secret, "box secret")?);
    let signed = Fields::new("bobix-push-grant-v1")
        .field(token.as_bytes())
        .field(relay_id.as_bytes())
        .num(expires_unix_ms)
        .done();
    let sig = id.sign_auth(&signed);
    let mut raw = id.public().auth.to_vec();
    raw.extend_from_slice(&expires_unix_ms.to_be_bytes());
    raw.extend_from_slice(&sig);
    Ok(b64(&raw))
}
