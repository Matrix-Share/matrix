//! Identity key rotation & revocation (G4).
//!
//! A Lifeline identity's network address is `blake3(sign_pub)[..16]` — the
//! signing key *is* the identity. That is what makes identity self-sovereign and
//! registrar-free, but it also means the base protocol has **no way to retire a
//! key**: if your key is compromised or you simply want to roll it, every contact
//! still trusts the old one forever. This is the same gap Nostr has (your `npub`
//! is your account, with no native rotation/revocation), and the current
//! best-practice answer there is social: sign a note from the old key that points
//! at the new one. This module makes that answer **cryptographic and
//! machine-checkable**.
//!
//! ## Certificates
//! - [`RotationCert`] — the retiring key attests a *successor* identity. A
//!   contact that already trusts the old key verifies the cert and migrates its
//!   directory entry old → new.
//! - [`RevocationCert`] — the retiring key attests that it is being retired with
//!   **no** successor (a compromised device you can still sign from once, a
//!   decommission). A contact drops/flags the identity.
//!
//! Both are **self-verifying**: they carry the retiring key's `sign_pub`, bound
//! to its address, so any node can check one offline without prior knowledge of
//! the signer — exactly like a gateway announce.
//!
//! ## What this does and does not solve (honest boundaries)
//! - **Voluntary rotation / decommission:** solved. You still hold the old key
//!   when you rotate, so you can sign the cert.
//! - **Compromise, if you rotate first:** helped. Contacts take the
//!   monotonically-newest cert (by `issued_at`), so racing the attacker to
//!   publish a rotation wins; an attacker who *also* holds the key can sign a
//!   competing cert, so this is damage-limitation, not a guarantee.
//! - **Lost key:** **not** solved here. If the key is gone you cannot sign
//!   anything — that needs a pre-registered cold master/recovery key, which is a
//!   separate, larger change (a rotation cert signed by a master key rather than
//!   the operational key). Tracked as future work; this module is the
//!   operational-key half.

use crate::identity::{address_of, verify_sig, Identity};
use crate::{CoreError, Result};
use lifeline_proto::codec::to_cbor;
use lifeline_proto::{Address, Bytes, IdentityPublic};
use serde::{Deserialize, Serialize};

/// Why a key is being retired (advisory; affects how a client presents it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetireReason {
    /// Routine roll to a fresh key; the old key was not known to be compromised.
    Rotated,
    /// The key is believed compromised — contacts should distrust past traffic
    /// signed by it after `issued_at` and stop using it immediately.
    Compromised,
    /// The identity is being decommissioned with no successor.
    Retired,
}

/// A signed attestation that identity `old_addr` is retiring in favour of the
/// `new` identity. Signed by the **old** key, so only its holder can authorize
/// the migration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RotationCert {
    pub old_addr: Address,
    /// The retiring key, carried so the cert is self-verifying (bound to `old_addr`).
    pub old_sign_pub: Bytes,
    /// The successor identity (its own `id` must equal `blake3(new.sign_pub)[..16]`).
    pub new: IdentityPublic,
    pub reason: RetireReason,
    pub issued_at: u64,
    pub sig: Bytes,
}

/// A signed attestation that identity `addr` is retired with **no** successor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RevocationCert {
    pub addr: Address,
    pub sign_pub: Bytes,
    pub reason: RetireReason,
    pub issued_at: u64,
    pub sig: Bytes,
}

/// The body carried in a [`PayloadKind::KeyRotation`](lifeline_proto::PayloadKind)
/// payload — either a rotation to a successor or a revocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IdentityUpdate {
    Rotate(RotationCert),
    Revoke(RevocationCert),
}

fn rotation_signing_bytes(
    old_addr: &Address,
    new: &IdentityPublic,
    reason: RetireReason,
    issued_at: u64,
) -> Vec<u8> {
    let mut v = crate::domain::KEY_ROTATION.to_vec();
    v.extend_from_slice(
        &to_cbor(&(
            old_addr,
            &new.id,
            new.sign_pub.as_slice(),
            new.kex_pub.as_slice(),
            reason,
            issued_at,
        ))
        .expect("cbor rotation"),
    );
    v
}

fn revocation_signing_bytes(
    addr: &Address,
    sign_pub: &[u8],
    reason: RetireReason,
    issued_at: u64,
) -> Vec<u8> {
    let mut v = crate::domain::KEY_REVOCATION.to_vec();
    v.extend_from_slice(&to_cbor(&(addr, sign_pub, reason, issued_at)).expect("cbor revocation"));
    v
}

/// Build a rotation cert: `old` (the current identity) attests `new` as its
/// successor. Signed by `old`.
pub fn make_rotation_cert(
    old: &Identity,
    new: &IdentityPublic,
    reason: RetireReason,
    now: u64,
) -> RotationCert {
    let old_addr = old.address().clone();
    let sig = old.sign(&rotation_signing_bytes(&old_addr, new, reason, now));
    RotationCert {
        old_addr,
        old_sign_pub: Bytes::new(old.verifying_key().as_bytes().to_vec()),
        new: new.clone(),
        reason,
        issued_at: now,
        sig,
    }
}

