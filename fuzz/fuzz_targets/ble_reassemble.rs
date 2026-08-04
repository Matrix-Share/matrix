#![no_main]
//! Fuzz the BLE ATT-segment reassembler. A malicious peer controls the stream of
//! segments; reassembly must stay bounded (never exceed `MAX_REASSEMBLY` or OOM)
//! and never panic, regardless of segment sizes, ordering, or missing final bits.

use libfuzzer_sys::fuzz_target;
use lifeline_transport::ble::SegmentReassembler;

fuzz_target!(|data: &[u8]| {
    let mut re = SegmentReassembler::new();
    // Interpret the input as a sequence of length-prefixed segments, spread over
    // two peers, to exercise interleaved streams and the per-peer bound.
    let mut i = 0usize;
    while i < data.len() {
        let len = data[i] as usize; // 0..=255
        i += 1;
        let end = (i + len).min(data.len());
        let seg = &data[i..end];
        i = end;
        let peer = (seg.first().copied().unwrap_or(0) as u64) & 1;
        let _ = re.accept(peer, seg);
    }
    // The reassembler tracks at most one buffer per distinct peer.
    assert!(re.in_flight() <= 2);
});
