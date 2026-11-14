use bobix_link::counters::{RecvState, SendCounter, EVENT_WINDOW, RESERVE_BLOCK};
use bobix_link::envelope::{Direction, OpenError, Stream, Update, VersionRange, CURRENT_VERSION};
use bobix_link::keys::{soft::SoftSigner, verify_auth, Identity, SignError, SignPublic};
use bobix_link::link::Link;

const NOW: u64 = 1_800_000_000_000;

/// A paired desktop and phone link.
fn pair() -> (Link, Link) {
    let (d, p) = (Identity::generate(), Identity::generate());
    let r = VersionRange::current();
    let desktop = Link::new(Direction::ToPhone, *d.box_secret(), p.public().boxk, CURRENT_VERSION, r, [0; 3], NOW, RecvState::default());
    let phone = Link::new(Direction::ToDesktop, *p.box_secret(), d.public().boxk, CURRENT_VERSION, r, [0; 3], NOW, RecvState::default());
    (desktop, phone)
}

#[test]
fn round_trip_both_ways() {
    let (mut d, mut p) = pair();
    let s = d.seal(Stream::Event, b"hello phone").unwrap();
    assert_eq!(p.open(&s.packet).unwrap().plaintext, b"hello phone");
    let s = p.seal(Stream::Event, b"hello desktop").unwrap();
    assert_eq!(d.open(&s.packet).unwrap().plaintext, b"hello desktop");
}

#[test]
fn wrong_recipient_does_not_open() {
    let (mut d, _) = pair();
    let (_, mut other_phone) = pair();
    let s = d.seal(Stream::Event, b"x").unwrap();
    assert_eq!(other_phone.open(&s.packet).unwrap_err(), OpenError::Decrypt);
}

#[test]
fn any_altered_byte_does_not_open() {
    let (mut d, mut p) = pair();
    let s = d.seal(Stream::Event, b"payload").unwrap();
    // Version (0), lane (1), counter (2..10), random part (10..24), ciphertext and tag.
    for i in 0..s.packet.len() {
        let mut bad = s.packet.clone();
        bad[i] ^= 1;
        assert!(p.open(&bad).is_err(), "byte {i} changed and the packet still opened");
    }
    assert!(p.open(&s.packet).is_ok(), "failed attempts must not use up the counter");
}

#[test]
fn a_changed_lane_or_counter_does_not_open_even_when_it_passes_the_rules() {
    let (mut d, mut p) = pair();
    let s = d.seal(Stream::Event, b"payload").unwrap();
    let mut other_stream = s.packet.clone();
    other_stream[1] = Stream::Activity as u8; // still towards the phone, another stream
    assert_eq!(p.open(&other_stream).unwrap_err(), OpenError::Decrypt);
    let mut higher = s.packet.clone();
    higher[9] = higher[9].wrapping_add(1);
    assert_eq!(p.open(&higher).unwrap_err(), OpenError::Decrypt);
}

#[test]
fn a_forged_packet_does_not_move_the_window() {
    let (mut d, mut p) = pair();
    let good = d.seal(Stream::Event, b"a").unwrap();
    let mut forged = good.packet.clone();
    forged[2..10].copy_from_slice(&u64::MAX.to_be_bytes());
    assert!(p.open(&forged).is_err());
    assert!(p.open(&good.packet).is_ok());
}

#[test]
fn replayed_counter_is_refused() {
    let (mut d, mut p) = pair();
    let s = d.seal(Stream::Event, b"once").unwrap();
    assert!(p.open(&s.packet).is_ok());
    assert_eq!(p.open(&s.packet).unwrap_err(), OpenError::Dropped);
}

#[test]
fn reflected_packet_is_refused() {
    let (mut d, mut p) = pair();
    let s = d.seal(Stream::Event, b"mine").unwrap();
    assert_eq!(d.open(&s.packet).unwrap_err(), OpenError::Reflected);
    let s = p.seal(Stream::Event, b"mine").unwrap();
    assert_eq!(p.open(&s.packet).unwrap_err(), OpenError::Reflected);
}

