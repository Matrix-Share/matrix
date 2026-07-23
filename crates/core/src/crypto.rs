//! Audited-primitive building blocks and the E2E channel seam.
//!
//! Every primitive here is a thin wrapper over a vetted crate — we compose,
//! we do not invent (PRD §15 security gate: "no custom crypto primitives").
//!
//! * Signing / identity: **Ed25519** (`ed25519-dalek`).
//! * Key agreement: **X25519** (`x25519-dalek`).
//! * KDF: **HKDF-SHA256** (`hkdf` + `sha2`).
//! * AEAD: **XChaCha20-Poly1305** (`chacha20poly1305`) — 24-byte random nonces
//!   make nonce reuse a non-issue for the stateless per-message sealing used by
//!   [`crate::message`], which matters in a DTN where senders may be offline
//!   and cannot coordinate counters.
//! * Hashing / addressing: **BLAKE3** (`blake3`).

use crate::{CoreError, Result};
use chacha20poly1305::aead::{Aead, KeyInit, Payload as AeadPayload};
use chacha20poly1305::XChaCha20Poly1305;
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use x25519_dalek::{EphemeralSecret, PublicKey as XPublic, StaticSecret as XSecret};

/// Length of an AEAD symmetric key.
pub const KEY_LEN: usize = 32;
/// XChaCha20-Poly1305 nonce length.
pub const NONCE_LEN: usize = 24;
/// X25519 public/secret key length.
pub const X25519_LEN: usize = 32;

/// Fill a fresh random buffer of length `n` using the OS CSPRNG.
pub fn random_bytes(n: usize) -> Vec<u8> {
    let mut v = vec![0u8; n];
    rand::rngs::OsRng.fill_bytes(&mut v);
    v
}

/// BLAKE3 hash → 32 bytes. Used for bundle ids, log links, and address
/// derivation.
pub fn blake3_32(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

/// HKDF-SHA256 expand to `out_len` bytes.
pub fn hkdf_sha256(ikm: &[u8], salt: &[u8], info: &[u8], out_len: usize) -> Vec<u8> {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut out = vec![0u8; out_len];
    hk.expand(info, &mut out)
        .expect("hkdf out_len within bounds");
    out
}

/// AEAD seal with XChaCha20-Poly1305.
pub fn aead_seal(key: &[u8; KEY_LEN], nonce: &[u8; NONCE_LEN], ad: &[u8], pt: &[u8]) -> Vec<u8> {
    let cipher = XChaCha20Poly1305::new(key.into());
    cipher
        .encrypt(nonce.into(), AeadPayload { msg: pt, aad: ad })
        .expect("aead encrypt is infallible for valid key/nonce")
}

/// AEAD open with XChaCha20-Poly1305. Fails on tamper / wrong key.
pub fn aead_open(
    key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    ad: &[u8],
    ct: &[u8],
) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(key.into());
    cipher
        .decrypt(nonce.into(), AeadPayload { msg: ct, aad: ad })
        .map_err(|_| CoreError::Decrypt)
}

/// Parse a 32-byte X25519 public key from a slice.
pub fn x25519_pub_from_slice(b: &[u8]) -> Result<XPublic> {
    let arr: [u8; X25519_LEN] = b
        .try_into()
        .map_err(|_| CoreError::BadKey(format!("x25519 pub len {}", b.len())))?;
    Ok(XPublic::from(arr))
}

/// The E2E channel seam.
///
/// Phase 1 provides one implementation, [`SealedBox`], a stateless
/// ephemeral-static authenticated public-key encryption to a recipient's
/// long-term X25519 key. A future DTN-tuned ratchet (PRD OQ3) can implement the
/// same trait without touching the [`crate::message`] callers.
///
/// The returned blob is self-describing: `eph_pub(32) || nonce(24) || ct`.
pub trait SecureChannel {
    /// Encrypt `plaintext` to `recipient_kex_pub`. `ad` is authenticated but not
    /// encrypted (we bind the bundle id so ciphertext can't be replayed under a
    /// different envelope).
    fn seal(recipient_kex_pub: &XPublic, ad: &[u8], plaintext: &[u8], info: &[u8]) -> Vec<u8>;

