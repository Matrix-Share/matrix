//! Forward-secret prekeys — rotating recipient keys that bound the exposure of a
//! seized long-term key (audit finding MED-1).
//!
//! The recipient's Ed25519 identity key is long-term (it *is* the address), but
//! its X25519 *encryption* key should not be: today a seized device decrypts all
//! past and future messages, because the one static key never rotates.
//!
//! A full Double Ratchet is the usual fix, but its security proof assumes timely,
//! roughly in-order delivery — assumptions that break under store-carry-forward's
//! long, one-way delays (Cohn-Gordon; Alwen). So we use the DTN-friendly
//! alternative: **rotating prekeys with a retention window**. The recipient
//! periodically mints a fresh prekey, publishes it **signed** by its identity
//! key, and keeps a small ring of recent prekey *secrets*. A sender seals to the
//! recipient's current prekey. The recipient retains each prekey secret only long
//! enough to cover the maximum in-flight message age (a bundle's TTL), then
//! deletes it — after which a compromise of the long-term identity key cannot
//! recover those older messages, because the prekey secret they were sealed to no
//! longer exists. Messages never arrive later than TTL, so retention >= TTL means
//! deliverability is unaffected.

use crate::crypto::SealedBox;
use crate::identity::{address_of, verify_sig, Identity};
use crate::{CoreError, Result};
use lifeline_proto::codec::to_cbor;
use lifeline_proto::{Address, Bytes, SignedPrekey};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use x25519_dalek::{PublicKey as XPublic, StaticSecret as XSecret};
use zeroize::Zeroize;

/// Domain separators.
const PREKEY_SIGN_DOMAIN: &[u8] = crate::domain::PREKEY_SIGN;
const PREKEY_INFO: &[u8] = crate::domain::PREKEY_SEAL;
const PREKEY_AD: &[u8] = crate::domain::PREKEY_AD;

fn prekey_signing_bytes(owner: &Address, epoch: u64, kex_pub: &Bytes) -> Vec<u8> {
    let mut m = Vec::with_capacity(PREKEY_SIGN_DOMAIN.len() + 32);
    m.extend_from_slice(PREKEY_SIGN_DOMAIN);
    m.extend_from_slice(&to_cbor(&(owner, epoch, kex_pub)).expect("cbor prekey"));
    m
}

/// Verify a prekey **self-containedly**: its embedded `sign_pub` binds to the
/// claimed `owner` address, and the signature is valid under it.
pub fn verify_prekey(pk: &SignedPrekey) -> Result<()> {
    if address_of(pk.sign_pub.as_slice())? != pk.owner {
        return Err(CoreError::Log("prekey owner key mismatch".into()));
    }
    verify_sig(
        pk.sign_pub.as_slice(),
        &prekey_signing_bytes(&pk.owner, pk.epoch, &pk.kex_pub),
        pk.sig.as_slice(),
    )
}

/// Seal `plaintext` to a verified recipient prekey (forward-secret path). Rejects
/// a prekey that isn't self-consistent, so a MITM can't substitute their own.
pub fn seal_to_prekey(prekey: &SignedPrekey, plaintext: &[u8]) -> Result<Vec<u8>> {
    verify_prekey(prekey)?;
    let pk = crate::crypto::x25519_pub_from_slice(prekey.kex_pub.as_slice())?;
    Ok(SealedBox::seal(&pk, PREKEY_AD, plaintext, PREKEY_INFO))
}

/// A serializable snapshot of a [`PrekeyRing`]'s retained secrets, for persisting
/// the ring **encrypted at rest** (via `core::vault`). Holds raw X25519 secrets,
/// so it zeroizes them on drop.
#[derive(Serialize, Deserialize, Default)]
pub struct PrekeyRingState {
    pub retain: u32,
    pub next_epoch: u64,
    /// `(epoch, 32-byte x25519 secret)` for each retained prekey.
    pub secrets: Vec<(u64, Bytes)>,
}

impl Drop for PrekeyRingState {
    fn drop(&mut self) {
        for (_, b) in self.secrets.iter_mut() {
            b.0.zeroize();
        }
    }
}

/// The recipient's ring of recent prekey secrets, newest last.
pub struct PrekeyRing {
    secrets: VecDeque<(u64, XSecret)>,
    /// How many recent prekeys to retain (>= the max in-flight message age / TTL,
    /// measured in rotations, so deliverability is unaffected).
    retain: usize,
    next_epoch: u64,
}

impl PrekeyRing {
    /// A ring retaining `retain` recent prekeys (minimum 1).
    pub fn new(retain: usize) -> Self {
        PrekeyRing {
            secrets: VecDeque::new(),
            retain: retain.max(1),
            next_epoch: 0,
        }
    }

    /// Mint a fresh prekey, dropping the oldest beyond the retention window (that
    /// deletion is what gives forward secrecy). Returns the new epoch.
    pub fn rotate(&mut self) -> u64 {
        let epoch = self.next_epoch;
        self.next_epoch += 1;
        let secret = XSecret::random_from_rng(rand::rngs::OsRng);
        self.secrets.push_back((epoch, secret));
        while self.secrets.len() > self.retain {
            self.secrets.pop_front();
        }
        epoch
    }