#[test]
fn unknown_version_or_lane_is_refused_without_a_panic() {
    let (mut d, mut p) = pair();
    let s = d.seal(Stream::Event, b"x").unwrap();
    // An unauthenticated version byte never says which side must update: it is just a packet that does not open.
    let mut v = s.packet.clone();
    v[0] = 99;
    assert_eq!(p.open(&v).unwrap_err(), OpenError::Decrypt);
    v[0] = 0;
    assert_eq!(p.open(&v).unwrap_err(), OpenError::Decrypt);
    let junk: Vec<u8> = [0xFF, 0x01].into_iter().chain(std::iter::repeat(7).take(60)).collect();
    assert_eq!(p.open(&junk).unwrap_err(), OpenError::Decrypt);
    let mut l = s.packet.clone();
    l[1] = 0x07;
    assert_eq!(p.open(&l).unwrap_err(), OpenError::Lane);
    assert_eq!(p.open(&[]).unwrap_err(), OpenError::Short);
    assert_eq!(p.open(&s.packet[..30]).unwrap_err(), OpenError::Short);
}

#[test]
fn event_window_rules() {
    let (d, mut p) = pair();
    let ev = |c: u64| d.seal_with(Stream::Event, c, b"e").unwrap();
    let base = 100_000;
    assert!(p.open(&ev(base + 10)).is_ok());
    // out of order inside the window: accepted once
    assert!(p.open(&ev(base + 5)).is_ok());
    assert_eq!(p.open(&ev(base + 5)).unwrap_err(), OpenError::Dropped);
    assert_eq!(p.open(&ev(base + 10)).unwrap_err(), OpenError::Dropped);
    assert!(p.open(&ev(base + 11)).is_ok());
    // far below the window: dropped
    assert!(p.open(&ev(base + 5000)).is_ok());
    assert_eq!(p.open(&ev(base + 5000 - EVENT_WINDOW)).unwrap_err(), OpenError::Dropped);
    assert_eq!(p.open(&ev(base + 100)).unwrap_err(), OpenError::Dropped);
    // the edge of the window still opens
    assert!(p.open(&ev(base + 5000 - EVENT_WINDOW + 1)).is_ok());
    // and the link is still alive
    assert!(p.open(&ev(base + 5001)).is_ok());
}

#[test]
fn state_accepts_only_above_the_highest() {
    let (d, mut p) = pair();
    let st = |c: u64| d.seal_with(Stream::State, c, b"home").unwrap();
    assert!(p.open(&st(50)).is_ok());
    assert_eq!(p.open(&st(40)).unwrap_err(), OpenError::Dropped);
    assert_eq!(p.open(&st(50)).unwrap_err(), OpenError::Dropped);
    assert!(p.open(&st(51)).is_ok());
}

#[test]
fn activity_accepts_the_same_counter_twice_but_not_a_lower_one() {
    let (d, mut p) = pair();
    let ac = |c: u64| d.seal_with(Stream::Activity, c, b"live").unwrap();
    let pkt = ac(7);
    assert!(p.open(&pkt).is_ok());
    assert!(p.open(&pkt).is_ok());
    assert!(p.open(&ac(8)).is_ok());
    assert_eq!(p.open(&ac(7)).unwrap_err(), OpenError::Dropped);
}

#[test]
fn streams_do_not_share_counters() {
    let (mut d, mut p) = pair();
    // a long run of `home` must not make a later event look old
    for _ in 0..3000 {
        let s = d.seal(Stream::State, b"home").unwrap();
        p.open(&s.packet).unwrap();
    }
    let e = d.seal_with(Stream::Event, 1, b"push").unwrap();
    assert!(p.open(&e).is_ok());
}

#[test]
fn receive_state_survives_a_restart_through_serde() {
    let (d, mut p) = pair();
    p.open(&d.seal_with(Stream::Event, 500, b"x").unwrap()).unwrap();
    let saved = serde_json::to_string(p.recv_state()).unwrap();
    let restored: RecvState = serde_json::from_str(&saved).unwrap();
    assert_eq!(&restored, p.recv_state());
    assert!(!restored.check(Stream::Event, 500));
    assert!(restored.check(Stream::Event, 501));
}

