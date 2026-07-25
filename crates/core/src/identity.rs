//! Self-sovereign identity (PRD L2, FR-1..FR-5).
//!
//! On first launch a node generates an Ed25519 signing keypair and an X25519
//! key-exchange keypair. Its **network address is derived from the signing
//! public key** (`blake3(sign_pub)[..16]`), so there is no registrar, phone
//! number, or email (FR-2, G3). The private half never leaves the device
//! unencrypted (NFR-1); [`KeyBackup`] provides an encrypted, passphrase-wrapped
//! export/restore (FR-5).

use crate::crypto::{self, KEY_LEN, NONCE_LEN};
use crate::{CoreError, Result};
use argon2::Argon2;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use lifeline_proto::{Address, Bytes, IdentityPublic};
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey as XPublic, StaticSecret as XSecret};
use zeroize::Zeroize;

// Message-sealing domain separators live in `crate::domain` (they are a
// messaging concern, not an identity one) — see `domain::MESSAGE` /
// `domain::SEALED_SENDER`.

/// A full identity, including secret keys. Secrets are zeroized on drop.
pub struct Identity {
    signing: SigningKey,
    kex_secret: XSecret,
    address: Address,
    created_at: u64,
    display_name: Option<String>,
}

impl Identity {
    /// Generate a brand-new identity (FR-1) using the OS CSPRNG.
    pub fn generate(now: u64) -> Self {
        Self::generate_from_rng(&mut rand::rngs::OsRng, now)
    }

    /// Generate an identity from a caller-supplied CSPRNG.
    ///
    /// ⚠️ **Testing / simulation only.** This mints a node's *long-term* signing
    /// and key-exchange secrets from `rng`. Calling it with a seeded/deterministic
    /// RNG in production would produce fully predictable identities — an
    /// attacker who knows the seed owns the key. Production code MUST use
    /// [`Identity::generate`] (OS CSPRNG). It exists so the simulator can get
    /// **reproducible** runs. (Message nonces and bundle ids still come from the
    /// OS CSPRNG regardless — deterministic nonces would be a crypto footgun — so
    /// sim runs are statistically, not bit-for-bit, reproducible.)
    #[doc(hidden)]
    pub fn generate_from_rng<R: rand_core::CryptoRngCore + ?Sized>(rng: &mut R, now: u64) -> Self {
        let signing = SigningKey::generate(rng);
        let mut kex_bytes = [0u8; 32];
        rng.fill_bytes(&mut kex_bytes);
        let kex_secret = XSecret::from(kex_bytes);
        kex_bytes.zeroize(); // wipe the raw secret copy off the stack
        let address = Self::derive_address(&signing.verifying_key());
        Identity {
            signing,
            kex_secret,
            address,
            created_at: now,
            display_name: None,
        }
    }

    /// Reconstruct an identity from raw secret material (used by backup restore
    /// and secure-storage load). `sign_secret` and `kex_secret` are 32 bytes each.
    pub fn from_secret_bytes(
        sign_secret: &[u8],
        kex_secret: &[u8],
        created_at: u64,
        display_name: Option<String>,
    ) -> Result<Self> {
        let mut sign_arr: [u8; 32] = sign_secret
            .try_into()
            .map_err(|_| CoreError::BadKey("ed25519 secret len".into()))?;
        let mut kex_arr: [u8; 32] = kex_secret
            .try_into()
            .map_err(|_| CoreError::BadKey("x25519 secret len".into()))?;
        let signing = SigningKey::from_bytes(&sign_arr);
        let kex_secret = XSecret::from(kex_arr);
        sign_arr.zeroize(); // wipe the raw secret copies off the stack
        kex_arr.zeroize();
        let address = Self::derive_address(&signing.verifying_key());
        Ok(Identity {
            signing,
            kex_secret,
            address,
            created_at,
            display_name,
        })
    }

    /// Address = `blake3(sign_pub)[..16]` (FR-2).
    pub fn derive_address(vk: &VerifyingKey) -> Address {
        let h = crypto::blake3_32(vk.as_bytes());
        Address::from_hash_slice(&h).expect("32 >= 16")
    }

    pub fn set_display_name(&mut self, name: Option<String>) {
        self.display_name = name;
    }