    /// The current (newest) prekey's public key, if any prekey exists.
    pub fn current_public(&self) -> Option<XPublic> {
        self.secrets.back().map(|(_, s)| XPublic::from(s))
    }

    /// Publish the current prekey as a signed, self-verifying record for
    /// distribution. Returns `None` if the ring is empty (rotate first).
    pub fn publish(&self, owner: &Identity) -> Option<SignedPrekey> {
        let (epoch, secret) = self.secrets.back()?;
        let kex_pub = Bytes::new(XPublic::from(secret).as_bytes().to_vec());
        let sig = owner.sign(&prekey_signing_bytes(owner.address(), *epoch, &kex_pub));
        Some(SignedPrekey {
            owner: owner.address().clone(),
            epoch: *epoch,
            kex_pub,
            sign_pub: Bytes::new(owner.verifying_key().as_bytes().to_vec()),
            sig,
        })
    }

    /// Try to open a prekey-sealed blob with each retained secret. Fails if no
    /// retained prekey matches — e.g. the message was sealed to a prekey already
    /// pruned for forward secrecy (which can only happen past its retention
    /// window, i.e. past the message TTL).
    pub fn open(&self, blob: &[u8]) -> Result<Vec<u8>> {
        for (_, secret) in self.secrets.iter().rev() {
            if let Ok(pt) = SealedBox::open(secret, PREKEY_AD, blob, PREKEY_INFO) {
                return Ok(pt);
            }
        }
        Err(CoreError::Decrypt)
    }

    /// Try to open a sealed [`Bundle`] a sender addressed to one of our recent
    /// prekeys, with each retained prekey secret (newest first). The bundle's
    /// `dst` is still our long-term address (the prekey only swaps the encryption
    /// key), so we pass that plus each candidate secret to the message opener.
    pub fn open_bundle(
        &self,
        recipient_addr: &lifeline_proto::Address,
        bundle: &lifeline_proto::Bundle,
    ) -> Option<crate::message::Opened> {
        for (_, secret) in self.secrets.iter().rev() {
            if let Ok(o) = crate::message::open_bundle_kex(recipient_addr, secret, bundle) {
                return Some(o);
            }
        }
        None
    }

    /// Snapshot the ring's secrets for **encrypted-at-rest persistence** so a
    /// node restart doesn't lose in-flight forward-secret messages (nor
    /// re-advertise a fresh key that makes sealed-in-flight messages unopenable).
    /// Only the currently *retained* secrets are exported — already-pruned
    /// prekeys stay gone, so forward secrecy is preserved across a restart too.
    pub fn export(&self) -> PrekeyRingState {
        PrekeyRingState {
            retain: self.retain as u32,
            next_epoch: self.next_epoch,
            secrets: self
                .secrets
                .iter()
                .map(|(e, s)| (*e, Bytes::new(s.to_bytes().to_vec())))
                .collect(),
        }
    }

    /// Restore a ring from a persisted snapshot (see [`PrekeyRing::export`]).
    pub fn import(state: &PrekeyRingState) -> Self {
        let secrets = state
            .secrets
            .iter()
            .filter_map(|(e, b)| {
                let arr: [u8; 32] = b.as_slice().try_into().ok()?;
                Some((*e, XSecret::from(arr)))
            })
            .collect();
        PrekeyRing {
            secrets,
            retain: (state.retain as usize).max(1),
            next_epoch: state.next_epoch,
        }
    }

