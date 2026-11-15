//! The relay's frames and the receive rules, from the shared crate: Dart builds and reads no frame and runs no counter
//! check of its own, it moves bytes and text. Everything here forwards to `bobix-link` (`Frame`, `auth_frame`, `Link`), the
//! same code the desktop and the relay use, so a change in the crate shows up here and nowhere else.

use super::core::{CoreError, CounterTake};
use super::messages::{parse_desktop_message, DesktopMessage};
use bobix_link::command;
use bobix_link::counters::RecvState;
use bobix_link::envelope::{Direction, OpenError, Update, VersionRange, CURRENT_VERSION};
use bobix_link::keys::Identity;
use bobix_link::link::Link;
use bobix_link::messages::Who;
use bobix_link::pairing::PairingCode;
use bobix_link::relay_frames::{ErrorCode, Frame, PresenceState};
use bobix_link::{b64, b64_decode, hex, random};

/// Why the relay refused or ended something (`error` frames).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayError {
    Revoked,
    Oversize,
    Invite,
    Update,
    Grant,
    Auth,
    Malformed,
    PairExpired,
    RelayUrl,
    NoPeer,
    Other,
}

impl From<&ErrorCode> for RelayError {
    fn from(c: &ErrorCode) -> Self {
        match c {
            ErrorCode::Revoked => RelayError::Revoked,
            ErrorCode::Oversize => RelayError::Oversize,
            ErrorCode::Invite => RelayError::Invite,
            ErrorCode::Update => RelayError::Update,
            ErrorCode::Grant => RelayError::Grant,
            ErrorCode::Auth => RelayError::Auth,
            ErrorCode::Malformed => RelayError::Malformed,
            ErrorCode::PairExpired => RelayError::PairExpired,
            ErrorCode::RelayUrl => RelayError::RelayUrl,
            ErrorCode::NoPeer => RelayError::NoPeer,
            ErrorCode::Other => RelayError::Other,
        }
    }
}

/// Which side an `error` is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelaySide {
    Relay,
    Desktop,
    Phone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeerState {
    Online,
    Offline,
    Asleep,
}

/// A frame from the relay, as far as the phone acts on it. Anything else is `Other` (ignored).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayFrame {
    Challenge { nonce: String },
    /// A sealed packet from `from`, its body already decoded.
    Msg { pid: String, from: String, packet: Vec<u8> },
    Presence { peer: String, state: PeerState },
    Error { code: RelayError, who: Option<RelaySide> },
    Queued { pid: String },
    Delivered { pid: String },
    Dropped { pid: String },
    Other,
}

/// Reads one text frame from the relay. A frame that is too big or not a frame is `Malformed`.
pub fn parse_relay_frame(text: String) -> Result<RelayFrame, CoreError> {
    let frame = Frame::parse(&text).map_err(|_| CoreError::Malformed)?;
    Ok(match frame {
        Frame::Challenge { nonce } => RelayFrame::Challenge { nonce },
        Frame::Msg { pid, from, body } => RelayFrame::Msg { pid, from, packet: b64_decode(&body).ok_or(CoreError::Malformed)? },
        Frame::Presence { peer, state } => RelayFrame::Presence {
            peer,
            state: match state {
                PresenceState::Online => PeerState::Online,
                PresenceState::Offline => PeerState::Offline,
                PresenceState::Asleep => PeerState::Asleep,
            },
        },
        Frame::Error { code, who } => RelayFrame::Error {
            code: RelayError::from(&code),
            who: who.map(|w| match w {
                Who::Relay => RelaySide::Relay,
                Who::Desktop => RelaySide::Desktop,
                Who::Phone => RelaySide::Phone,
            }),
        },
        Frame::Queued { pid } => RelayFrame::Queued { pid },
        Frame::Delivered { pid } => RelayFrame::Delivered { pid },
        Frame::Dropped { pid } => RelayFrame::Dropped { pid },
        _ => RelayFrame::Other,
    })
}

fn text(f: &Frame) -> Result<String, CoreError> {
    f.to_text().map_err(|_| CoreError::Limit)
}

