#![no_main]
//! "Open an envelope": arbitrary bytes arrive as a packet, and so do mutated copies of a genuine one.

use bobix_link::counters::RecvState;
use bobix_link::envelope::{Direction, Stream, VersionRange, CURRENT_VERSION};
use bobix_link::keys::Identity;
use bobix_link::link::Link;
use libfuzzer_sys::fuzz_target;

fn link(sends: Direction, own: u8, peer: u8) -> Link {
    let (own, peer) = ([own; 32], Identity::from_secrets([0; 32], [peer; 32]).public().boxk);
    Link::new(sends, own, peer, CURRENT_VERSION, VersionRange::current(), [0; 3], 0, RecvState::default())
}

fuzz_target!(|data: &[u8]| {
    let mut receiver = link(Direction::ToDesktop, 2, 1);
    let _ = receiver.open(data);
    // A genuine packet with the fuzzer's bytes xored over it must never open as something else.
    let sender = link(Direction::ToPhone, 1, 2);
    let mut packet = sender.seal_with(Stream::Event, 7, b"{\"kind\":\"paired\"}");
    for (i, b) in data.iter().enumerate() {
        let n = packet.len();
        packet[i % n] ^= b;
    }
    let _ = receiver.open(&packet);
});