/// Build a revocation cert: `id` retires itself with no successor. Signed by `id`.
pub fn make_revocation_cert(id: &Identity, reason: RetireReason, now: u64) -> RevocationCert {
    let addr = id.address().clone();
    let sign_pub = id.verifying_key().as_bytes().to_vec();
    let sig = id.sign(&revocation_signing_bytes(&addr, &sign_pub, reason, now));
    RevocationCert {
        addr,
        sign_pub: Bytes::new(sign_pub),
        reason,
        issued_at: now,
        sig,
    }
}

/// Verify a rotation cert **self-containedly** (offline, no prior knowledge of
/// the signer): the carried key binds to `old_addr`, the successor identity is
/// internally consistent, the successor is actually a *different* identity, and
/// the signature is valid under the old key.
pub fn verify_rotation_cert(cert: &RotationCert) -> Result<()> {
    // 1. The retiring key must be the one that owns `old_addr`.
    if address_of(cert.old_sign_pub.as_slice())? != cert.old_addr {
        return Err(CoreError::BadKey("rotation: old key ≠ old address".into()));
    }
    // 2. The successor identity must be internally consistent (its address is the
    //    hash of its own signing key), so a cert can't point at a spoofed address.
    if address_of(cert.new.sign_pub.as_slice())? != cert.new.id {
        return Err(CoreError::BadKey("rotation: new key ≠ new address".into()));
    }
    // 3. A rotation must actually change identity.
    if cert.new.id == cert.old_addr {
        return Err(CoreError::BadKey(
            "rotation: successor == predecessor".into(),
        ));
    }
    // 4. The old key must have signed it.
    verify_sig(
        cert.old_sign_pub.as_slice(),
        &rotation_signing_bytes(&cert.old_addr, &cert.new, cert.reason, cert.issued_at),
        cert.sig.as_slice(),
    )
}

/// Verify a revocation cert self-containedly.
pub fn verify_revocation_cert(cert: &RevocationCert) -> Result<()> {
    if address_of(cert.sign_pub.as_slice())? != cert.addr {
        return Err(CoreError::BadKey("revocation: key ≠ address".into()));
    }
    verify_sig(
        cert.sign_pub.as_slice(),
        &revocation_signing_bytes(
            &cert.addr,
            cert.sign_pub.as_slice(),
            cert.reason,
            cert.issued_at,
        ),
        cert.sig.as_slice(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_roundtrips_and_verifies() {
        let old = Identity::generate(0);
        let new = Identity::generate(1);
        let cert = make_rotation_cert(&old, &new.public(), RetireReason::Rotated, 100);
        assert!(verify_rotation_cert(&cert).is_ok());
        assert_eq!(cert.old_addr, *old.address());
        assert_eq!(cert.new.id, *new.address());
    }

    #[test]
    fn a_forged_signature_is_rejected() {
        let old = Identity::generate(0);
        let new = Identity::generate(1);
        let eve = Identity::generate(2);
        let mut cert = make_rotation_cert(&old, &new.public(), RetireReason::Rotated, 100);
        // Eve tries to pass her signature off as the old key's.
        cert.sig = eve.sign(&rotation_signing_bytes(
            &cert.old_addr,
            &cert.new,
            cert.reason,
            cert.issued_at,
        ));
        assert!(verify_rotation_cert(&cert).is_err());
    }

    #[test]
    fn a_mismatched_old_key_is_rejected() {
        let old = Identity::generate(0);
        let new = Identity::generate(1);
        let eve = Identity::generate(2);
        let mut cert = make_rotation_cert(&old, &new.public(), RetireReason::Rotated, 100);
        // Swap in Eve's key as the "old" key — no longer binds to old_addr.
        cert.old_sign_pub = Bytes::new(eve.verifying_key().as_bytes().to_vec());
        assert!(verify_rotation_cert(&cert).is_err());
    }

    #[test]
    fn a_successor_with_a_spoofed_address_is_rejected() {
        let old = Identity::generate(0);
        let new = Identity::generate(1);
        let mut cert = make_rotation_cert(&old, &new.public(), RetireReason::Rotated, 100);
        // Tamper the successor's advertised address so it no longer matches its key.
        cert.new.id = old.address().clone();
        assert!(verify_rotation_cert(&cert).is_err());
    }

    #[test]
    fn tampering_the_successor_key_breaks_the_signature() {
        let old = Identity::generate(0);
        let new = Identity::generate(1);
        let attacker = Identity::generate(2);
        let mut cert = make_rotation_cert(&old, &new.public(), RetireReason::Rotated, 100);
        // Redirect the rotation at the attacker's identity; sig no longer matches.
        cert.new = attacker.public();
        assert!(verify_rotation_cert(&cert).is_err());
    }

    #[test]
    fn revocation_roundtrips_and_rejects_forgery() {
        let id = Identity::generate(0);
        let eve = Identity::generate(1);
        let cert = make_revocation_cert(&id, RetireReason::Compromised, 42);
        assert!(verify_revocation_cert(&cert).is_ok());

        let mut forged = cert.clone();
        forged.sign_pub = Bytes::new(eve.verifying_key().as_bytes().to_vec());
        assert!(verify_revocation_cert(&forged).is_err());
    }
}
