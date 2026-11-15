//! What the Android push service calls (through UniFFI): opens one sealed `notice` that arrived as a push.
//!
//! It only forwards to `bobix-link`: peek, the `event` window check, the open, the version check, the message parse and the
//! window commit, in the order `Link::open` uses. The new window state is returned for the caller to store; nothing is
//! written here. The push service and the app are one app, but not one moment: the caller holds a file lock around
//! "read the state, call this, store the new state".

use bobix_link::envelope::{self, Direction, OpenError, Stream, VersionRange};
use bobix_link::{counters, keys, messages};
use crypto_box::{PublicKey, SecretKey};

/// A notice that opened and passed the window. `state` is the receive state to store in place of the one passed in.
#[derive(uniffi::Record)]
pub struct OpenedNotice {
    /// `waiting`, `closed` or `done`.
    pub what: String,
    /// The request or question the notice is about (the same `ref` for its `waiting` and its `closed`).
    pub reference: String,
    pub session: String,
    pub title: String,
    pub body: String,
    pub state: String,
}

/// Why nothing is shown. The push service drops the push in every case; the names are for its log (never the content).
#[derive(Debug, PartialEq, Eq, uniffi::Error)]
pub enum OpenNoticeError {
    /// A key of the wrong length, or a peer key that is a small-order point.
    InvalidKey,
    /// The stored window does not parse.
    BadState,
    /// Not a packet, or in this device's own direction.
    NotAPacket,
    /// Not on the `event` stream, where every notice travels.
    WrongStream,
    /// The counter was already used, or is older than the window.
    Replayed,
    /// It did not authenticate (wrong key, altered).
    Decrypt,
    /// Its version is one this app does not understand.
    Version,
    /// It opened but is not a valid `notice`.
    NotANotice,
}

impl std::fmt::Display for OpenNoticeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

fn key32(b: &[u8]) -> Result<[u8; 32], OpenNoticeError> {
    b.try_into().map_err(|_| OpenNoticeError::InvalidKey)
}

/// The receive window of a computer that has sent nothing yet (the first `state` to pass to [`open_notice`]).
#[uniffi::export]
pub fn new_notice_window() -> String {
    serde_json::to_string(&counters::RecvState::default()).expect("a state serialises")
}

/// Opens `packet` (from the computer whose `box` public key is `desktop_box_public`) with this phone's `box` secret,
/// against the receive window `state` of that computer (`new_notice_window` gives the first one) and the protocol
/// versions this pair accepts.
#[uniffi::export]
pub fn open_notice(
    box_secret: Vec<u8>,
    desktop_box_public: Vec<u8>,
    packet: Vec<u8>,
    state: String,
    version_min: u8,
    version_max: u8,
) -> Result<OpenedNotice, OpenNoticeError> {
    let secret = SecretKey::from(key32(&box_secret)?);
    // A small-order peer key would make the shared secret all zeros: the crate refuses it.
    let peer: PublicKey = keys::box_public(key32(&desktop_box_public)?).ok_or(OpenNoticeError::InvalidKey)?;
    let mut window: counters::RecvState = serde_json::from_str(&state).map_err(|_| OpenNoticeError::BadState)?;

    // This device sends `ToDesktop`; a packet in that direction is its own, reflected.
    let header = envelope::peek(Direction::ToDesktop, &packet).map_err(|_| OpenNoticeError::NotAPacket)?;
    if header.lane.stream != Stream::Event {
        return Err(OpenNoticeError::WrongStream);
    }
    if !window.check(Stream::Event, header.counter) {
        return Err(OpenNoticeError::Replayed);
    }
    let plain = envelope::open(&secret, &peer, &packet).map_err(|e| match e {
        OpenError::Dropped => OpenNoticeError::Replayed,
        _ => OpenNoticeError::Decrypt,
    })?;
    envelope::check_version(&VersionRange { min: version_min, max: version_max }, header.version).map_err(|_| OpenNoticeError::Version)?;

    let msg = messages::DesktopMsg::from_json(&plain).map_err(|_| OpenNoticeError::NotANotice)?;
    msg.validate().map_err(|_| OpenNoticeError::NotANotice)?;
    let messages::DesktopMsg::Notice(notice) = msg else { return Err(OpenNoticeError::NotANotice) };

    // Only a packet that authenticated and parsed moves the window, so a forged one cannot use up a counter.
    window.commit(Stream::Event, header.counter);
    Ok(OpenedNotice {
        what: match notice.what {
            messages::NoticeKind::Waiting => "waiting",
            messages::NoticeKind::Closed => "closed",
            messages::NoticeKind::Done => "done",
        }
        .into(),
        reference: notice.reference,
        session: notice.session,
        title: notice.title,
        body: notice.body,
        state: serde_json::to_string(&window).expect("a state serialises"),
    })
}

