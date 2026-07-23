//! Signed delivery receipts + **offline** delivery verification (PRD L5,
//! FR-31..FR-34, §12.4).
//!
//! When a recipient decrypts a bundle it emits a [`DeliveryReceipt`] =
//! `Sign_R(bundle_id || delivered_at)`. The receipt diffuses back through the
//! mesh; the sender matches it to its log entry to reach the `delivered`
//! (verified) state. Verification is a pure function of the bundle, the
//! receipt, and the two public keys — it runs with **zero connectivity**, which
//! is the whole point of "verifiable delivery without a blockchain".

use crate::identity::{verify_sig, Identity};
use crate::message::header_signing_bytes_of;
use crate::{CoreError, Result};
use lifeline_proto::{Bundle, Bytes, DeliveryReceipt};

/// Bytes signed for a delivery receipt: `bundle_id || delivered_at(be64)`.
fn receipt_signing_bytes(bundle_id: &Bytes, delivered_at: u64) -> Vec<u8> {
    let mut m = Vec::with_capacity(bundle_id.len() + 8);
    m.extend_from_slice(bundle_id.as_slice());
    m.extend_from_slice(&delivered_at.to_be_bytes());
    m
}

/// Recipient emits a signed delivery receipt for a bundle it just decrypted
/// (FR-31).
pub fn make_delivery_receipt(
    recipient: &Identity,
    bundle_id: &Bytes,
    delivered_at: u64,
) -> DeliveryReceipt {
    let sig = recipient.sign(&receipt_signing_bytes(bundle_id, delivered_at));
    DeliveryReceipt {
        bundle_id: bundle_id.clone(),
        recipient: recipient.address().clone(),
        delivered_at,
        sig,
    }
}

/// Offline delivery verification (PRD §12.4).
///
/// Returns `Ok(())` iff:
/// 1. `receipt.bundle_id == bundle.bundle_id`, and
/// 2. `receipt.sig` verifies over `bundle_id || delivered_at` under
///    `recipient_sign_pub`, and
/// 3. `bundle.sig` verifies over the canonical header under `sender_sign_pub`,
///    and the receipt's `recipient` matches the bundle's `dst`.
///
/// Runs fully offline — no network, no clock dependence, no ledger.
pub fn verify_delivery(
    bundle: &Bundle,
    receipt: &DeliveryReceipt,
    sender_sign_pub: &[u8],
    recipient_sign_pub: &[u8],
) -> Result<()> {
    // 1. The receipt must be for this exact bundle.
    if receipt.bundle_id != bundle.bundle_id {
        return Err(CoreError::Log("receipt/bundle id mismatch".into()));
    }
    // The receipt's declared recipient must be the bundle's destination.
    if receipt.recipient != bundle.dst {
        return Err(CoreError::Log("receipt recipient != bundle dst".into()));
    }
    // Bind the recipient public key to the address it claims (self-sovereign id).
    let recipient_addr = derive_addr(recipient_sign_pub)?;
    if recipient_addr != bundle.dst {
        return Err(CoreError::Log(
            "recipient key does not match dst address".into(),
        ));
    }

    // 2. Recipient's signature over the receipt.
    verify_sig(
        recipient_sign_pub,
        &receipt_signing_bytes(&receipt.bundle_id, receipt.delivered_at),
        receipt.sig.as_slice(),
    )?;

    // 3. Sender's signature over the bundle header.
    let header = header_signing_bytes_of(bundle)?;
    verify_sig(sender_sign_pub, &header, bundle.sig.as_slice())?;

    Ok(())
}

fn derive_addr(sign_pub: &[u8]) -> Result<lifeline_proto::Address> {
    let arr: [u8; 32] = sign_pub
        .try_into()
        .map_err(|_| CoreError::BadKey("sign_pub len".into()))?;
    let vk = ed25519_dalek::VerifyingKey::from_bytes(&arr)
        .map_err(|e| CoreError::BadKey(e.to_string()))?;
    Ok(Identity::derive_address(&vk))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{seal_bundle, SealOptions};
    use lifeline_proto::{Payload, PayloadKind};

    fn text(body: &str) -> Payload {
        Payload {
            kind: PayloadKind::Text,
            body: Some(body.into()),
            coords: None,
            battery_pct: None,
            attach: None,
            group_id: None,
        }
    }

    #[test]
    fn full_delivery_proof_offline() {
        let alice = Identity::generate(1000);
        let bob = Identity::generate(1000);
        let bundle = seal_bundle(
            &alice,
            &bob.public(),
            &text("meet at the school"),
            &SealOptions::normal(2000),
        )
        .unwrap();

        // Bob receives, decrypts, emits a receipt.
        let receipt = make_delivery_receipt(&bob, &bundle.bundle_id, 2100);

        // Alice (or anyone with the two public keys) verifies delivery offline.
        assert!(verify_delivery(
            &bundle,
            &receipt,
            alice.verifying_key().as_bytes(),
            bob.verifying_key().as_bytes(),
        )
        .is_ok());
    }

    #[test]
    fn forged_receipt_rejected() {
        let alice = Identity::generate(1000);
        let bob = Identity::generate(1000);
        let eve = Identity::generate(1000);
        let bundle = seal_bundle(
            &alice,
            &bob.public(),
            &text("hi"),
            &SealOptions::normal(2000),
        )
        .unwrap();

        // Eve tries to forge a receipt claiming to be Bob.
        let mut forged = make_delivery_receipt(&eve, &bundle.bundle_id, 2100);
        forged.recipient = bob.address().clone();
        // Verifying with Bob's key must fail (Eve signed it).
        assert!(verify_delivery(
            &bundle,
            &forged,
            alice.verifying_key().as_bytes(),
            bob.verifying_key().as_bytes(),
        )
        .is_err());
    }

    #[test]
    fn receipt_for_other_bundle_rejected() {
        let alice = Identity::generate(1000);
        let bob = Identity::generate(1000);
        let b1 = seal_bundle(
            &alice,
            &bob.public(),
            &text("one"),
            &SealOptions::normal(2000),
        )
        .unwrap();
        let b2 = seal_bundle(
            &alice,
            &bob.public(),
            &text("two"),
            &SealOptions::normal(2000),
        )
        .unwrap();
        let receipt = make_delivery_receipt(&bob, &b2.bundle_id, 2100);
        assert!(verify_delivery(
            &b1,
            &receipt,
            alice.verifying_key().as_bytes(),
            bob.verifying_key().as_bytes(),
        )
        .is_err());
    }
}
