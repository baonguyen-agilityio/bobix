//! Wire format and crypto shared by the Bobix desktop app, the relay and the phone app.
//!
//! Every primitive comes from an audited RustCrypto crate; this crate only fixes how they are combined:
//! which bytes are signed, what goes into a nonce, which counter rule a stream follows.
//! There is no file or network I/O here: callers persist what the API hands back (counter reservations,
//! receive state) so the same code can sit behind flutter_rust_bridge and UniFFI.

pub mod command;
pub mod counters;
pub mod envelope;
pub mod keys;
pub mod link;
pub mod messages;
pub mod pairing;
pub mod relay_frames;
mod util;

pub use util::{b64, b64_decode, hex, random, sha256, unhex, Fields};
