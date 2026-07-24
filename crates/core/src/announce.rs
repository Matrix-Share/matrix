//! Signed gateway announces (PRD FR-36, §12.2).
//!
//! A gateway periodically emits a signed [`GatewayAnnounce`] carrying its
//! capabilities, a coarse load, and a freshness `seq`. The announce propagates a
//! few hops (see `router::gateway`), letting every node compute a **gradient**
//! toward the nearest healthy gateway. Signing binds the announce to the
//! gateway's key so a node that knows the gateway can reject forgeries — the
//! anti-abuse defence against a black-hole node forging attractive announces.

use crate::identity::{address_of, verify_sig, Identity};
use crate::{CoreError, Result};
use lifeline_proto::codec::to_cbor;
use lifeline_proto::{Bytes, GatewayAnnounce};

/// Canonical bytes signed for an announce: the CBOR of its immutable fields
/// (everything except the signature itself and the mutable hop distance).
fn announce_signing_bytes(a: &GatewayAnnounce) -> Vec<u8> {
    to_cbor(&(&a.gateway, &a.caps, a.load_q, a.seq, a.expires_at)).expect("cbor announce")
}

/// Build a signed gateway announce for `caps`, fresh until `expires_at`.
pub fn make_gateway_announce(
    gateway: &Identity,
    caps: &[String],
    load: f32,
    seq: u64,
    expires_at: u64,
) -> GatewayAnnounce {
    let mut a = GatewayAnnounce {
        gateway: gateway.address().clone(),
        caps: caps.to_vec(),
        load_q: GatewayAnnounce::from_load(load),
        seq,
        expires_at,
        sig: Bytes::default(),
    };
    a.sig = gateway.sign(&announce_signing_bytes(&a));
    a
}

/// Verify an announce's signature against the gateway's signing key, and that
/// the key matches the announced gateway address. Offline, pure.
pub fn verify_gateway_announce(a: &GatewayAnnounce, gateway_sign_pub: &[u8]) -> Result<()> {
    if address_of(gateway_sign_pub)? != a.gateway {
        return Err(CoreError::Log("gateway key does not match address".into()));
    }
    verify_sig(
        gateway_sign_pub,
        &announce_signing_bytes(a),
        a.sig.as_slice(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announce_verifies_and_rejects_forgery() {
        let gw = Identity::generate(0);
        let eve = Identity::generate(0);
        let a = make_gateway_announce(&gw, &["internet".into()], 0.3, 1, 10_000);
        assert!(verify_gateway_announce(&a, gw.verifying_key().as_bytes()).is_ok());
        // Wrong key → rejected.
        assert!(verify_gateway_announce(&a, eve.verifying_key().as_bytes()).is_err());
        // Tampered caps → signature no longer matches.
        let mut tampered = a.clone();
        tampered.caps.push("lora".into());
        assert!(verify_gateway_announce(&tampered, gw.verifying_key().as_bytes()).is_err());
    }
}