#[test]
fn send_counter_reserves_blocks_and_never_goes_backwards() {
    // A fresh counter starts at the clock and asks to store a reservation before the first packet.
    let mut c = SendCounter::resume(0, NOW);
    assert_eq!(c.take(), (NOW, Some(NOW + RESERVE_BLOCK)));
    assert_eq!(c.take(), (NOW + 1, None));
    let mut last = 0;
    let mut stored = NOW + RESERVE_BLOCK;
    for _ in 0..3 * RESERVE_BLOCK {
        let (n, res) = c.take();
        if let Some(r) = res {
            assert!(r > n);
            stored = r;
        }
        last = n;
    }
    assert!(stored > last, "every counter used is below the stored reservation");
    // Restart with the stored reservation and a clock that went back: continues above everything sent.
    let mut again = SendCounter::resume(stored, NOW - 10_000);
    assert!(again.take().0 > last);
    // Restart after the counter file was lost: starts at the clock, above everything sent in the past.
    let mut lost = SendCounter::resume(0, last + 5_000);
    assert!(lost.take().0 > last);
}

#[test]
fn versions() {
    let ours = VersionRange { min: 2, max: 3 };
    assert_eq!(ours.negotiate(&VersionRange { min: 1, max: 2 }), Ok(2)); // one behind works
    assert_eq!(ours.negotiate(&VersionRange { min: 3, max: 4 }), Ok(3)); // one ahead works
    assert_eq!(ours.negotiate(&VersionRange { min: 1, max: 1 }), Err(Update::Peer)); // two behind
    assert_eq!(ours.negotiate(&VersionRange { min: 4, max: 5 }), Err(Update::Us)); // two ahead
    assert_eq!(VersionRange::current().max, CURRENT_VERSION);
}

#[test]
fn identity_and_device_id() {
    let a = Identity::generate();
    let a2 = Identity::from_secrets(*a.auth_seed(), *a.box_secret());
    assert_eq!(a.public(), a2.public());
    let id = a.device_id();
    assert_eq!(id.len(), 32);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    let sig = a.sign_auth(b"challenge");
    assert!(verify_auth(&a.public().auth, b"challenge", &sig));
    assert!(!verify_auth(&a.public().auth, b"other", &sig));
    assert!(!verify_auth(&a.public().auth, b"challenge", &sig[..10]));
}

#[test]
fn p256_signatures() {
    let s = SoftSigner::generate();
    let public = s.public();
    let der = s.sign(b"msg");
    assert_eq!(public.verify(b"msg", &der), Ok(()));
    assert_eq!(public.verify(b"msg!", &der), Err(SignError::Invalid));
    assert_eq!(public.verify(b"msg", &der[..der.len() - 1]), Err(SignError::Encoding));
    assert_eq!(public.verify(b"msg", &[0u8; 64]), Err(SignError::Encoding)); // raw r|s is not DER
    assert_eq!(SignPublic::from_sec1(&public.to_sec1()).unwrap(), public);
    assert!(SignPublic::from_sec1(&[2u8; 5]).is_err());
    let other = SoftSigner::generate();
    assert_eq!(other.public().verify(b"msg", &der), Err(SignError::Invalid));
}

#[test]
fn p256_high_s_signature_verifies() {
    use p256::ecdsa::Signature;
    let s = SoftSigner::generate();
    let any = Signature::from_der(&s.sign(b"msg")).unwrap();
    let low = any.normalize_s();
    // Flip S to n - S: the hardware signers do not normalise, so both forms must verify.
    let (r, sv) = low.split_scalars();
    let high = Signature::from_scalars(r, -*sv).unwrap();
    assert_ne!(high, high.normalize_s(), "built a high-S signature");
    assert_eq!(s.public().verify(b"msg", high.to_der().as_bytes()), Ok(()));
}

fn link_with(version: u8, sends: Direction, own: &Identity, peer: &Identity) -> Link {
    Link::new(sends, *own.box_secret(), peer.public().boxk, version, VersionRange::current(), [0; 3], NOW, RecvState::default())
}