/// The answer to the relay's challenge: signed with the `auth` key over the address the phone connected to. `None`-like
/// refusals (a relay address the crate does not accept) are `InsecureRelay`.
pub fn relay_auth_frame(
    auth_seed: Vec<u8>,
    box_secret: Vec<u8>,
    relay_url: String,
    challenge: String,
    version_min: u8,
    version_max: u8,
) -> Result<String, CoreError> {
    let id = Identity::from_secrets(
        auth_seed.as_slice().try_into().map_err(|_| CoreError::InvalidKey)?,
        box_secret.as_slice().try_into().map_err(|_| CoreError::InvalidKey)?,
    );
    let frame = command::auth_frame(&id, &relay_url, &challenge, VersionRange { min: version_min, max: version_max })
        .ok_or(CoreError::InsecureRelay)?;
    text(&frame)
}

/// The devices allowed to send to this one.
pub fn relay_peers_frame(allow: Vec<String>) -> String {
    text(&Frame::Peers { allow }).expect("a peers frame is small")
}

/// The `hello` body for the open code, to be matched by the relay with the computer that opened it.
pub fn relay_pair_send_frame(pairing_uri: String, body: Vec<u8>) -> Result<String, CoreError> {
    let code = PairingCode::parse(&pairing_uri)?;
    text(&Frame::PairSend { relay_token: b64(&code.relay_token()), body: b64(&body) })
}

/// A frame to send, and the id the relay's `delivered` / `queued` / `dropped` will name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutFrame {
    pub pid: String,
    pub text: String,
}

/// A sealed packet for `to`, which the relay may hold for `ttl_seconds` while that device is offline.
pub fn relay_send_frame(to: String, packet: Vec<u8>, ttl_seconds: u32) -> Result<OutFrame, CoreError> {
    let pid = hex(&random::<8>());
    let text = text(&Frame::Send { pid: pid.clone(), to, ttl: ttl_seconds, collapse: None, push: None, body: b64(&packet) })?;
    Ok(OutFrame { pid, text })
}

/// The acknowledgement of a packet.
pub fn relay_ack_frame(pid: String) -> String {
    text(&Frame::Ack { pid }).expect("an ack is small")
}

// ---- the phone's side of one pairing ----

/// A packet that was sealed, and the reservation to store before it leaves (when `Some`).
pub struct SealedPacket {
    pub packet: Vec<u8>,
    pub reserve: Option<u64>,
}

/// What opening a packet from the computer came to.
pub enum OpenedPacket {
    Message(DesktopMessage),
    /// Late, replayed, reflected, forged or unreadable: no reply, nothing changes.
    Dropped,
    /// The packet's version is outside what this app understands, and the computer is the one to update.
    PeerMustUpdate,
    /// ... and this app is the one to update.
    WeMustUpdate,
}

/// This phone's two directions with one computer: the crate's `Link`, which seals on the `event` stream, applies the
/// receive rule of each stream and judges the version only after a packet authenticated. The caller stores what the
/// results say to store (a send reservation before the packet leaves, `recv_state` after one was accepted).
#[flutter_rust_bridge::frb(opaque)]
pub struct PhoneLink(Link);

impl PhoneLink {
    /// `event_reservation` is the last value a send asked to store, `recv_state` the stored receive state (`recv_state_new`
    /// when none), `now_unix_ms` the clock.
    #[flutter_rust_bridge::frb(sync)]
    pub fn create(box_secret: Vec<u8>, desktop_box_public: Vec<u8>, event_reservation: u64, now_unix_ms: u64, recv_state: String) -> Result<PhoneLink, CoreError> {
        let recv: RecvState = serde_json::from_str(&recv_state).map_err(|_| CoreError::Malformed)?;
        Ok(PhoneLink(Link::new(
            Direction::ToDesktop,
            box_secret.as_slice().try_into().map_err(|_| CoreError::InvalidKey)?,
            desktop_box_public.as_slice().try_into().map_err(|_| CoreError::InvalidKey)?,
            CURRENT_VERSION,
            VersionRange::current(),
            [0, event_reservation, 0],
            now_unix_ms,
            recv,
        )))
    }