    pub fn address(&self) -> &Address {
        &self.address
    }

    pub fn created_at(&self) -> u64 {
        self.created_at
    }

    /// The Ed25519 verifying (public) key.
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing.verifying_key()
    }

    /// The X25519 public key.
    pub fn kex_public(&self) -> XPublic {
        XPublic::from(&self.kex_secret)
    }

    /// Borrow the X25519 secret (used by [`crate::message`] to unseal).
    pub(crate) fn kex_secret(&self) -> &XSecret {
        &self.kex_secret
    }

    /// Derive a stable 32-byte secret for a subsystem that needs its own keypair
    /// — e.g. a **bearer-level Nostr identity** — without ever exposing the
    /// long-term signing secret. Domain-separated, so distinct `domain`s yield
    /// independent, mutually-unlinkable keys, and deterministic, so the derived
    /// account (and thus its offline mailbox address) survives restarts.
    pub fn derive_subkey(&self, domain: &[u8]) -> [u8; 32] {
        let mut sk = self.signing.to_bytes();
        let mut input = Vec::with_capacity(18 + domain.len() + 32);
        input.extend_from_slice(crate::domain::SUBKEY);
        input.extend_from_slice(domain);
        input.extend_from_slice(&sk);
        let out = crypto::blake3_32(&input);
        sk.zeroize(); // scrub the copies of the signing secret
        input.zeroize();
        out
    }

    /// The public identity record for sharing (QR, announces) (FR-3, §11.1).
    pub fn public(&self) -> IdentityPublic {
        IdentityPublic {
            id: self.address.clone(),
            sign_pub: Bytes::new(self.verifying_key().as_bytes().to_vec()),
            kex_pub: Bytes::new(self.kex_public().as_bytes().to_vec()),
            display_name: self.display_name.clone(),
            created_at: self.created_at,
            // The long-term record carries no prekey; a node's engine injects its
            // current rotating prekey into its beacon (forward secrecy).
            prekey: None,
        }
    }

    /// Sign a message with the identity's Ed25519 key.
    pub fn sign(&self, msg: &[u8]) -> Bytes {
        Bytes::new(self.signing.sign(msg).to_bytes().to_vec())
    }

    /// Short fingerprint for out-of-band verification (FR-3).
    pub fn fingerprint(&self) -> String {
        self.address.to_text()
    }
}

impl Drop for Identity {
    fn drop(&mut self) {
        // ed25519/x25519 dalek types zeroize themselves; be explicit for clarity.
        // (SigningKey and StaticSecret both impl Zeroize/ZeroizeOnDrop.)
    }
}

/// Verify an Ed25519 signature given the raw 32-byte public key (used by relays
/// and recipients to check headers, receipts, and log entries).
/// Derive the network address a raw Ed25519 public key maps to (FR-2). Used to
/// bind a claimed address to the key that signed for it.
pub fn address_of(sign_pub: &[u8]) -> Result<lifeline_proto::Address> {
    let arr: [u8; 32] = sign_pub
        .try_into()
        .map_err(|_| CoreError::BadKey("ed25519 pub len".into()))?;
    let vk = VerifyingKey::from_bytes(&arr).map_err(|e| CoreError::BadKey(e.to_string()))?;
    Ok(Identity::derive_address(&vk))
}

pub fn verify_sig(sign_pub: &[u8], msg: &[u8], sig: &[u8]) -> Result<()> {
    let vk_arr: [u8; 32] = sign_pub
        .try_into()
        .map_err(|_| CoreError::BadKey("ed25519 pub len".into()))?;
    let vk = VerifyingKey::from_bytes(&vk_arr).map_err(|e| CoreError::BadKey(e.to_string()))?;
    let sig_arr: [u8; 64] = sig
        .try_into()
        .map_err(|_| CoreError::BadKey("ed25519 sig len".into()))?;
    let signature = Signature::from_bytes(&sig_arr);
    vk.verify(msg, &signature)
        .map_err(|_| CoreError::BadSignature)
}