#[test]
fn a_version_outside_the_range_is_reported_only_for_an_authenticated_packet() {
    let (d, p) = (Identity::generate(), Identity::generate());
    let mut phone = link_with(CURRENT_VERSION, Direction::ToDesktop, &p, &d);
    let too_new = link_with(CURRENT_VERSION + 5, Direction::ToPhone, &d, &p);
    let packet = too_new.seal_with(Stream::Event, 1, b"{}").unwrap();
    assert_eq!(phone.open(&packet).unwrap_err(), OpenError::Version(Update::Us), "we must update");
    let too_old = link_with(0, Direction::ToPhone, &d, &p);
    assert_eq!(phone.open(&too_old.seal_with(Stream::Event, 2, b"{}").unwrap()).unwrap_err(), OpenError::Version(Update::Peer));
    // a stranger's packet with the same version byte says nothing
    let stranger = link_with(CURRENT_VERSION + 5, Direction::ToPhone, &Identity::generate(), &p);
    assert_eq!(phone.open(&stranger.seal_with(Stream::Event, 3, b"{}").unwrap()).unwrap_err(), OpenError::Decrypt);
}

#[test]
fn low_order_box_keys_are_refused() {
    use bobix_link::keys::{box_public, is_low_order};
    let mut points = vec![[0u8; 32], {
        let mut one = [0u8; 32];
        one[0] = 1;
        one
    }];
    for (head, tail) in [(0xecu8, 0x7fu8), (0xed, 0x7f), (0xee, 0x7f)] {
        let mut p = [0xffu8; 32];
        p[0] = head;
        p[31] = tail;
        points.push(p);
    }
    for pt in &points {
        assert!(is_low_order(pt), "{pt:?}");
        assert!(box_public(*pt).is_none());
    }
    let ok = Identity::generate().public().boxk;
    assert!(!is_low_order(&ok) && box_public(ok).is_some());
    // a link whose peer key is low-order opens nothing, even a packet that the zero shared secret would "authenticate"
    use bobix_link::envelope::{seal, Lane};
    let p = Identity::generate();
    // Sealing to a low-order key is refused too.
    let mut sealing = Link::new(Direction::ToPhone, *p.box_secret(), [0; 32], CURRENT_VERSION, VersionRange::current(), [0; 3], NOW, RecvState::default());
    assert_eq!(sealing.seal(Stream::Event, b"{}").err(), Some(bobix_link::link::SealError::LowOrderPeer));
    assert_eq!(sealing.seal_with(Stream::Event, 1, b"{}").err(), Some(bobix_link::link::SealError::LowOrderPeer));
    let mut zero_peer = Link::new(Direction::ToDesktop, *p.box_secret(), [0; 32], CURRENT_VERSION, VersionRange::current(), [0; 3], NOW, RecvState::default());
    // Sealed to the zero key, the shared secret is all zeros: anyone can make this packet, and without the check it would open.
    let lane = Lane { dir: Direction::ToPhone, stream: Stream::Event };
    let forged = seal(CURRENT_VERSION, lane, 1, &crypto_box::SecretKey::from([5u8; 32]), &crypto_box::PublicKey::from([0u8; 32]), b"{}");
    assert_eq!(zero_peer.open(&forged).unwrap_err(), OpenError::Decrypt);
}

#[test]
fn a_corrupt_stored_receive_state_is_an_error_not_a_panic() {
    let good = serde_json::to_value(RecvState::default()).unwrap();
    assert!(serde_json::from_value::<RecvState>(good).is_ok());
    let bad = r#"{"state":{"highest":null,"seen":[]},"event":{"highest":5,"seen":[1,2,3]},"activity":{"highest":null,"seen":[]}}"#;
    assert!(serde_json::from_str::<RecvState>(bad).is_err());
    // and a state that was used round-trips
    let (d, mut p) = pair();
    p.open(&d.seal_with(Stream::Event, 77, b"{}").unwrap()).unwrap();
    let again: RecvState = serde_json::from_str(&serde_json::to_string(p.recv_state()).unwrap()).unwrap();
    assert!(!again.check(Stream::Event, 77));
}

#[test]
fn a_send_counter_near_the_top_of_u64_does_not_overflow() {
    for stored in [u64::MAX, u64::MAX - 3, u64::MAX / 2 + 1] {
        let mut c = SendCounter::resume(stored, NOW);
        let (first, _) = c.take();
        assert!((NOW..u64::MAX / 2).contains(&first), "a corrupt reservation is treated as a lost file");
        c.take();
    }
    // even a clock at the very top cannot panic
    let mut c = SendCounter::resume(0, u64::MAX - 1);
    for _ in 0..4 {
        c.take();
    }
}
