//! Hashcash-style proof-of-work "postage" (PRD FR-46, §12.5; Back 2002).
//!
//! Attaching a small proof-of-work to a message makes bulk spam expensive while
//! staying cheap for a real user sending one message. Difficulty is scaled
//! **inversely by priority**: emergency traffic is exempt, ordinary traffic
//! pays a little, bulk pays more. A relay verifies postage before admitting a
//! NORMAL/BULK bundle, throttling floods **without ever delaying SOS**.
//!
//! The proof binds to the bundle id (which is inside the signed header), so a
//! valid proof cannot be transplanted onto a different message.

use crate::{Bytes, Priority};

/// Domain-separation prefix so postage hashes can't collide with other uses.
const POW_TAG: &[u8] = b"lifeline/v1/postage";

/// Required leading-zero-bit difficulty for each priority class. Tuned to be
/// trivial for a single urgent/normal message but costly to sustain in bulk.
pub fn difficulty_for(priority: Priority) -> u32 {
    match priority {
        Priority::Sos => 0,     // exempt — never gated
        Priority::Alert => 6,   // ~64 hashes
        Priority::Normal => 12, // ~4k hashes
        Priority::Bulk => 16,   // ~65k hashes
    }
}

/// Count leading zero bits of a hash digest.
fn leading_zero_bits(digest: &[u8]) -> u32 {
    let mut bits = 0u32;
    for &b in digest {
        if b == 0 {
            bits += 8;
        } else {
            bits += b.leading_zeros();
            break;
        }
    }
    bits
}

/// The PoW digest for a `(bundle_id, nonce)` pair.
fn pow_hash(bundle_id: &[u8], nonce: u64) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(POW_TAG);
    hasher.update(bundle_id);
    hasher.update(&nonce.to_le_bytes());
    *hasher.finalize().as_bytes()
}

/// Mint a nonce whose digest has at least `difficulty` leading zero bits.
/// Returns the nonce; `difficulty == 0` returns 0 immediately.
pub fn mint(bundle_id: &[u8], difficulty: u32) -> u64 {
    if difficulty == 0 {
        return 0;
    }
    let mut nonce = 0u64;
    loop {
        if leading_zero_bits(&pow_hash(bundle_id, nonce)) >= difficulty {
            return nonce;
        }
        nonce = nonce.wrapping_add(1);
    }
}

/// Mint postage appropriate for a bundle's priority. Returns `None` for exempt
/// classes (no postage field needed).
pub fn mint_for_priority(bundle_id: &[u8], priority: Priority) -> Option<Bytes> {
    let difficulty = difficulty_for(priority);
    if difficulty == 0 {
        return None;
    }
    let nonce = mint(bundle_id, difficulty);
    Some(Bytes::new(nonce.to_le_bytes().to_vec()))
}

/// Verify a bundle's postage meets the difficulty required for its priority.
/// Exempt classes (difficulty 0) always pass, even with no postage attached.
pub fn verify_for_priority(bundle_id: &[u8], postage: Option<&Bytes>, priority: Priority) -> bool {
    let difficulty = difficulty_for(priority);
    if difficulty == 0 {
        return true;
    }
    let Some(p) = postage else {
        return false;
    };
    let Ok(arr) = <[u8; 8]>::try_from(p.as_slice()) else {
        return false;
    };
    let nonce = u64::from_le_bytes(arr);
    leading_zero_bits(&pow_hash(bundle_id, nonce)) >= difficulty
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sos_is_exempt() {
        assert_eq!(difficulty_for(Priority::Sos), 0);
        // No postage needed and verification passes.
        assert!(mint_for_priority(b"id", Priority::Sos).is_none());
        assert!(verify_for_priority(b"id", None, Priority::Sos));
    }

    #[test]
    fn mint_then_verify_normal() {
        let id = b"bundle-abc";
        let postage = mint_for_priority(id, Priority::Normal).unwrap();
        assert!(verify_for_priority(id, Some(&postage), Priority::Normal));
    }

    #[test]
    fn missing_postage_rejected_for_gated_class() {
        assert!(!verify_for_priority(b"id", None, Priority::Bulk));
    }

    #[test]
    fn postage_bound_to_bundle_id() {
        let postage = mint_for_priority(b"id-one", Priority::Normal).unwrap();
        // Same postage on a different bundle id must not verify.
        assert!(!verify_for_priority(
            b"id-two",
            Some(&postage),
            Priority::Normal
        ));
    }
}