    /// The next counter of the `event` stream, for a message that carries its own (the pairing `hello`).
    #[flutter_rust_bridge::frb(sync)]
    pub fn take_counter(&mut self) -> CounterTake {
        let (counter, reserve) = self.0.reserve_counter(bobix_link::envelope::Stream::Event);
        CounterTake { counter, reserve: reserve.map(|(_, v)| v) }
    }

    #[flutter_rust_bridge::frb(sync)]
    pub fn seal(&mut self, plaintext: Vec<u8>) -> Result<SealedPacket, CoreError> {
        let s = self.0.seal(bobix_link::envelope::Stream::Event, &plaintext).map_err(|_| CoreError::InvalidKey)?;
        Ok(SealedPacket { packet: s.packet, reserve: s.reserve.map(|(_, v)| v) })
    }

    #[flutter_rust_bridge::frb(sync)]
    pub fn open(&mut self, packet: Vec<u8>) -> OpenedPacket {
        match self.0.open(&packet) {
            Ok(o) => parse_desktop_message(o.plaintext).map(OpenedPacket::Message).unwrap_or(OpenedPacket::Dropped),
            Err(OpenError::Version(Update::Peer)) => OpenedPacket::PeerMustUpdate,
            Err(OpenError::Version(Update::Us)) => OpenedPacket::WeMustUpdate,
            Err(_) => OpenedPacket::Dropped,
        }
    }

