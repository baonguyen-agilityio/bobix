//! The success criterion: the relay never holds a key that opens a packet. Packets travel through the real relay
//! between real `bobix-link` devices; a hostile device the phone allowed (or the relay itself) alters, replays,
//! reflects, re-routes or ages a packet, and the receiving device refuses every one.

mod common;

use bobix_link::counters::RecvState;
use bobix_link::envelope::{Direction, OpenError, Stream, VersionRange, CURRENT_VERSION};
use bobix_link::keys::Identity;
use bobix_link::link::Link;
use bobix_link::relay_frames::*;
use bobix_link::{b64, b64_decode};
use common::*;

fn link(sends: Direction, me: &Identity, peer_box: [u8; 32]) -> Link {
    Link::new(sends, *me.box_secret(), peer_box, CURRENT_VERSION, VersionRange::current(), [0; 3], 0, RecvState::default())
}

fn body_of(f: Frame) -> Vec<u8> {
    let Frame::Msg { body, .. } = f else { panic!("not a msg") };
    b64_decode(&body).expect("the body is base64")
}

fn is_msg(f: &Frame) -> bool {
    matches!(f, Frame::Msg { .. })
}

/// Seals with the desktop's key for the phone, sends it through the relay, and returns what the phone's socket got.
async fn through_relay(from: &mut Dev, to: &mut Dev, pid: &str, packet: &[u8]) -> Vec<u8> {
    from.send(&send(&to.id(), pid, 600, None, &b64(packet))).await;
    body_of(to.expect(is_msg, 1000).await)
}

#[tokio::test]
async fn altered_replayed_reflected_rerouted_and_old_packets_are_refused_by_the_receiver() {
    let s = start(cfg()).await;
    let (mut desktop, mut phone) = pair_up(&s).await;
    let mut mallory = Dev::connect(&s, ident()).await; // allowed by the phone, so the relay carries its packets
    phone.send(&Frame::Peers { allow: vec![desktop.id(), mallory.id()] }).await;
    phone.sync().await;
    let mut other_phone = Dev::connect(&s, ident()).await;
    other_phone.send(&Frame::Peers { allow: vec![desktop.id(), mallory.id()] }).await;
    other_phone.sync().await;

    let (d, p, o) = (desktop.ident.clone(), phone.ident.clone(), other_phone.ident.clone());
    let mut d_to_p = link(Direction::ToPhone, &d, p.public().boxk);
    let mut p_from_d = link(Direction::ToDesktop, &p, d.public().boxk);
    let mut p_to_d = link(Direction::ToDesktop, &p, d.public().boxk);
    let mut d_from_p = link(Direction::ToPhone, &d, p.public().boxk);
    let mut o_from_d = link(Direction::ToDesktop, &o, d.public().boxk);

    // The honest path works, so the refusals below are the packet's doing and not the setup's.
    let first = d_to_p.seal(Stream::State, b"home-1").packet;
    let got = through_relay(&mut desktop, &mut phone, "ok", &first).await;
    assert_eq!(got, first, "the relay forwards the packet byte for byte");
    assert_eq!(p_from_d.open(&got).unwrap().plaintext, b"home-1");

    // Replayed: the same packet again (mallory re-sends what it captured).
    let replayed = through_relay(&mut mallory, &mut phone, "replay", &first).await;
    assert_eq!(p_from_d.open(&replayed).unwrap_err(), OpenError::Dropped);

    // Altered: one bit flipped on the way.
    let second = d_to_p.seal(Stream::State, b"home-2").packet;
    let mut altered = second.clone();
    let last = altered.len() - 1;
    altered[last] ^= 1;
    let got = through_relay(&mut mallory, &mut phone, "altered", &altered).await;
    assert_eq!(p_from_d.open(&got).unwrap_err(), OpenError::Decrypt);
    // ... and the untouched one still opens: a forged packet did not move the counter window.
    assert!(p_from_d.open(&second).is_ok());

    // Old: a packet with a counter below the highest the phone has taken on the `state` stream.
    let newest = d_to_p.seal(Stream::State, b"home-9").packet;
    let older = d_to_p.seal_with(Stream::State, 1, b"home-old");
    p_from_d.open(&newest).unwrap();
    let got = through_relay(&mut mallory, &mut phone, "old", &older).await;
    assert_eq!(p_from_d.open(&got).unwrap_err(), OpenError::Dropped);

    // Reflected: the phone's own packet (made for the desktop) sent back to the phone.
    let mine = p_to_d.seal(Stream::Event, b"command").packet;
    let got = through_relay(&mut mallory, &mut phone, "reflect", &mine).await;
    assert_eq!(p_from_d.open(&got).unwrap_err(), OpenError::Reflected);
    assert!(d_from_p.open(&mine).is_ok(), "while the desktop it was meant for opens it");

    // Re-routed: a packet the desktop made for the first phone, delivered to the second one.
    let for_p = d_to_p.seal(Stream::Event, b"for the first phone").packet;
    let got = through_relay(&mut mallory, &mut other_phone, "reroute", &for_p).await;
    assert_eq!(o_from_d.open(&got).unwrap_err(), OpenError::Decrypt);

    // A packet that is not one, or a truncated one, is refused too.
    assert!(p_from_d.open(b"junk").is_err());
}

#[tokio::test]
async fn the_relay_sources_hold_no_code_that_could_open_a_packet() {
    // The relay moves opaque strings. Nothing in it may reference the sealing API or a box key; the only crypto it
    // calls is the signature check on `auth` and on push grants.
    let forbidden = ["Link", "envelope::", "box_secret", "crypto_box", "Identity", "SecretKey"];
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).unwrap().flatten() {
        let text = std::fs::read_to_string(entry.path()).unwrap();
        for word in forbidden {
            assert!(!text.contains(word), "{} mentions {word}", entry.path().display());
        }
    }
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    assert!(!manifest.contains("crypto_box") && !manifest.contains("chacha"), "no box crypto among the relay's own dependencies");
}
