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
use crate::{CoreError, Result};
use lifeline_proto::{Bundle, Bytes, CustodyReceipt, DeliveryReceipt};

/// Domain separator so a delivery-receipt signature can never be confused with a
/// signature over some other `bundle_id || u64` context (defense-in-depth,
/// matching the tagging used by custody/announce/alert/relay-credit).
const DELIVERY_DOMAIN: &[u8] = b"lifeline/v1/delivery-receipt";

/// Bytes signed for a delivery receipt: `DOMAIN || bundle_id || delivered_at(be64)`.
fn receipt_signing_bytes(bundle_id: &Bytes, delivered_at: u64) -> Vec<u8> {
    let mut m = Vec::with_capacity(DELIVERY_DOMAIN.len() + bundle_id.len() + 8);
    m.extend_from_slice(DELIVERY_DOMAIN);
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
/// 1. `receipt.bundle_id == bundle.bundle_id` and `receipt.recipient == bundle.dst`, and
/// 2. `receipt.sig` verifies over the canonical receipt bytes under
///    `recipient_sign_pub`, whose key binds to the `dst` address.
///
/// The **sender** holds their own authored `bundle`, so this gives them complete
/// proof that the recipient acknowledged that exact message. Note it does *not*
/// re-verify the sender's signature: since HIGH-1 that signature lives inside the
/// recipient-sealed envelope (true sealed sender), so it is deliberately *not*
/// third-party-verifiable — you cannot both hide the sender from observers and
/// let arbitrary third parties confirm the sender.
///
/// Runs fully offline — no network, no clock dependence, no ledger.
pub fn verify_delivery(
    bundle: &Bundle,
    receipt: &DeliveryReceipt,
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

// --- Custody receipts (PRD FR-25 / §11.5) ---

/// Bytes signed for a custody receipt: `bundle_id || "custody" || at(be64)`.
fn custody_signing_bytes(bundle_id: &Bytes, at: u64) -> Vec<u8> {
    let mut m = Vec::with_capacity(bundle_id.len() + 15);
    m.extend_from_slice(bundle_id.as_slice());
    m.extend_from_slice(b"custody");
    m.extend_from_slice(&at.to_be_bytes());
    m
}

/// A relay signs this when it **accepts responsibility** for a bundle (FR-25),
/// so the previous holder can safely drop its copy — hand-offs are acknowledged,
/// never silent.
pub fn make_custody_receipt(custodian: &Identity, bundle_id: &Bytes, at: u64) -> CustodyReceipt {
    let sig = custodian.sign(&custody_signing_bytes(bundle_id, at));
    CustodyReceipt {
        bundle_id: bundle_id.clone(),
        custodian: custodian.address().clone(),
        at,
        sig,
    }
}

/// Verify a custody receipt offline: the signature is valid under
/// `custodian_sign_pub` and that key matches the receipt's custodian address.
pub fn verify_custody_receipt(receipt: &CustodyReceipt, custodian_sign_pub: &[u8]) -> Result<()> {
    if derive_addr(custodian_sign_pub)? != receipt.custodian {
        return Err(CoreError::Log(
            "custodian key does not match address".into(),
        ));
    }
    verify_sig(
        custodian_sign_pub,
        &custody_signing_bytes(&receipt.bundle_id, receipt.at),
        receipt.sig.as_slice(),
    )
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
    fn custody_receipt_verifies_and_rejects_forgery() {
        let relay = Identity::generate(0);
        let eve = Identity::generate(0);
        let bundle_id = Bytes::new(vec![3; 16]);
        let cr = make_custody_receipt(&relay, &bundle_id, 1234);

        // Valid under the real custodian's key.
        assert!(verify_custody_receipt(&cr, relay.verifying_key().as_bytes()).is_ok());
        // A different key (wrong custodian address) fails.
        assert!(verify_custody_receipt(&cr, eve.verifying_key().as_bytes()).is_err());
        // Tampered timestamp fails.
        let mut tampered = cr.clone();
        tampered.at = 9999;
        assert!(verify_custody_receipt(&tampered, relay.verifying_key().as_bytes()).is_err());
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
        assert!(verify_delivery(&bundle, &receipt, bob.verifying_key().as_bytes(),).is_ok());
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
        assert!(verify_delivery(&bundle, &forged, bob.verifying_key().as_bytes(),).is_err());
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
        assert!(verify_delivery(&b1, &receipt, bob.verifying_key().as_bytes(),).is_err());
    }
}