    /// Decrypt a blob produced by [`SecureChannel::seal`].
    fn open(recipient_kex_secret: &XSecret, ad: &[u8], blob: &[u8], info: &[u8])
        -> Result<Vec<u8>>;
}

/// Stateless ephemeral-static sealed box (ECIES-style) over X25519 + HKDF +
/// XChaCha20-Poly1305.
pub struct SealedBox;

impl SecureChannel for SealedBox {
    fn seal(recipient_kex_pub: &XPublic, ad: &[u8], plaintext: &[u8], info: &[u8]) -> Vec<u8> {
        // Fresh ephemeral keypair per message → forward secrecy against later
        // compromise of the ephemeral secret, and unlinkable nonces.
        let eph_secret = EphemeralSecret::random_from_rng(rand::rngs::OsRng);
        let eph_pub = XPublic::from(&eph_secret);
        let shared = eph_secret.diffie_hellman(recipient_kex_pub);

        // Bind both public keys into the KDF salt so a ciphertext is
        // cryptographically tied to this exact (ephemeral, recipient) pair.
        let mut salt = Vec::with_capacity(64);
        salt.extend_from_slice(eph_pub.as_bytes());
        salt.extend_from_slice(recipient_kex_pub.as_bytes());
        let key_vec = hkdf_sha256(shared.as_bytes(), &salt, info, KEY_LEN);
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&key_vec);

        let nonce_vec = random_bytes(NONCE_LEN);
        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(&nonce_vec);

        let ct = aead_seal(&key, &nonce, ad, plaintext);

        let mut blob = Vec::with_capacity(X25519_LEN + NONCE_LEN + ct.len());
        blob.extend_from_slice(eph_pub.as_bytes());
        blob.extend_from_slice(&nonce);
        blob.extend_from_slice(&ct);
        blob
    }

    fn open(
        recipient_kex_secret: &XSecret,
        ad: &[u8],
        blob: &[u8],
        info: &[u8],
    ) -> Result<Vec<u8>> {
        if blob.len() < X25519_LEN + NONCE_LEN {
            return Err(CoreError::Decrypt);
        }
        let eph_pub = x25519_pub_from_slice(&blob[..X25519_LEN])?;
        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(&blob[X25519_LEN..X25519_LEN + NONCE_LEN]);
        let ct = &blob[X25519_LEN + NONCE_LEN..];

        let shared = recipient_kex_secret.diffie_hellman(&eph_pub);
        let recipient_pub = XPublic::from(recipient_kex_secret);
        let mut salt = Vec::with_capacity(64);
        salt.extend_from_slice(eph_pub.as_bytes());
        salt.extend_from_slice(recipient_pub.as_bytes());
        let key_vec = hkdf_sha256(shared.as_bytes(), &salt, info, KEY_LEN);
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&key_vec);

        aead_open(&key, &nonce, ad, ct)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_box_roundtrip() {
        let recipient = XSecret::random_from_rng(rand::rngs::OsRng);
        let recipient_pub = XPublic::from(&recipient);
        let msg = b"one gateway lights the mesh";
        let ad = b"bundle-id-123";
        let blob = SealedBox::seal(&recipient_pub, ad, msg, b"test/info");
        let out = SealedBox::open(&recipient, ad, &blob, b"test/info").unwrap();
        assert_eq!(out, msg);
    }

    #[test]
    fn sealed_box_rejects_tamper() {
        let recipient = XSecret::random_from_rng(rand::rngs::OsRng);
        let recipient_pub = XPublic::from(&recipient);
        let mut blob = SealedBox::seal(&recipient_pub, b"ad", b"secret", b"i");
        let last = blob.len() - 1;
        blob[last] ^= 0x01; // flip a ciphertext bit
        assert!(SealedBox::open(&recipient, b"ad", &blob, b"i").is_err());
    }

    #[test]
    fn sealed_box_rejects_wrong_ad() {
        let recipient = XSecret::random_from_rng(rand::rngs::OsRng);
        let recipient_pub = XPublic::from(&recipient);
        let blob = SealedBox::seal(&recipient_pub, b"ad-1", b"secret", b"i");
        assert!(SealedBox::open(&recipient, b"ad-2", &blob, b"i").is_err());
    }
}
