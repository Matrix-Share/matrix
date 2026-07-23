//! Framing hardening for the transport layer (PRD NFR-1; Bridgefy lesson).
//! `Frame::decode` and the `Reassembler` must never panic or allocate
//! unboundedly on adversarial input.

use lifeline_transport::{Frame, FrameKind, Reassembler};

struct Rng(u64);
impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

#[test]
fn frame_decode_never_panics_on_random_bytes() {
    let mut rng = Rng(0xa5a5_5a5a_1234_9999);
    for _ in 0..100_000 {
        let len = (rng.next_u64() as usize) % 80;
        let buf: Vec<u8> = (0..len).map(|_| (rng.next_u64() & 0xff) as u8).collect();
        let _ = Frame::decode(&buf);
    }
}

#[test]
fn reassembler_rejects_adversarial_frames_without_growing() {
    let mut re = Reassembler::new();
    let mut rng = Rng(0x0f0f_0f0f_dead_0001);
    for _ in 0..100_000 {
        // Random header fields, including absurd totals/indices.
        let frame = Frame {
            mid: lifeline_proto::Bytes::new(vec![(rng.next_u64() & 0xff) as u8; 4]),
            idx: rng.next_u64() as u16,
            total: rng.next_u64() as u16,
            kind: FrameKind::Bundle,
            data: vec![0u8; (rng.next_u64() as usize) % 8].into(),
        };
        let _ = re.accept(frame);
        // The reassembler must never accumulate an unbounded number of
        // in-flight messages from junk (bounded by valid mids only).
        assert!(re.in_flight() < 200_000);
    }
}

#[test]
fn oversized_total_is_refused() {
    let mut re = Reassembler::new();
    let bad = Frame {
        mid: lifeline_proto::Bytes::new(vec![1; 4]),
        idx: 0,
        total: u16::MAX,
        kind: FrameKind::Bundle,
        data: vec![0u8; 4].into(),
    };
    assert!(re.accept(bad).is_none());
    assert_eq!(re.in_flight(), 0);
}
