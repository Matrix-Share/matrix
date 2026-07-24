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

use crate::crypto::{SealedBox, SecureChannel};
use crate::identity::{address_of, verify_sig, Identity};
use crate::{CoreError, Result};
use lifeline_proto::codec::to_cbor;
use lifeline_proto::{Address, Bytes};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use x25519_dalek::{PublicKey as XPublic, StaticSecret as XSecret};

/// Domain separators.
const PREKEY_SIGN_DOMAIN: &[u8] = b"lifeline/v1/prekey";
const PREKEY_INFO: &[u8] = b"lifeline/v1/prekey-seal";
const PREKEY_AD: &[u8] = b"lifeline/prekey";

/// A published, signed prekey. Senders verify it against the owner's identity key
/// (known via TOFU/QR), then seal messages to `kex_pub` for forward secrecy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedPrekey {
    pub owner: Address,
    /// Rotation counter — higher is newer.
    pub epoch: u64,
    /// The prekey's X25519 public key.
    pub kex_pub: Bytes,
    /// Owner's Ed25519 signature over `(owner, epoch, kex_pub)`.
    pub sig: Bytes,
}

fn prekey_signing_bytes(owner: &Address, epoch: u64, kex_pub: &Bytes) -> Vec<u8> {
    let mut m = Vec::with_capacity(PREKEY_SIGN_DOMAIN.len() + 32);
    m.extend_from_slice(PREKEY_SIGN_DOMAIN);
    m.extend_from_slice(&to_cbor(&(owner, epoch, kex_pub)).expect("cbor prekey"));
    m
}

impl SignedPrekey {
    /// Verify this prekey is authentic: the signature is valid under
    /// `owner_sign_pub` and that key binds to the claimed `owner` address.
    pub fn verify(&self, owner_sign_pub: &[u8]) -> Result<()> {
        if address_of(owner_sign_pub)? != self.owner {
            return Err(CoreError::Log("prekey owner key mismatch".into()));
        }
        verify_sig(
            owner_sign_pub,
            &prekey_signing_bytes(&self.owner, self.epoch, &self.kex_pub),
            self.sig.as_slice(),
        )
    }

    fn public(&self) -> Result<XPublic> {
        crate::crypto::x25519_pub_from_slice(self.kex_pub.as_slice())
    }
}

/// Seal `plaintext` to a verified recipient prekey (forward-secret path). The
/// caller supplies the recipient's identity signing key to authenticate the
/// prekey first, so a MITM cannot substitute their own.
pub fn seal_to_prekey(
    prekey: &SignedPrekey,
    owner_sign_pub: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    prekey.verify(owner_sign_pub)?;
    let pk = prekey.public()?;
    Ok(SealedBox::seal(&pk, PREKEY_AD, plaintext, PREKEY_INFO))
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

    /// Publish the current prekey as a signed record for distribution. Returns
    /// `None` if the ring is empty (call [`PrekeyRing::rotate`] first).
    pub fn publish(&self, owner: &Identity) -> Option<SignedPrekey> {
        let (epoch, secret) = self.secrets.back()?;
        let kex_pub = Bytes::new(XPublic::from(secret).as_bytes().to_vec());
        let sig = owner.sign(&prekey_signing_bytes(owner.address(), *epoch, &kex_pub));
        Some(SignedPrekey {
            owner: owner.address().clone(),
            epoch: *epoch,
            kex_pub,
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

        assert!(pk.verify(bob.verifying_key().as_bytes()).is_ok());
        // Wrong signer.
        assert!(pk.verify(eve.verifying_key().as_bytes()).is_err());
        // Tampered key.
        let mut tampered = pk.clone();
        tampered.kex_pub = Bytes::new(vec![9; 32]);
        assert!(tampered.verify(bob.verifying_key().as_bytes()).is_err());
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
        let ct1 =
            seal_to_prekey(&pk1, bob.verifying_key().as_bytes(), b"the drop is at noon").unwrap();
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
        let ct2 = seal_to_prekey(&pk2, bob.verifying_key().as_bytes(), b"still here").unwrap();
        assert_eq!(ring.open(&ct2).unwrap(), b"still here");
    }

    #[test]
    fn retention_window_keeps_in_flight_messages_openable() {
        let bob = Identity::generate(0);
        // Retain 3 prekeys: messages sealed to any of the last 3 still open,
        // modelling store-carry-forward delays within the TTL window.
        let mut ring = PrekeyRing::new(3);
        ring.rotate();
        let pk_old = ring.publish(&bob).unwrap();
        let ct =
            seal_to_prekey(&pk_old, bob.verifying_key().as_bytes(), b"delayed mule msg").unwrap();

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