    /// The receive state as JSON, to be stored after a packet was accepted.
    #[flutter_rust_bridge::frb(sync)]
    pub fn recv_state(&self) -> String {
        serde_json::to_string(self.0.recv_state()).expect("a state serialises")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bobix_link::counters::RecvState;
    use bobix_link::envelope::Stream;
    use bobix_link::messages::DesktopMsg;

    fn desktop_link(desktop: &Identity, phone: &Identity, version: u8) -> Link {
        Link::new(Direction::ToPhone, *desktop.box_secret(), phone.public().boxk, version, VersionRange::current(), [0; 3], 1_000, RecvState::default())
    }

    fn phone_link(desktop: &Identity, phone: &Identity) -> PhoneLink {
        PhoneLink::create(phone.box_secret().to_vec(), desktop.public().boxk.to_vec(), 0, 1_000, serde_json::to_string(&RecvState::default()).unwrap()).unwrap()
    }

    fn error_message() -> Vec<u8> {
        DesktopMsg::Error { code: "x".into(), who: None }.to_json()
    }

    #[test]
    fn frames_from_the_relay_are_read_by_the_crate() {
        assert_eq!(parse_relay_frame(r#"{"type":"challenge","nonce":"n1"}"#.into()), Ok(RelayFrame::Challenge { nonce: "n1".into() }));
        let msg = format!(r#"{{"type":"msg","pid":"p","from":"d","body":"{}"}}"#, b64(&[1, 2, 3]));
        assert_eq!(parse_relay_frame(msg), Ok(RelayFrame::Msg { pid: "p".into(), from: "d".into(), packet: vec![1, 2, 3] }));
        assert_eq!(
            parse_relay_frame(r#"{"type":"error","code":"update","who":"desktop"}"#.into()),
            Ok(RelayFrame::Error { code: RelayError::Update, who: Some(RelaySide::Desktop) })
        );
        assert_eq!(
            parse_relay_frame(r#"{"type":"error","code":"somethingNew"}"#.into()),
            Ok(RelayFrame::Error { code: RelayError::Other, who: None }),
            "a code this build does not know"
        );
        assert_eq!(parse_relay_frame(r#"{"type":"presence","peer":"d","state":"asleep"}"#.into()), Ok(RelayFrame::Presence { peer: "d".into(), state: PeerState::Asleep }));
        assert_eq!(parse_relay_frame(r#"{"type":"withdrawn","pid":"p"}"#.into()), Ok(RelayFrame::Other));
        assert_eq!(parse_relay_frame("not a frame".into()), Err(CoreError::Malformed));
        assert_eq!(parse_relay_frame(r#"{"type":"msg","pid":"p","from":"d","body":"***"}"#.into()), Err(CoreError::Malformed), "a body that is not base64");
        assert_eq!(parse_relay_frame(format!(r#"{{"type":"challenge","nonce":"{}"}}"#, "x".repeat(300_000))), Err(CoreError::Malformed), "too big");
    }

    #[test]
    fn frames_to_the_relay_are_built_by_the_crate() {
        let id = Identity::generate();
        let text = relay_auth_frame(id.auth_seed().to_vec(), id.box_secret().to_vec(), "ws://127.0.0.1:8787".into(), "nonce".into(), 1, 1).unwrap();
        match Frame::parse(&text).unwrap() {
            Frame::Auth { id: who, relay, versions, .. } => {
                assert_eq!(who, id.device_id());
                assert_eq!(relay, "ws://127.0.0.1:8787");
                assert_eq!((versions.min, versions.max), (1, 1));
            }
            other => panic!("not an auth frame: {other:?}"),
        }
        assert_eq!(relay_auth_frame(id.auth_seed().to_vec(), id.box_secret().to_vec(), "ws://evil.example".into(), "n".into(), 1, 1), Err(CoreError::InsecureRelay));
        let out = relay_send_frame("d".into(), vec![9], 600).unwrap();
        assert!(matches!(Frame::parse(&out.text).unwrap(), Frame::Send { pid, to, ttl: 600, .. } if pid == out.pid && to == "d"));
        assert!(matches!(Frame::parse(&relay_ack_frame("p".into())).unwrap(), Frame::Ack { pid } if pid == "p"));
        assert!(matches!(Frame::parse(&relay_peers_frame(vec!["d".into()])).unwrap(), Frame::Peers { allow } if allow == vec!["d".to_string()]));
    }

    #[test]
    fn the_receive_rules_are_the_crates() {
        let (desktop, phone) = (Identity::generate(), Identity::generate());
        let mut from_desktop = desktop_link(&desktop, &phone, bobix_link::envelope::CURRENT_VERSION);
        let mut link = phone_link(&desktop, &phone);
        let packet = from_desktop.seal(Stream::Event, &error_message()).unwrap().packet;
        assert!(matches!(link.open(packet.clone()), OpenedPacket::Message(m) if m.kind == "error"));
        assert!(matches!(link.open(packet), OpenedPacket::Dropped), "a replay");
        assert!(matches!(link.open(vec![0; 10]), OpenedPacket::Dropped), "too short");
        let state = link.recv_state();
        assert!(state.contains("event"));
        // A phone that restarts with the stored state still refuses the replay.
        let mut again = PhoneLink::create(phone.box_secret().to_vec(), desktop.public().boxk.to_vec(), 0, 1_000, state).unwrap();
        let replay = from_desktop.seal(Stream::Event, &error_message()).unwrap().packet;
        assert!(matches!(again.open(replay.clone()), OpenedPacket::Message(_)));
        assert!(matches!(again.open(replay), OpenedPacket::Dropped));
    }

    #[test]
    fn a_version_outside_the_range_says_who_updates_after_authentication() {
        let (desktop, phone) = (Identity::generate(), Identity::generate());
        let mut link = phone_link(&desktop, &phone);
        let newer = desktop_link(&desktop, &phone, 9).seal(Stream::Event, &error_message()).unwrap().packet;
        assert!(matches!(link.open(newer), OpenedPacket::WeMustUpdate));
        let older = desktop_link(&desktop, &phone, 0).seal(Stream::Event, &error_message()).unwrap().packet;
        assert!(matches!(link.open(older), OpenedPacket::PeerMustUpdate));
        let forged = Identity::generate();
        let by_stranger = desktop_link(&forged, &phone, 9).seal(Stream::Event, &error_message()).unwrap().packet;
        assert!(matches!(link.open(by_stranger), OpenedPacket::Dropped), "an unauthenticated version byte says nothing");
    }

    #[test]
    fn counters_never_go_back_and_ask_for_their_reservation() {
        let (desktop, phone) = (Identity::generate(), Identity::generate());
        let mut link = phone_link(&desktop, &phone);
        let first = link.take_counter();
        let second = link.take_counter();
        assert!(second.counter > first.counter);
        assert!(first.reserve.is_some(), "the first take asks to store a reservation");
        let sealed = link.seal(vec![1]).unwrap();
        assert!(!sealed.packet.is_empty());
    }
}
