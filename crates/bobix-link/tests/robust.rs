//! Garbage in, no panic out: the same entry points the fuzz targets feed, driven by a fixed pseudo-random sequence
//! so a plain `cargo test` covers them everywhere.

use bobix_link::counters::RecvState;
use bobix_link::envelope::{Direction, Stream, VersionRange, CURRENT_VERSION};
use bobix_link::keys::Identity;
use bobix_link::link::Link;
use bobix_link::messages::{DesktopMsg, PhoneMsg};
use bobix_link::pairing::{PairingCode, split_hello_body};
use bobix_link::relay_frames::Frame;

fn bytes(seed: &mut u64, n: usize) -> Vec<u8> {
    (0..n)
        .map(|_| {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 7;
            *seed ^= *seed << 17;
            (*seed >> 24) as u8
        })
        .collect()
}

#[test]
fn random_input_never_panics() {
    let (d, p) = (Identity::generate(), Identity::generate());
    let mut link = Link::new(Direction::ToDesktop, *p.box_secret(), d.public().boxk, CURRENT_VERSION, VersionRange::current(), [0; 3], 0, RecvState::default());
    let good = Link::new(Direction::ToPhone, *d.box_secret(), p.public().boxk, CURRENT_VERSION, VersionRange::current(), [0; 3], 0, RecvState::default());
    let mut seed = 0x9e3779b97f4a7c15u64;
    for i in 0..5000 {
        let n = (i % 97) + (i % 5) * 40;
        let mut b = bytes(&mut seed, n);
        // Make some inputs look like packets so the later checks run too.
        if i % 3 == 0 && b.len() > 2 {
            b[0] = CURRENT_VERSION;
            b[1] = (i % 4) as u8;
        }
        let _ = link.open(&b);
        let text = String::from_utf8_lossy(&b).into_owned();
        let _ = Frame::parse(&text);
        let _ = DesktopMsg::from_json(&b);
        let _ = PhoneMsg::from_json(&b);
        let _ = PairingCode::parse(&text);
        let _ = split_hello_body(&b);
    }
    // And a genuine packet still opens afterwards.
    assert!(link.open(&good.seal_with(Stream::Event, 1, b"{}")).is_ok());
}

#[test]
fn truncated_and_extended_packets_never_panic() {
    let (d, p) = (Identity::generate(), Identity::generate());
    let sender = Link::new(Direction::ToPhone, *d.box_secret(), p.public().boxk, CURRENT_VERSION, VersionRange::current(), [0; 3], 0, RecvState::default());
    let mut receiver = Link::new(Direction::ToDesktop, *p.box_secret(), d.public().boxk, CURRENT_VERSION, VersionRange::current(), [0; 3], 0, RecvState::default());
    let packet = sender.seal_with(Stream::Event, 5, b"payload");
    for cut in 0..packet.len() {
        assert!(receiver.open(&packet[..cut]).is_err());
    }
    let mut longer = packet.clone();
    longer.push(0);
    assert!(receiver.open(&longer).is_err());
    assert!(receiver.open(&packet).is_ok());
}

#[test]
fn a_used_receive_state_survives_garbage_and_mutated_packets() {
    let (d, p) = (Identity::generate(), Identity::generate());
    let sender = Link::new(Direction::ToPhone, *d.box_secret(), p.public().boxk, CURRENT_VERSION, VersionRange::current(), [0; 3], 0, RecvState::default());
    let mut receiver = Link::new(Direction::ToDesktop, *p.box_secret(), d.public().boxk, CURRENT_VERSION, VersionRange::current(), [0; 3], 0, RecvState::default());
    for c in [5_000u64, 5_001, 9_000] {
        receiver.open(&sender.seal_with(Stream::Event, c, b"{}")).unwrap();
        receiver.open(&sender.seal_with(Stream::State, c, b"{}")).unwrap();
    }
    let packet = sender.seal_with(Stream::Event, 9_001, b"{\"k\":1}");
    let mut seed = 0x1234_5678_9abc_def1u64;
    for i in 0..3000 {
        let mut m = packet.clone();
        let flips = bytes(&mut seed, 3);
        for (k, f) in flips.iter().enumerate() {
            let at = (*f as usize * (i + k + 1)) % m.len();
            m[at] ^= 1 << (f % 8);
        }
        if let Ok(o) = receiver.open(&m) {
            assert_eq!((o.plaintext.as_slice(), o.counter), (b"{\"k\":1}".as_slice(), 9_001));
        }
    }
}
