//! One direction pair of a pairing, seen from one device: seals what it sends, opens what it receives, and keeps
//! the counters of both. No file I/O: the caller stores `Sealed::reserve` before it sends the packet, and the
//! `RecvState` whenever it changes (a device that lost it accepts one old packet once; commands stay bound by
//! `issuedAt`, `cid`, the `shown` hash and the open-item check).

use crate::counters::{RecvState, SendCounter};
use crate::envelope::{self, Direction, Lane, OpenError, Stream, VersionRange};
use crypto_box::{PublicKey, SecretKey};
use zeroize::Zeroizing;

pub struct Sealed {
    pub packet: Vec<u8>,
    pub counter: u64,
    /// When `Some((stream, value))`: store `value` as the reservation of `stream` before the packet leaves.
    pub reserve: Option<(Stream, u64)>,
}

#[derive(Debug)]
pub struct Opened {
    pub stream: Stream,
    pub counter: u64,
    pub version: u8,
    pub plaintext: Vec<u8>,
}

pub struct Link {
    /// The direction this device sends in. It refuses packets that travel in that direction.
    sends: Direction,
    secret: Zeroizing<[u8; 32]>,
    peer: [u8; 32],
    accepted: VersionRange,
    /// The version used for sealing.
    version: u8,
    counters: [SendCounter; 3],
    recv: RecvState,
}

impl Link {
    /// `reservations` are the values last stored for `state`, `event`, `activity` (0 when none), `now_unix_ms` the
    /// clock; `recv` is the stored receive state (`RecvState::default()` when none).
    pub fn new(
        sends: Direction,
        box_secret: [u8; 32],
        peer_box_public: [u8; 32],
        version: u8,
        accepted: VersionRange,
        reservations: [u64; 3],
        now_unix_ms: u64,
        recv: RecvState,
    ) -> Self {
        Link {
            sends,
            secret: Zeroizing::new(box_secret),
            peer: peer_box_public,
            accepted,
            version,
            counters: reservations.map(|r| SendCounter::resume(r, now_unix_ms)),
            recv,
        }
    }

    pub fn recv_state(&self) -> &RecvState {
        &self.recv
    }

    pub fn seal(&mut self, stream: Stream, plaintext: &[u8]) -> Sealed {
        let (counter, reserve) = self.counters[stream as usize].next();
        let packet = self.seal_with(stream, counter, plaintext);
        Sealed { packet, counter, reserve: reserve.map(|v| (stream, v)) }
    }

    /// Seals with a counter the caller chose. For tools that play a late or replayed packet; a device never calls it.
    pub fn seal_with(&self, stream: Stream, counter: u64, plaintext: &[u8]) -> Vec<u8> {
        let lane = Lane { dir: self.sends, stream };
        envelope::seal(
            self.version,
            lane,
            counter,
            &SecretKey::from(*self.secret),
            &PublicKey::from(self.peer),
            plaintext,
        )
    }

    /// Opens a packet from the peer. The counter rule is checked before the open and committed only after the packet
    /// authenticated, so a forged packet cannot move the window. A packet outside its stream's rule is `Dropped`.
    pub fn open(&mut self, packet: &[u8]) -> Result<Opened, OpenError> {
        let h = envelope::peek(&self.accepted, self.sends, packet)?;
        if !self.recv.check(h.lane.stream, h.counter) {
            return Err(OpenError::Dropped);
        }
        let plaintext = envelope::open(&SecretKey::from(*self.secret), &PublicKey::from(self.peer), packet)?;
        self.recv.commit(h.lane.stream, h.counter);
        Ok(Opened { stream: h.lane.stream, counter: h.counter, version: h.version, plaintext })
    }
}
