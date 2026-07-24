//! Encrypted-at-rest storage (PRD FR-15 "local encrypted history"; FR-9
//! contact store).
//!
//! Anything the app persists — the contact directory, message history — is
//! sensitive and must be encrypted on disk (a seized or lost device must not
//! leak conversations). This module wraps a passphrase into a key with
//! **Argon2id** (memory-hard) once, then seals/opens blobs with
//! **XChaCha20-Poly1305**. The Argon2 cost is paid a single time at unlock; each
//! save reuses the derived [`Vault`] key with a fresh random nonce, so writing
//! is cheap enough to do continuously.

use crate::crypto::{self, KEY_LEN, NONCE_LEN};
use crate::{CoreError, Result};
use argon2::Argon2;
use lifeline_proto::Bytes;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

const VAULT_AD: &[u8] = b"lifeline/v1/vault";

/// An unlocked vault key + the salt it was derived from (so re-saves reuse it).
pub struct Vault {
    key: [u8; KEY_LEN],
    salt: Vec<u8>,
}

impl Drop for Vault {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

/// A sealed blob on disk: everything needed to re-derive the key and decrypt.
#[derive(Clone, Serialize, Deserialize)]
pub struct SealedBlob {
    pub v: u8,
    pub salt: Bytes,
    pub nonce: Bytes,
    pub ct: Bytes,
}

fn derive(passphrase: &[u8], salt: &[u8]) -> Result<[u8; KEY_LEN]> {
    let mut key = [0u8; KEY_LEN];
    Argon2::default()
        .hash_password_into(passphrase, salt, &mut key)
        .map_err(|e| CoreError::Backup(e.to_string()))?;
    Ok(key)
}

impl Vault {
    /// Create a fresh vault (new random salt) from a passphrase.
    pub fn create(passphrase: &str) -> Result<Vault> {
        let salt = crypto::random_bytes(16);
        let key = derive(passphrase.as_bytes(), &salt)?;
        Ok(Vault { key, salt })
    }

    /// Unlock an existing sealed blob, returning the vault (for re-saving) and
    /// the decrypted plaintext.
    pub fn unlock(passphrase: &str, blob: &SealedBlob) -> Result<(Vault, Vec<u8>)> {
        let key = derive(passphrase.as_bytes(), blob.salt.as_slice())?;
        let nonce: [u8; NONCE_LEN] = blob
            .nonce
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::Backup("nonce len".into()))?;
        let pt = crypto::aead_open(&key, &nonce, VAULT_AD, blob.ct.as_slice())
            .map_err(|_| CoreError::Backup("wrong passphrase or corrupt vault".into()))?;
        Ok((
            Vault {
                key,
                salt: blob.salt.0.clone(),
            },
            pt,
        ))
    }

    /// Seal `plaintext` for storage (fresh nonce, reused key/salt).
    pub fn seal(&self, plaintext: &[u8]) -> SealedBlob {
        let nonce_v = crypto::random_bytes(NONCE_LEN);
        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(&nonce_v);
        let ct = crypto::aead_seal(&self.key, &nonce, VAULT_AD, plaintext);
        SealedBlob {
            v: 1,
            salt: Bytes::new(self.salt.clone()),
            nonce: Bytes::new(nonce_v),
            ct: Bytes::new(ct),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_unlock_roundtrip() {
        let vault = Vault::create("correct horse battery staple").unwrap();
        let blob = vault.seal(b"contacts + history");
        let (_v2, pt) = Vault::unlock("correct horse battery staple", &blob).unwrap();
        assert_eq!(pt, b"contacts + history");
    }

    #[test]
    fn wrong_passphrase_fails() {
        let vault = Vault::create("right").unwrap();
        let blob = vault.seal(b"secret");
        assert!(Vault::unlock("wrong", &blob).is_err());
    }

    #[test]
    fn resave_with_unlocked_vault() {
        let vault = Vault::create("pw").unwrap();
        let blob1 = vault.seal(b"v1");
        let (vault2, _) = Vault::unlock("pw", &blob1).unwrap();
        // Re-saving with the reused key produces a blob openable by the same pw.
        let blob2 = vault2.seal(b"v2 updated");
        let (_, pt) = Vault::unlock("pw", &blob2).unwrap();
        assert_eq!(pt, b"v2 updated");
    }
}