/// An encrypted, passphrase-wrapped key backup (FR-5).
///
/// The passphrase is stretched with **Argon2id** (memory-hard, resists brute
/// force) into an AEAD key that wraps the two secret keys. This is what a user
/// exports to move identities between devices; the file is useless without the
/// passphrase.
#[derive(Clone, Serialize, Deserialize)]
pub struct KeyBackup {
    /// Format version.
    pub v: u8,
    /// Argon2 salt.
    pub salt: Bytes,
    /// AEAD nonce.
    pub nonce: Bytes,
    /// AEAD ciphertext over the CBOR of the secret material.
    pub ct: Bytes,
    pub created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// The plaintext inside a [`KeyBackup`], never persisted unencrypted.
#[derive(Serialize, Deserialize, Zeroize)]
#[zeroize(drop)]
struct BackupSecret {
    sign_secret: Vec<u8>,
    kex_secret: Vec<u8>,
}

impl KeyBackup {
    /// Derive an AEAD key from a passphrase + salt using Argon2id.
    fn derive_key(passphrase: &[u8], salt: &[u8]) -> Result<[u8; KEY_LEN]> {
        let mut key = [0u8; KEY_LEN];
        Argon2::default()
            .hash_password_into(passphrase, salt, &mut key)
            .map_err(|e| CoreError::Backup(e.to_string()))?;
        Ok(key)
    }

    /// Encrypt `identity`'s secret keys under `passphrase`.
    pub fn create(identity: &Identity, passphrase: &str) -> Result<KeyBackup> {
        let salt = crypto::random_bytes(16);
        let mut key = Self::derive_key(passphrase.as_bytes(), &salt)?;

        let secret = BackupSecret {
            sign_secret: identity.signing.to_bytes().to_vec(),
            kex_secret: identity.kex_secret.to_bytes().to_vec(),
        };
        let pt = lifeline_proto::codec::to_cbor(&secret)?;

        let nonce_v = crypto::random_bytes(NONCE_LEN);
        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(&nonce_v);
        let ct = crypto::aead_seal(&key, &nonce, crate::domain::BACKUP, &pt);
        key.zeroize();

        Ok(KeyBackup {
            v: 1,
            salt: Bytes::new(salt),
            nonce: Bytes::new(nonce_v),
            ct: Bytes::new(ct),
            created_at: identity.created_at,
            display_name: identity.display_name.clone(),
        })
    }

    /// Decrypt and restore an [`Identity`] from this backup with `passphrase`.
    pub fn restore(&self, passphrase: &str) -> Result<Identity> {
        let mut key = Self::derive_key(passphrase.as_bytes(), self.salt.as_slice())?;
        let nonce: [u8; NONCE_LEN] = self
            .nonce
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::Backup("nonce len".into()))?;
        let pt = crypto::aead_open(&key, &nonce, crate::domain::BACKUP, self.ct.as_slice())
            .map_err(|_| CoreError::Backup("wrong passphrase or corrupt backup".into()))?;
        key.zeroize();

        let secret: BackupSecret = lifeline_proto::codec::from_cbor(&pt)?;
        Identity::from_secret_bytes(
            &secret.sign_secret,
            &secret.kex_secret,
            self.created_at,
            self.display_name.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_is_stable_and_derived_from_pubkey() {
        let id = Identity::generate(1000);
        let again = Identity::derive_address(&id.verifying_key());
        assert_eq!(id.address(), &again);
    }

    #[test]
    fn sign_and_verify() {
        let id = Identity::generate(1000);
        let msg = b"i am safe";
        let sig = id.sign(msg);
        assert!(verify_sig(id.verifying_key().as_bytes(), msg, sig.as_slice()).is_ok());
        assert!(verify_sig(id.verifying_key().as_bytes(), b"tampered", sig.as_slice()).is_err());
    }

    #[test]
    fn backup_roundtrip() {
        let mut id = Identity::generate(1000);
        id.set_display_name(Some("Asha".into()));
        let backup = KeyBackup::create(&id, "correct horse battery staple").unwrap();
        let restored = backup.restore("correct horse battery staple").unwrap();
        assert_eq!(id.address(), restored.address());
        assert_eq!(id.public(), restored.public());
    }

    #[test]
    fn backup_wrong_passphrase_fails() {
        let id = Identity::generate(1000);
        let backup = KeyBackup::create(&id, "right").unwrap();
        assert!(backup.restore("wrong").is_err());
    }
}
