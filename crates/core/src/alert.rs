//! Authenticated authority alerts (PRD FR-42, §8.8; gateway doc Phase 3
//! "ingest of official alerts").
//!
//! Emergency authorities and NGOs need to broadcast warnings into the mesh, and
//! users need to know an alert is genuine — a spoofed "evacuate now" is
//! dangerous. An alert therefore carries an **authenticated broadcast identity**:
//! it is Ed25519-signed by the issuing authority, and a node accepts it only if
//! the signing key is in its trusted-authority store *and* that key matches the
//! claimed authority address.
//!
//! This is the trust half of FR-42 and is fully offline-verifiable. Pulling
//! alerts from an external source (e.g. India's Cell Broadcast) is a separate
//! gateway-side integration that feeds signed alerts into this type.

use crate::identity::{address_of, verify_sig, Identity};
use crate::{CoreError, Result};
use lifeline_proto::codec::to_cbor;
use lifeline_proto::{Address, Bytes};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// How urgent an alert is (drives UI prominence and routing priority).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    Info,
    Advisory,
    Warning,
    Emergency,
}

/// A signed broadcast alert from an authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedAlert {
    /// Claimed issuing authority (must match `sign_pub`).
    pub authority: Address,
    /// The authority's Ed25519 public key.
    pub sign_pub: Bytes,
    pub severity: AlertSeverity,
    /// Free-form affected area ("Chennai — North zone").
    pub area: String,
    pub body: String,
    pub issued_at: u64,
    /// After this time the alert is stale and must not be shown as current.
    pub expires_at: u64,
    pub sig: Bytes,
}

fn alert_signing_bytes(
    authority: &Address,
    severity: AlertSeverity,
    area: &str,
    body: &str,
    issued_at: u64,
    expires_at: u64,
) -> Result<Vec<u8>> {
    Ok(to_cbor(&(
        crate::domain::ALERT,
        authority,
        severity,
        area,
        body,
        issued_at,
        expires_at,
    ))?)
}

/// Issue a signed alert as `authority`.
pub fn make_alert(
    authority: &Identity,
    severity: AlertSeverity,
    area: &str,
    body: &str,
    issued_at: u64,
    expires_at: u64,
) -> Result<SignedAlert> {
    let signing = alert_signing_bytes(
        authority.address(),
        severity,
        area,
        body,
        issued_at,
        expires_at,
    )?;
    Ok(SignedAlert {
        authority: authority.address().clone(),
        sign_pub: Bytes::new(authority.verifying_key().as_bytes().to_vec()),
        severity,
        area: area.to_string(),
        body: body.to_string(),
        issued_at,
        expires_at,
        sig: authority.sign(&signing),
    })
}

/// The set of authority keys this node will accept alerts from — a small root
/// store, configured out-of-band (shipped with the app, or added by the user).
#[derive(Debug, Clone, Default)]
pub struct TrustedAuthorities {
    keys: BTreeSet<Vec<u8>>,
}

impl TrustedAuthorities {
    pub fn new() -> Self {
        Self::default()
    }
    /// Trust an authority's Ed25519 public key.
    pub fn trust(&mut self, sign_pub: &[u8]) {
        self.keys.insert(sign_pub.to_vec());
    }
    pub fn untrust(&mut self, sign_pub: &[u8]) {
        self.keys.remove(sign_pub);
    }
    pub fn is_trusted(&self, sign_pub: &[u8]) -> bool {
        self.keys.contains(sign_pub)
    }
    pub fn len(&self) -> usize {
        self.keys.len()
    }
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// Verify an alert offline: the signing key is trusted, it matches the claimed
/// authority address, the alert has not expired, and the signature is valid.
pub fn verify_alert(alert: &SignedAlert, trusted: &TrustedAuthorities, now: u64) -> Result<()> {
    if !trusted.is_trusted(alert.sign_pub.as_slice()) {
        return Err(CoreError::Log("alert from untrusted authority".into()));
    }
    if address_of(alert.sign_pub.as_slice())? != alert.authority {
        return Err(CoreError::Log(
            "authority key does not match address".into(),
        ));
    }
    if now >= alert.expires_at {
        return Err(CoreError::Log("alert expired".into()));
    }
    let signing = alert_signing_bytes(
        &alert.authority,
        alert.severity,
        &alert.area,
        &alert.body,
        alert.issued_at,
        alert.expires_at,
    )?;
    verify_sig(alert.sign_pub.as_slice(), &signing, alert.sig.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alert_from(id: &Identity) -> SignedAlert {
        make_alert(
            id,
            AlertSeverity::Emergency,
            "Chennai — North zone",
            "Move to higher ground immediately",
            1000,
            2000,
        )
        .unwrap()
    }

    #[test]
    fn trusted_authority_alert_verifies() {
        let ndma = Identity::generate(0);
        let mut trusted = TrustedAuthorities::new();
        trusted.trust(ndma.verifying_key().as_bytes());
        let alert = alert_from(&ndma);
        assert!(verify_alert(&alert, &trusted, 1500).is_ok());
    }

    #[test]
    fn untrusted_authority_rejected() {
        let impostor = Identity::generate(0);
        let trusted = TrustedAuthorities::new(); // empty root store
        let alert = alert_from(&impostor);
        assert!(
            verify_alert(&alert, &trusted, 1500).is_err(),
            "an unsigned-for authority must not be accepted"
        );
    }

    #[test]
    fn tampered_body_rejected() {
        let ndma = Identity::generate(0);
        let mut trusted = TrustedAuthorities::new();
        trusted.trust(ndma.verifying_key().as_bytes());
        let mut alert = alert_from(&ndma);
        alert.body = "Stay where you are".into(); // dangerous tampering
        assert!(verify_alert(&alert, &trusted, 1500).is_err());
    }

    #[test]
    fn expired_alert_rejected() {
        let ndma = Identity::generate(0);
        let mut trusted = TrustedAuthorities::new();
        trusted.trust(ndma.verifying_key().as_bytes());
        let alert = alert_from(&ndma);
        assert!(verify_alert(&alert, &trusted, 2001).is_err(), "expired");
    }

    #[test]
    fn spoofed_key_for_real_authority_address_rejected() {
        let ndma = Identity::generate(0);
        let eve = Identity::generate(0);
        let mut trusted = TrustedAuthorities::new();
        trusted.trust(eve.verifying_key().as_bytes()); // Eve's key is trusted…
        let mut alert = alert_from(&eve);
        alert.authority = ndma.address().clone(); // …but she claims NDMA's address
        assert!(verify_alert(&alert, &trusted, 1500).is_err());
    }
}
