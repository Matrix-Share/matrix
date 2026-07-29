//! Rotating rendezvous addresses — recipient-unlinkable DTN addressing (G2).
//!
//! A Lifeline bundle already hides its *sender* from carriers (sealed sender,
//! `src_sealed`), but the [`Bundle::dst`](lifeline_proto::Bundle) — the
//! recipient's real network address — travels in the clear on every stored-and-
//! forwarded copy. Any carrier can therefore log who is receiving mail and track
//! a recipient across the mesh over time (the same metadata leak Nostr has with
//! the recipient `p`-tag, and the "stable device id" that bitchat's own authors
//! call their design's weakest part).
//!
//! ## The fix: address to a rotating pseudonym, not the real address
//! For a **private** send we set `dst` to a *rendezvous address*
//!
//! ```text
//! rv(recipient, epoch) = HKDF(ikm = recipient_sign_pub, salt = epoch, info = "…/rendezvous")[..16]
//! ```
//!
//! that changes every [`EPOCH_SECS`]. The recipient recognizes its own current
//! address by recomputing `rv(self, epoch)`; a carrier sees only an opaque,
//! rotating 16-byte tag.
//!
//! **O(1) matching, despite rotation.** The recipient does not scan a window of
//! epochs: the bundle already carries `created_at`, so the epoch the sender used
//! is `epoch_of(created_at)`. The recipient checks that epoch plus its immediate
//! neighbours ([`SKEW_EPOCHS`]) to tolerate clock skew across the boundary — a
//! small constant number of HKDF calls per received bundle.
//!
//! ## Threat model — what this does and does not buy (stated honestly)
//! The HKDF is keyed on the recipient's **public** signing key, which is not a
//! secret among the recipient's contacts. So this is **pseudonymity against
//! carriers who do not already know the recipient's key**, not confidentiality
//! against a global adversary who does. That is the right and useful boundary:
//! the overwhelming majority of nodes carrying a bundle are strangers to its
//! recipient, and against them a rendezvous address is unlinkable both to an
//! identity *and* across epochs. An adversary who already holds the recipient's
//! public key could compute the tags — but such an adversary could also just read
//! the real `dst` we replaced, so nothing is lost relative to today. The payload
//! itself stays sealed to the recipient's real key regardless, so `dst` never
//! gates confidentiality — only routing/recognition.

use crate::crypto;
use crate::domain;
use lifeline_proto::address::{Address, ADDRESS_LEN};

/// How often a recipient's rendezvous address rotates. One hour balances
/// unlinkability (finer = fewer bundles share a tag) against the clock-skew
/// window we must tolerate at epoch boundaries.
pub const EPOCH_SECS: u64 = 3600;

/// How many neighbouring epochs on each side of the bundle's stamped epoch a
/// recipient also checks, to tolerate sender/recipient clock skew across an
/// epoch boundary. 1 → the previous and next hour are still recognized.
pub const SKEW_EPOCHS: u64 = 1;

/// The epoch index a timestamp falls in.
pub fn epoch_of(unix_secs: u64) -> u64 {
    unix_secs / EPOCH_SECS
}

/// The rendezvous address for `recipient_sign_pub` in `epoch`. Deterministic:
/// the sender (who knows the recipient's signing key) and the recipient compute
/// the same value; a third party without the key cannot.
pub fn rendezvous_addr(recipient_sign_pub: &[u8], epoch: u64) -> Address {
    let tag = crypto::hkdf_sha256(
        recipient_sign_pub,
        &epoch.to_be_bytes(),
        domain::RENDEZVOUS,
        ADDRESS_LEN,
    );
    let mut bytes = [0u8; ADDRESS_LEN];
    bytes.copy_from_slice(&tag[..ADDRESS_LEN]);
    Address::from_hash_bytes(bytes)
}

/// Does `dst` name *me* as a rendezvous recipient, given the bundle's
/// `created_at`? Checks the stamped epoch and [`SKEW_EPOCHS`] neighbours on each
/// side. Constant work per call.
pub fn is_rendezvous_for(dst: &Address, my_sign_pub: &[u8], created_at: u64) -> bool {
    let center = epoch_of(created_at);
    // `center` can be small; guard the subtraction so we never underflow.
    let lo = center.saturating_sub(SKEW_EPOCHS);
    let hi = center.saturating_add(SKEW_EPOCHS);
    (lo..=hi).any(|e| &rendezvous_addr(my_sign_pub, e) == dst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Identity;

    fn pubkey(id: &Identity) -> Vec<u8> {
        id.verifying_key().as_bytes().to_vec()
    }

    #[test]
    fn sender_and_recipient_agree_on_the_address() {
        let bob = Identity::generate(0);
        let bob_pub = pubkey(&bob);
        let t = 5 * EPOCH_SECS + 123; // somewhere inside epoch 5
                                      // Sender computes it from Bob's public key; recipient recomputes it.
        let addressed = rendezvous_addr(&bob_pub, epoch_of(t));
        assert!(is_rendezvous_for(&addressed, &bob_pub, t));
    }

    #[test]
    fn a_different_recipient_does_not_match() {
        let bob = Identity::generate(0);
        let carol = Identity::generate(0);
        let t = 9 * EPOCH_SECS;
        let to_bob = rendezvous_addr(&pubkey(&bob), epoch_of(t));
        assert!(is_rendezvous_for(&to_bob, &pubkey(&bob), t));
        assert!(
            !is_rendezvous_for(&to_bob, &pubkey(&carol), t),
            "Carol must not recognize a bundle addressed to Bob"
        );
    }

    #[test]
    fn the_address_rotates_across_epochs() {
        let bob_pub = pubkey(&Identity::generate(0));
        let e = 100;
        let a = rendezvous_addr(&bob_pub, e);
        let b = rendezvous_addr(&bob_pub, e + 1);
        assert_ne!(a, b, "a rendezvous address must change every epoch");
        // And an unlinkability property: knowing one epoch's tag tells you
        // nothing you can trivially turn into the next (they just differ).
        let c = rendezvous_addr(&bob_pub, e + 5);
        assert_ne!(a, c);
        assert_ne!(b, c);
    }

    #[test]
    fn skew_window_tolerates_a_boundary_but_not_a_distant_epoch() {
        let bob_pub = pubkey(&Identity::generate(0));
        // Sender addressed at epoch E; bundle's created_at also in epoch E.
        let e = 50u64;
        let created = e * EPOCH_SECS + 10;
        let sent_prev = rendezvous_addr(&bob_pub, e - 1);
        let sent_next = rendezvous_addr(&bob_pub, e + 1);
        // ±1 epoch is still recognized (clock skew across the boundary)...
        assert!(is_rendezvous_for(&sent_prev, &bob_pub, created));
        assert!(is_rendezvous_for(&sent_next, &bob_pub, created));
        // ...but an epoch well outside the skew window is not.
        let sent_far = rendezvous_addr(&bob_pub, e + 5);
        assert!(!is_rendezvous_for(&sent_far, &bob_pub, created));
    }

    #[test]
    fn epoch_zero_does_not_underflow() {
        // created_at inside epoch 0 must be safe (saturating_sub guard).
        let bob_pub = pubkey(&Identity::generate(0));
        let a = rendezvous_addr(&bob_pub, 0);
        assert!(is_rendezvous_for(&a, &bob_pub, 3)); // t=3s → epoch 0
    }
}
