#![no_main]
//! "Parse a relay frame" (and the other text a peer controls: a pairing code, a message).

use bobix_link::messages::{DesktopMsg, PhoneMsg};
use bobix_link::pairing::PairingCode;
use bobix_link::relay_frames::Frame;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        if let Ok(f) = Frame::parse(text) {
            // Whatever parsed must serialize and parse back to the same frame.
            if let Ok(again) = f.to_text() {
                assert_eq!(Frame::parse(&again), Ok(f));
            }
        }
        let _ = PairingCode::parse(text);
    }
    let _ = DesktopMsg::from_json(data);
    let _ = PhoneMsg::from_json(data);
});
