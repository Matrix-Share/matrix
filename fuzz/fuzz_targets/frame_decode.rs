#![no_main]
//! Fuzz the wire-frame decoder and reassembler — the framing layer where mesh
//! messengers have been broken before (Bridgefy, USENIX 2022). Feeding arbitrary
//! bytes must never panic, over-allocate, or loop; a decoded frame must survive a
//! re-encode round-trip; and reassembly must stay bounded.

use libfuzzer_sys::fuzz_target;
use lifeline_transport::frame::{Frame, Reassembler};

fuzz_target!(|data: &[u8]| {
    if let Ok(frame) = Frame::decode(data) {
        // A frame that decoded must re-encode identically (no lossy parsing).
        if let Ok(reencoded) = frame.encode() {
            let redecoded = Frame::decode(&reencoded).expect("re-decode of our own encoding");
            assert_eq!(frame, redecoded);
        }
        // Feed it to the reassembler; this must never panic or grow unboundedly.
        let mut re = Reassembler::new();
        let _ = re.accept(frame);
    }
});
