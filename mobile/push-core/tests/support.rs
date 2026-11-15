//! Packets made the way a desktop makes them, with the keys of the crate's own test vectors (`crates/bobix-link/tests/
//! vectors.json`): secrets are 32 bytes of one repeated value.

use bobix_link::envelope::{seal_with_tail, Direction, Lane, Stream};
use bobix_link::{b64, hex};
use crypto_box::{PublicKey, SecretKey};

pub const PHONE_BOX_SECRET: [u8; 32] = [4; 32];
pub const DESKTOP_BOX_SECRET: [u8; 32] = [2; 32];
pub const STRANGER_BOX_SECRET: [u8; 32] = [9; 32];

pub fn phone_box_public() -> [u8; 32] {
    PublicKey::from(&SecretKey::from(PHONE_BOX_SECRET)).to_bytes()
}

pub fn desktop_box_public() -> [u8; 32] {
    PublicKey::from(&SecretKey::from(DESKTOP_BOX_SECRET)).to_bytes()
}

/// A sealed packet from the desktop to the phone, with a fixed nonce tail so the fixtures never change by themselves.
pub fn packet(stream: Stream, counter: u64, version: u8, sender: [u8; 32], plaintext: &str) -> Vec<u8> {
    seal_with_tail(
        version,
        Lane { dir: Direction::ToPhone, stream },
        counter,
        [0xb0; 14],
        &SecretKey::from(sender),
        &PublicKey::from(phone_box_public()),
        plaintext.as_bytes(),
    )
}

pub fn notice(what: &str, reference: &str, title: &str, body: &str) -> String {
    serde_json::json!({"kind":"notice","what":what,"ref":reference,"session":"s1","title":title,"body":body}).to_string()
}

pub fn sealed(p: &[u8]) -> String {
    b64(p)
}

#[allow(dead_code)]
pub fn desktop_id() -> String {
    hex(&desktop_box_public()[..16])
}
