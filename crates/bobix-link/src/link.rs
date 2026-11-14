//! One direction pair of a pairing, seen from one device: seals what it sends, opens what it receives, and keeps
//! the counters of both. No file I/O: the caller stores `Sealed::reserve` before it sends the packet, and the
//! `RecvState` whenever it changes (a device that lost it accepts one old packet once; commands stay bound by
//! `issuedAt`, `cid`, the `shown` hash and the open-item check).
//!
//! Keep the `Link` (or at least its `RecvState`) that opened the pairing `hello`: a sender's counters start at its
//! clock, so the `hello`'s counter is what puts replays of packets from an earlier pairing with the same keys below
//! the window. A fresh `RecvState` at confirm time would accept one such old packet (an old `bye`, an old `revoked`)
//! once.

use crate::counters::{RecvState, SendCounter};
use crate::envelope::{self, Direction, Lane, OpenError, Stream, VersionRange};
use crypto_box::{PublicKey, SecretKey};
use zeroize::Zeroizing;

/// Why a packet was not sealed.
#[derive(Debug, PartialEq, Eq)]
pub enum SealError {
    /// The peer's box key is of low order: the shared secret would be all zeros, readable and forgeable by anyone.
    LowOrderPeer,
}

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
    /// Checked once: the peer key is a small-order point, so nothing can authenticate.
    peer_low_order: bool,
    accepted: VersionRange,
    /// The version used for sealing.
    version: u8,
    counters: [SendCounter; 3],
    recv: RecvState,
}

#[allow(clippy::too_many_arguments)]
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
            peer_low_order: crate::keys::is_low_order(&peer_box_public),
            accepted,
            version,
            counters: reservations.map(|r| SendCounter::resume(r, now_unix_ms)),
            recv,
        }
    }

    pub fn recv_state(&self) -> &RecvState {
        &self.recv
    }

    /// Takes the next counter of a stream, for a message that must contain its own counter (a signed command). The
    /// caller seals with `seal_with` and must store `reserve` first. `seal` does both steps for the common case.
    pub fn reserve_counter(&mut self, stream: Stream) -> (u64, Option<(Stream, u64)>) {
        let (counter, reserve) = self.counters[stream as usize].take();
        (counter, reserve.map(|v| (stream, v)))
    }

    /// `Err(SealError::LowOrderPeer)` when the peer's box key is a small-order point: nothing is sealed to it.
    pub fn seal(&mut self, stream: Stream, plaintext: &[u8]) -> Result<Sealed, SealError> {
        let (counter, reserve) = self.reserve_counter(stream);
        Ok(Sealed { packet: self.seal_with(stream, counter, plaintext)?, counter, reserve })
    }

    /// Seals with a counter the caller took with `reserve_counter`, or chose to play a late or replayed packet.
    pub fn seal_with(&self, stream: Stream, counter: u64, plaintext: &[u8]) -> Result<Vec<u8>, SealError> {
        if self.peer_low_order {
            return Err(SealError::LowOrderPeer);
        }
        let lane = Lane { dir: self.sends, stream };
        Ok(envelope::seal(
            self.version,
            lane,
            counter,
            &SecretKey::from(*self.secret),
            &PublicKey::from(self.peer),
            plaintext,
        ))
    }

    /// Opens a packet from the peer. The counter rule is checked before the open and committed only after the packet
    /// authenticated, so a forged packet cannot move the window. A packet outside its stream's rule is `Dropped`. The
    /// version is judged only after the open: an unauthenticated byte never says which side must update.
    pub fn open(&mut self, packet: &[u8]) -> Result<Opened, OpenError> {
        let h = envelope::peek(self.sends, packet)?;
        if !self.recv.check(h.lane.stream, h.counter) {
            return Err(OpenError::Dropped);
        }
        if self.peer_low_order {
            return Err(OpenError::Decrypt);
        }
        let plaintext = envelope::open(&SecretKey::from(*self.secret), &PublicKey::from(self.peer), packet)?;
        envelope::check_version(&self.accepted, h.version)?;
        self.recv.commit(h.lane.stream, h.counter);
        Ok(Opened { stream: h.lane.stream, counter: h.counter, version: h.version, plaintext })
    }
}