/// A packet that opened, whatever its stream: what the app takes through the shared window.
#[derive(uniffi::Record)]
pub struct OpenedPacket {
    /// `state`, `event` or `activity`.
    pub stream: String,
    pub counter: u64,
    pub plaintext: Vec<u8>,
    /// The receive window to store in place of the one passed in.
    pub state: String,
}

/// Opens any packet from one computer against its stored window: the same check, open and commit as [`open_notice`],
/// for a packet that is not necessarily a notice.
#[uniffi::export]
pub fn open_packet(
    box_secret: Vec<u8>,
    desktop_box_public: Vec<u8>,
    packet: Vec<u8>,
    state: String,
    version_min: u8,
    version_max: u8,
) -> Result<OpenedPacket, OpenNoticeError> {
    let secret = SecretKey::from(key32(&box_secret)?);
    let peer: PublicKey = keys::box_public(key32(&desktop_box_public)?).ok_or(OpenNoticeError::InvalidKey)?;
    let mut window: counters::RecvState = serde_json::from_str(&state).map_err(|_| OpenNoticeError::BadState)?;
    let header = envelope::peek(Direction::ToDesktop, &packet).map_err(|_| OpenNoticeError::NotAPacket)?;
    if !window.check(header.lane.stream, header.counter) {
        return Err(OpenNoticeError::Replayed);
    }
    let plaintext = envelope::open(&secret, &peer, &packet).map_err(|e| match e {
        OpenError::Dropped => OpenNoticeError::Replayed,
        _ => OpenNoticeError::Decrypt,
    })?;
    envelope::check_version(&VersionRange { min: version_min, max: version_max }, header.version).map_err(|_| OpenNoticeError::Version)?;
    window.commit(header.lane.stream, header.counter);
    Ok(OpenedPacket {
        stream: match header.lane.stream {
            Stream::State => "state",
            Stream::Event => "event",
            Stream::Activity => "activity",
        }
        .into(),
        counter: header.counter,
        plaintext,
        state: serde_json::to_string(&window).expect("a state serialises"),
    })
}

