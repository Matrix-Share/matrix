//! Framing/parsing hardening (PRD NFR-1; the Bridgefy lesson — CT-RSA 2021 /
//! USENIX 2022). Every parser that touches untrusted bytes must **never panic**:
//! it returns `Err`, it does not crash the node. These tests fuzz each decoder
//! with random and mutated input. A panic here fails the build.

use lifeline_proto::codec::{b64url_decode, from_cbor, to_cbor};
use lifeline_proto::pow;
use lifeline_proto::{
    Address, Bundle, Bytes, DeliveryReceipt, GatewayAnnounce, IdentityPublic, LogEntry, Payload,
    Priority, WIRE_VERSION,
};

/// Tiny deterministic xorshift PRNG (no external deps; reproducible).
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
    fn bytes(&mut self, max_len: usize) -> Vec<u8> {
        let len = (self.next_u64() as usize) % (max_len + 1);
        (0..len).map(|_| (self.next_u64() & 0xff) as u8).collect()
    }
}

/// Try every decoder on `buf`. None may panic; results are ignored.
fn decode_all(buf: &[u8]) {
    let _ = from_cbor::<Bundle>(buf);
    let _ = from_cbor::<Payload>(buf);
    let _ = from_cbor::<IdentityPublic>(buf);
    let _ = from_cbor::<DeliveryReceipt>(buf);
    let _ = from_cbor::<GatewayAnnounce>(buf);
    let _ = from_cbor::<LogEntry>(buf);
    let _ = from_cbor::<Address>(buf);
    let _ = from_cbor::<Bytes>(buf);
    // Text parsers.
    if let Ok(s) = std::str::from_utf8(buf) {
        let _ = Address::from_text(s);
        let _ = b64url_decode(s);
    }
    // PoW verifier over arbitrary "postage".
    let _ = pow::verify_for_priority(buf, Some(&Bytes::new(buf.to_vec())), Priority::Bulk);
}

#[test]
fn decoders_never_panic_on_random_bytes() {
    let mut rng = Rng(0x1234_5678_9abc_def0);
    for _ in 0..50_000 {
        let buf = rng.bytes(64);
        decode_all(&buf);
    }
}

#[test]
fn decoders_never_panic_on_mutated_valid_bundle() {
    // Start from a real, valid bundle encoding and flip bytes.
    let valid = Bundle {
        v: WIRE_VERSION,
        bundle_id: Bytes::new(vec![1; 16]),
        dst: Address::from_hash_bytes([2; 16]),
        src_sealed: Bytes::new(vec![3; 48]),
        priority: Priority::Normal,
        created_at: 1_700_000_000,
        ttl_s: 604_800,
        hop_limit: 32,
        hops: 0,
        copies_left: 6,
        ciphertext: Bytes::new(vec![4; 128]),
        postage: Some(Bytes::new(vec![6; 8])),
        frag: None,
    };
    let base = to_cbor(&valid).unwrap();
    let mut rng = Rng(0xdead_beef_cafe_babe);

    for _ in 0..50_000 {
        let mut buf = base.clone();
        // Flip 1–4 random bytes.
        let flips = 1 + (rng.next_u64() as usize % 4);
        for _ in 0..flips {
            if buf.is_empty() {
                break;
            }
            let idx = rng.next_u64() as usize % buf.len();
            buf[idx] ^= (rng.next_u64() & 0xff) as u8;
        }
        // Occasionally truncate.
        if rng.next_u64() % 3 == 0 && !buf.is_empty() {
            let cut = rng.next_u64() as usize % buf.len();
            buf.truncate(cut);
        }
        decode_all(&buf);
    }
}

#[test]
fn truncated_prefixes_never_panic() {
    // Every prefix of a valid encoding must decode-or-error, never panic.
    let receipt = DeliveryReceipt {
        bundle_id: Bytes::new(vec![9; 16]),
        recipient: Address::from_hash_bytes([7; 16]),
        delivered_at: 42,
        sig: Bytes::new(vec![8; 64]),
    };
    let enc = to_cbor(&receipt).unwrap();
    for i in 0..=enc.len() {
        decode_all(&enc[..i]);
    }
}