    /// Number of retained prekeys (diagnostics/tests).
    pub fn len(&self) -> usize {
        self.secrets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_prekey_verifies_and_rejects_forgery() {
        let bob = Identity::generate(0);
        let eve = Identity::generate(0);
        let mut ring = PrekeyRing::new(3);
        ring.rotate();
        let pk = ring.publish(&bob).unwrap();

        assert!(verify_prekey(&pk).is_ok());
        // A prekey whose embedded sign_pub doesn't match its owner is rejected.
        let mut forged = pk.clone();
        forged.sign_pub = Bytes::new(eve.verifying_key().as_bytes().to_vec());
        assert!(verify_prekey(&forged).is_err());
        // Tampered key → signature no longer matches.
        let mut tampered = pk.clone();
        tampered.kex_pub = Bytes::new(vec![9; 32]);
        assert!(verify_prekey(&tampered).is_err());
    }

    #[test]
    fn rotation_gives_forward_secrecy() {
        let bob = Identity::generate(0);
        // Retain only the current prekey, so one rotation prunes the previous
        // secret — the sharpest demonstration of forward secrecy.
        let mut ring = PrekeyRing::new(1);
        ring.rotate();
        let pk1 = ring.publish(&bob).unwrap();

        // Alice seals a message to prekey 1.
        let ct1 = seal_to_prekey(&pk1, b"the drop is at noon").unwrap();
        assert_eq!(ring.open(&ct1).unwrap(), b"the drop is at noon");

        // Bob rotates; prekey 1's secret is deleted.
        ring.rotate();
        let pk2 = ring.publish(&bob).unwrap();
        assert_ne!(pk1.kex_pub, pk2.kex_pub);

        // The OLD ciphertext can no longer be opened — forward secrecy: a later
        // compromise of Bob's device (and identity key) can't recover it.
        assert!(
            ring.open(&ct1).is_err(),
            "a pruned prekey must make old ciphertext unrecoverable"
        );

        // A message to the current prekey still opens.
        let ct2 = seal_to_prekey(&pk2, b"still here").unwrap();
        assert_eq!(ring.open(&ct2).unwrap(), b"still here");
    }

    #[test]
    fn forward_secret_bundle_opens_via_ring_only() {
        use crate::message::{open_bundle, seal_bundle, SealOptions};
        use lifeline_proto::{Payload, PayloadKind};

        let alice = Identity::generate(0);
        let bob = Identity::generate(0);
        let mut ring = PrekeyRing::new(2);
        ring.rotate();
        let pk = ring.publish(&bob).unwrap();
        assert!(verify_prekey(&pk).is_ok());

        // Alice seals a real bundle to Bob's *prekey* (recipient record with the
        // prekey as its encryption key) — exactly what the engine does.
        let mut recip = bob.public();
        recip.kex_pub = pk.kex_pub.clone();
        let payload = Payload {
            kind: PayloadKind::Text,
            body: Some("meet at dawn".into()),
            coords: None,
            battery_pct: None,
            attach: None,
            group_id: None,
        };
        let bundle = seal_bundle(&alice, &recip, &payload, &SealOptions::normal(0)).unwrap();

        // Bob's long-term key cannot open it (it was sealed to the prekey)...
        assert!(open_bundle(&bob, &bundle).is_err());
        // ...but the prekey ring opens it and still authenticates Alice.
        let opened = ring
            .open_bundle(bob.address(), &bundle)
            .expect("ring opens FS bundle");
        assert_eq!(opened.payload.body.as_deref(), Some("meet at dawn"));
        assert_eq!(&opened.sender.id, alice.address());

        // Once rotations prune that prekey, the message is unrecoverable — forward
        // secrecy even against seizure of Bob's long-term identity key.
        ring.rotate();
        ring.rotate();
        assert!(
            ring.open_bundle(bob.address(), &bundle).is_none(),
            "a pruned prekey must make the sealed bundle unrecoverable"
        );
    }

    #[test]
    fn export_import_survives_restart_and_preserves_forward_secrecy() {
        use crate::message::{seal_bundle, SealOptions};
        use lifeline_proto::{Payload, PayloadKind};

        let alice = Identity::generate(0);
        let bob = Identity::generate(0);
        let mut ring = PrekeyRing::new(2);
        ring.rotate();
        let pk = ring.publish(&bob).unwrap();

        // A message is sealed to Bob's current prekey while Bob is "up".
        let mut recip = bob.public();
        recip.kex_pub = pk.kex_pub.clone();
        let payload = Payload {
            kind: PayloadKind::Text,
            body: Some("still reachable after restart".into()),
            coords: None,
            battery_pct: None,
            attach: None,
            group_id: None,
        };
        let bundle = seal_bundle(&alice, &recip, &payload, &SealOptions::normal(0)).unwrap();

        // Bob "restarts": persist the ring, drop it, restore from the snapshot.
        let restored = {
            let state = ring.export();
            drop(ring);
            PrekeyRing::import(&state)
        };
        // The in-flight message still opens after the restart.
        assert_eq!(
            restored
                .open_bundle(bob.address(), &bundle)
                .unwrap()
                .payload
                .body
                .as_deref(),
            Some("still reachable after restart")
        );

        // Forward secrecy still holds: rotate past the retained prekey and it's gone.
        let mut r = restored;
        r.rotate();
        r.rotate();
        assert!(
            r.open_bundle(bob.address(), &bundle).is_none(),
            "forward secrecy is preserved across a restart"
        );
    }

    #[test]
    fn retention_window_keeps_in_flight_messages_openable() {
        let bob = Identity::generate(0);
        // Retain 3 prekeys: messages sealed to any of the last 3 still open,
        // modelling store-carry-forward delays within the TTL window.
        let mut ring = PrekeyRing::new(3);
        ring.rotate();
        let pk_old = ring.publish(&bob).unwrap();
        let ct = seal_to_prekey(&pk_old, b"delayed mule msg").unwrap();

        ring.rotate();
        ring.rotate(); // pk_old is still within the retain=3 window
        assert_eq!(
            ring.open(&ct).unwrap(),
            b"delayed mule msg",
            "a message within the retention window must still deliver"
        );
        assert_eq!(ring.len(), 3, "ring never exceeds its retention window");
    }
}
