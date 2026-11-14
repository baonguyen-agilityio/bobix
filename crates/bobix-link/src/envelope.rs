//! The sealed packet: `nonce (24 bytes) | ciphertext`.
//!
//! nonce = `version (1) | lane (1) | counter (u64, big endian) | 14 random bytes`. The nonce is the AEAD nonce, so a
//! packet in which the version, the lane or the counter was changed does not open. The ciphertext is
//! ChaChaBox (XChaCha20-Poly1305) from the sender's box secret key to the recipient's box public key.

use crate::util::random;
use crypto_box::{
    aead::{generic_array::GenericArray, Aead},
    ChaChaBox, PublicKey, SecretKey,
};
use serde::{Deserialize, Serialize};

/// The protocol version this build speaks.
pub const CURRENT_VERSION: u8 = 1;
pub const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;

/// The versions a side understands: the current one and the one before.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionRange {
    pub min: u8,
    pub max: u8,
}

/// Which side has to update when two ranges do not meet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Update {
    /// The peer is too old for us.
    Peer,
    /// We are too old for the peer.
    Us,
}

impl VersionRange {
    pub const fn current() -> Self {
        VersionRange { min: if CURRENT_VERSION > 1 { CURRENT_VERSION - 1 } else { 1 }, max: CURRENT_VERSION }
    }

    pub fn contains(&self, v: u8) -> bool {
        self.min <= v && v <= self.max
    }

    /// The version two sides talk: the highest both understand.
    pub fn negotiate(&self, peer: &VersionRange) -> Result<u8, Update> {
        if peer.max < self.min {
            Err(Update::Peer)
        } else if peer.min > self.max {
            Err(Update::Us)
        } else {
            Ok(self.max.min(peer.max))
        }
    }

    /// Which side must update for a packet that carries version `v`.
    pub fn outside(&self, v: u8) -> Update {
        if v < self.min {
            Update::Peer
        } else {
            Update::Us
        }
    }
}

/// Who a packet travels to. Bit 7 of the lane byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    ToPhone,
    ToDesktop,
}

impl Direction {
    pub fn opposite(self) -> Self {
        match self {
            Direction::ToPhone => Direction::ToDesktop,
            Direction::ToDesktop => Direction::ToPhone,
        }
    }
}

/// `state` carries `home` only; `event` every other message; `activity` Live Activity and Live Update content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stream {
    State = 0,
    Event = 1,
    Activity = 2,
}

impl Stream {
    pub const ALL: [Stream; 3] = [Stream::State, Stream::Event, Stream::Activity];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lane {
    pub dir: Direction,
    pub stream: Stream,
}

impl Lane {
    pub fn to_byte(self) -> u8 {
        (if self.dir == Direction::ToDesktop { 0x80 } else { 0 }) | self.stream as u8
    }

    pub fn from_byte(b: u8) -> Option<Lane> {
        let stream = match b & 0x7f {
            0 => Stream::State,
            1 => Stream::Event,
            2 => Stream::Activity,
            _ => return None,
        };
        Some(Lane { dir: if b & 0x80 != 0 { Direction::ToDesktop } else { Direction::ToPhone }, stream })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub version: u8,
    pub lane: Lane,
    pub counter: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum OpenError {
    /// Shorter than a nonce and a tag.
    Short,
    /// The packet authenticated but carries a version outside the accepted range; says which side must update.
    /// Never returned for a packet that did not authenticate.
    Version(Update),
    /// The lane byte is not a known direction and stream.
    Lane,
    /// The packet travels in the receiver's own direction: a relay sent a device its own packet back.
    Reflected,
    /// The counter breaks the rule of the stream (replayed, or too old). Dropped without a reply.
    Dropped,
    /// Wrong keys, or any byte of the packet changed.
    Decrypt,
}

/// Seals `plaintext` with `version`, `lane` and `counter` in the nonce.
pub fn seal(
    version: u8,
    lane: Lane,
    counter: u64,
    sender: &SecretKey,
    recipient: &PublicKey,
    plaintext: &[u8],
) -> Vec<u8> {
    seal_with_tail(version, lane, counter, random::<14>(), sender, recipient, plaintext)
}

/// `seal` with the 14 random nonce bytes given. For test vectors only: a nonce must never repeat.
pub fn seal_with_tail(
    version: u8,
    lane: Lane,
    counter: u64,
    tail: [u8; 14],
    sender: &SecretKey,
    recipient: &PublicKey,
    plaintext: &[u8],
) -> Vec<u8> {
    let mut nonce = [0u8; NONCE_LEN];
    nonce[0] = version;
    nonce[1] = lane.to_byte();
    nonce[2..10].copy_from_slice(&counter.to_be_bytes());
    nonce[10..].copy_from_slice(&tail);
    let ct = ChaChaBox::new(recipient, sender)
        .encrypt(GenericArray::from_slice(&nonce), plaintext)
        .expect("sealing in memory cannot fail");
    [nonce.as_slice(), &ct].concat()
}

/// Reads the cleartext part of a packet without opening it: length, lane, counter. `receiver_sends` is the direction
/// the receiver sends in; a packet in that direction is refused. The version byte is only reported here: nothing
/// unauthenticated may decide "which side must update", so `check_version` runs after `open` succeeded.
pub fn peek(receiver_sends: Direction, packet: &[u8]) -> Result<Header, OpenError> {
    if packet.len() < NONCE_LEN + TAG_LEN {
        return Err(OpenError::Short);
    }
    let version = packet[0];
    let lane = Lane::from_byte(packet[1]).ok_or(OpenError::Lane)?;
    if lane.dir == receiver_sends {
        return Err(OpenError::Reflected);
    }
    let counter = u64::from_be_bytes(packet[2..10].try_into().expect("8 bytes"));
    Ok(Header { version, lane, counter })
}

/// For a packet that authenticated: is its version one we understand, and if not, which side must update?
pub fn check_version(accepted: &VersionRange, version: u8) -> Result<(), OpenError> {
    if accepted.contains(version) {
        Ok(())
    } else {
        Err(OpenError::Version(accepted.outside(version)))
    }
}

/// Opens a packet that `peek` accepted. Anything that does not authenticate is `Decrypt`.
pub fn open(receiver: &SecretKey, sender: &PublicKey, packet: &[u8]) -> Result<Vec<u8>, OpenError> {
    if packet.len() < NONCE_LEN + TAG_LEN {
        return Err(OpenError::Short);
    }
    let (nonce, ct) = packet.split_at(NONCE_LEN);
    ChaChaBox::new(sender, receiver).decrypt(GenericArray::from_slice(nonce), ct).map_err(|_| OpenError::Decrypt)
}