/// The grant a phone hands a relay with its push token: `auth public key (32) ‖ expiry unix ms (8, big endian) ‖
/// Ed25519 signature (64)`, URL-safe base64, over the label, the token, the relay id and the expiry. The same bytes the
/// iOS push core makes and `relay/src/push.rs` checks. The `auth` seed never leaves native code: Dart gets the grant.
#[uniffi::export]
pub fn make_grant(auth_seed: Vec<u8>, box_secret: Vec<u8>, token: String, relay_id: String, expires_unix_ms: u64) -> Result<String, OpenNoticeError> {
    let id = keys::Identity::from_secrets(key32(&auth_seed)?, key32(&box_secret)?);
    let signed = bobix_link::Fields::new("bobix-push-grant-v1").field(token.as_bytes()).field(relay_id.as_bytes()).num(expires_unix_ms).done();
    let mut raw = id.public().auth.to_vec();
    raw.extend_from_slice(&expires_unix_ms.to_be_bytes());
    raw.extend_from_slice(&id.sign_auth(&signed));
    Ok(bobix_link::b64(&raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bobix_link::envelope::Lane;

    const PHONE_SECRET: [u8; 32] = [4; 32];
    const DESKTOP_SECRET: [u8; 32] = [2; 32];

    fn desktop_box_public() -> Vec<u8> {
        PublicKey::from(&SecretKey::from(DESKTOP_SECRET)).as_bytes().to_vec()
    }

    fn phone_box_public() -> PublicKey {
        PublicKey::from(&SecretKey::from(PHONE_SECRET))
    }

    fn seal(counter: u64, dir: Direction, stream: Stream, plain: &str) -> Vec<u8> {
        envelope::seal(1, Lane { dir, stream }, counter, &SecretKey::from(DESKTOP_SECRET), &phone_box_public(), plain.as_bytes())
    }

    fn notice(what: &str) -> String {
        format!(r#"{{"kind":"notice","what":"{what}","ref":"r1","session":"s1","title":"shop-app needs you","body":"Bash wants to run: ls"}}"#)
    }

    fn open(packet: Vec<u8>, state: &str) -> Result<OpenedNotice, OpenNoticeError> {
        open_notice(PHONE_SECRET.to_vec(), desktop_box_public(), packet, state.into(), 1, 1)
    }

    fn fresh() -> String {
        serde_json::to_string(&counters::RecvState::default()).unwrap()
    }

    #[test]
    fn a_waiting_notice_opens_and_moves_the_window() {
        let got = open(seal(10, Direction::ToPhone, Stream::Event, &notice("waiting")), &fresh()).unwrap();
        assert_eq!((got.what.as_str(), got.reference.as_str(), got.title.as_str()), ("waiting", "r1", "shop-app needs you"));
        assert_eq!(got.body, "Bash wants to run: ls");
        // The same packet again is refused with the stored state; a late counter inside the window is still accepted.
        let again = open(seal(10, Direction::ToPhone, Stream::Event, &notice("waiting")), &got.state);
        assert_eq!(again.err(), Some(OpenNoticeError::Replayed));
        let late = open(seal(9, Direction::ToPhone, Stream::Event, &notice("closed")), &got.state).unwrap();
        assert_eq!(late.what, "closed");
    }

    #[test]
    fn what_cannot_be_opened_is_refused_and_leaves_the_window_alone() {
        let ok = seal(5, Direction::ToPhone, Stream::Event, &notice("waiting"));
        let mut altered = ok.clone();
        *altered.last_mut().unwrap() ^= 1;
        assert_eq!(open(altered, &fresh()).err(), Some(OpenNoticeError::Decrypt));
        assert_eq!(open(ok[..10].to_vec(), &fresh()).err(), Some(OpenNoticeError::NotAPacket));
        // Wrong stream, own direction, a message that is not a notice, a bad window, a bad key.
        assert_eq!(open(seal(5, Direction::ToPhone, Stream::State, &notice("waiting")), &fresh()).err(), Some(OpenNoticeError::WrongStream));
        assert_eq!(open(seal(5, Direction::ToDesktop, Stream::Event, &notice("waiting")), &fresh()).err(), Some(OpenNoticeError::NotAPacket));
        assert_eq!(open(seal(5, Direction::ToPhone, Stream::Event, r#"{"kind":"home"}"#), &fresh()).err(), Some(OpenNoticeError::NotANotice));
        assert_eq!(open(ok.clone(), "nope").err(), Some(OpenNoticeError::BadState));
        assert_eq!(open_notice(vec![1; 5], desktop_box_public(), ok.clone(), fresh(), 1, 1).err(), Some(OpenNoticeError::InvalidKey));
        assert_eq!(open_notice(PHONE_SECRET.to_vec(), vec![0; 32], ok.clone(), fresh(), 1, 1).err(), Some(OpenNoticeError::InvalidKey));
        // A packet from another computer (another key) does not authenticate.
        assert_eq!(open_notice(PHONE_SECRET.to_vec(), vec![9; 32], ok, fresh(), 1, 1).err(), Some(OpenNoticeError::Decrypt));
    }

    // ---- fixtures for the Kotlin tests of the push service ----
    //
    // `android/app/src/test/resources/notice-fixtures.json` holds sealed notices made with fixed keys, in the order the
    // Kotlin tests feed them to a push service, and what each must do. This test rewrites it when run with
    // `UPDATE_NOTICE_FIXTURES=1` and otherwise fails when it is stale, so the packets always open under this crate.

    const OTHER_DESKTOP_SECRET: [u8; 32] = [9; 32];

    fn fixture_packet(counter: u64, dir: Direction, stream: Stream, sender: [u8; 32], plain: &str) -> String {
        let tail = [counter as u8; 14];
        let lane = Lane { dir, stream };
        let packet = envelope::seal_with_tail(1, lane, counter, tail, &SecretKey::from(sender), &phone_box_public(), plain.as_bytes());
        bobix_link::b64(&packet)
    }

    fn fixtures() -> String {
        let n = |what: &str, r: &str, title: &str, body: &str| {
            format!(r#"{{"kind":"notice","what":"{what}","ref":"{r}","session":"s-{r}","title":"{title}","body":"{body}"}}"#)
        };
        let (ev, up) = (Stream::Event, Direction::ToPhone);
        let mut altered = bobix_link::b64_decode(&fixture_packet(6000, up, ev, DESKTOP_SECRET, &n("waiting", "r9", "x", "y"))).unwrap();
        *altered.last_mut().unwrap() ^= 1;
        // (name, packet, outcome): `waiting`, `closed`, `done`, or the error the opener gives.
        let cases: Vec<(&str, String, &str)> = vec![
            ("waiting", fixture_packet(100, up, ev, DESKTOP_SECRET, &n("waiting", "r1", "shop-app needs you", "Bash wants to run: ls")), "waiting"),
            ("waiting again for the same ref", fixture_packet(101, up, ev, DESKTOP_SECRET, &n("waiting", "r1", "shop-app needs you", "Bash wants to run: pwd")), "waiting"),
            ("closed", fixture_packet(102, up, ev, DESKTOP_SECRET, &n("closed", "r1", "", "")), "closed"),
            ("done", fixture_packet(103, up, ev, DESKTOP_SECRET, &n("done", "d1", "shop-app finished", "Fixed the tests")), "done"),
            ("replay of the first", fixture_packet(100, up, ev, DESKTOP_SECRET, &n("waiting", "r1", "shop-app needs you", "Bash wants to run: ls")), "Replayed"),
            ("late but inside the window", fixture_packet(90, up, ev, DESKTOP_SECRET, &n("waiting", "r2", "api needs you", "Edit wants to change a file")), "waiting"),
            ("jump ahead", fixture_packet(5000, up, ev, DESKTOP_SECRET, &n("waiting", "r3", "web needs you", "Question")), "waiting"),
            ("too old for the window", fixture_packet(200, up, ev, DESKTOP_SECRET, &n("waiting", "r4", "web needs you", "Question")), "Replayed"),
            ("altered", bobix_link::b64(&altered), "Decrypt"),
            ("not a notice", fixture_packet(6001, up, ev, DESKTOP_SECRET, r#"{"kind":"home"}"#), "NotANotice"),
            ("on the state stream", fixture_packet(6002, up, Stream::State, DESKTOP_SECRET, &n("waiting", "r5", "x", "y")), "WrongStream"),
            ("reflected", fixture_packet(6003, Direction::ToDesktop, ev, DESKTOP_SECRET, &n("waiting", "r5", "x", "y")), "NotAPacket"),
            ("from another computer", fixture_packet(6004, up, ev, OTHER_DESKTOP_SECRET, &n("waiting", "r5", "x", "y")), "Decrypt"),
        ];
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        let rows: Vec<String> = cases.iter().map(|(name, packet, outcome)| format!(r#"    {{"name": "{name}", "sealed": "{packet}", "outcome": "{outcome}"}}"#)).collect();
        format!(
            "{{\n  \"note\": \"Generated by the test `fixtures_are_current` in mobile/rust/src/push_opener.rs (UPDATE_NOTICE_FIXTURES=1 rewrites it). Made-up keys and data.\",\n  \"phoneBoxSecret\": \"{}\",\n  \"desktopBoxPublic\": \"{}\",\n  \"cases\": [\n{}\n  ]\n}}\n",
            hex(&PHONE_SECRET),
            hex(&desktop_box_public()),
            rows.join(",\n")
        )
    }

    #[test]
    fn fixtures_are_current_and_open_as_they_say() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../android/app/src/test/resources/notice-fixtures.json");
        let want = fixtures();
        if std::env::var("UPDATE_NOTICE_FIXTURES").is_ok() {
            std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap()).unwrap();
            std::fs::write(path, &want).unwrap();
        }
        assert_eq!(std::fs::read_to_string(path).unwrap_or_default(), want, "run with UPDATE_NOTICE_FIXTURES=1 to rewrite {path}");
        // Feeding them in order must give each outcome, the window moving only for the ones that open.
        let mut state = fresh();
        for line in want.lines().filter(|l| l.contains("\"sealed\"")) {
            let field = |k: &str| line.split(&format!("\"{k}\": \"")).nth(1).unwrap().split('"').next().unwrap().to_string();
            let packet = bobix_link::b64_decode(&field("sealed")).unwrap();
            let outcome = field("outcome");
            let got = open(packet, &state);
            match got {
                Ok(o) => {
                    assert_eq!(o.what, outcome, "{line}");
                    state = o.state;
                }
                Err(e) => assert_eq!(format!("{e:?}"), outcome, "{line}"),
            }
        }
    }

    #[test]
    fn open_packet_takes_any_stream_and_shares_the_window() {
        let home = seal(7, Direction::ToPhone, Stream::State, r#"{"kind":"home"}"#);
        let got = open_packet(PHONE_SECRET.to_vec(), desktop_box_public(), home.clone(), fresh(), 1, 1).unwrap();
        assert_eq!((got.stream.as_str(), got.counter), ("state", 7));
        assert_eq!(open_packet(PHONE_SECRET.to_vec(), desktop_box_public(), home, got.state.clone(), 1, 1).err(), Some(OpenNoticeError::Replayed));
        // A notice taken here is a replay for the push service that shares this window, and the other way round.
        let n = seal(20, Direction::ToPhone, Stream::Event, &notice("waiting"));
        let taken = open_packet(PHONE_SECRET.to_vec(), desktop_box_public(), n.clone(), got.state, 1, 1).unwrap();
        assert_eq!(open(n, &taken.state).err(), Some(OpenNoticeError::Replayed));
        // A version outside the pair's range is refused.
        assert_eq!(open_packet(PHONE_SECRET.to_vec(), desktop_box_public(), seal(30, Direction::ToPhone, Stream::Event, "{}"), fresh(), 2, 3).err(), Some(OpenNoticeError::Version));
    }

    #[test]
    fn a_grant_is_what_the_relay_checks() {
        let g = make_grant(vec![1; 32], vec![2; 32], "tok".into(), "relay.example".into(), 1_800_000_000_000).unwrap();
        let raw = bobix_link::b64_decode(&g).unwrap();
        assert_eq!(raw.len(), 32 + 8 + 64);
        assert_eq!(&raw[32..40], &1_800_000_000_000u64.to_be_bytes());
        let id = keys::Identity::from_secrets([1; 32], [2; 32]);
        assert_eq!(&raw[..32], &id.public().auth);
        let signed = bobix_link::Fields::new("bobix-push-grant-v1").field(b"tok").field(b"relay.example").num(1_800_000_000_000).done();
        assert!(keys::verify_auth(&id.public().auth, &signed, &raw[40..]));
        assert_eq!(make_grant(vec![1; 5], vec![2; 32], "t".into(), "r".into(), 1).err(), Some(OpenNoticeError::InvalidKey));
    }

    #[test]
    fn a_counter_too_far_below_the_window_is_refused() {
        let first = open(seal(5000, Direction::ToPhone, Stream::Event, &notice("waiting")), &fresh()).unwrap();
        let late = open(seal(5000 - counters::EVENT_WINDOW, Direction::ToPhone, Stream::Event, &notice("closed")), &first.state);
        assert_eq!(late.err(), Some(OpenNoticeError::Replayed));
    }
}
