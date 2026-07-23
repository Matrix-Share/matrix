//! Project Lifeline — decentralized core (PRD layers L2 & L5).
//!
//! This crate is the cryptographic heart the PRD calls out: it runs **fully
//! offline** and is the only place plaintext ever exists (endpoints only). It
//! provides:
//!
//! * [`identity`] — self-sovereign keypair identity; address = hash(pubkey)
//!   (FR-1..FR-5).
//! * [`crypto`] — audited-primitive building blocks (X25519 ECDH, HKDF-SHA256,
//!   XChaCha20-Poly1305, Ed25519). No custom primitives (PRD §15 gate).
//! * [`message`] — sealing a [`proto::Payload`] into a [`proto::Bundle`] with
//!   sealed sender (FR-44, FR-45) and unsealing it on the recipient.
//! * [`log`] — append-only hash-linked per-identity log (FR-30).
//! * [`receipt`] — signed delivery receipts + **offline** verification of
//!   delivery (FR-31..FR-34, §12.4).
//!
//! ## Engineering note on the message ratchet (OQ3)
//! The PRD's L2 names "X3DH + Double Ratchet". The Double Ratchet's security
//! proof assumes roughly in-order delivery, which a delay-tolerant network with
//! multi-day one-way delays and reordering violates (PRD OQ3, and the linked
//! Cohn-Gordon 2017 caveat). Phase 1 therefore ships a **stateless
//! ephemeral-static sealed-box** construction (see [`message`]): forward-secret
//! against later ephemeral-key compromise, authenticated, sealed-sender, and —
//! crucially — tolerant of arbitrary reordering and long delays. A DTN-tuned
//! ratchet is tracked as follow-up work behind the [`crypto::SecureChannel`]
//! seam rather than shipping a ratchet that breaks under DTN conditions.

pub mod content;
pub mod crypto;
pub mod erasure;
pub mod group;
pub mod identity;
pub mod log;
pub mod message;
pub mod receipt;

pub use identity::{Identity, KeyBackup};
pub use log::HashLog;
pub use message::{Opened, SealOptions};

/// Errors surfaced by the core crate.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("crypto: {0}")]
    Crypto(String),
    #[error("bad signature")]
    BadSignature,
    #[error("decrypt failed (wrong key or tampered ciphertext)")]
    Decrypt,
    #[error("malformed key material: {0}")]
    BadKey(String),
    #[error("wire codec: {0}")]
    Codec(#[from] lifeline_proto::CodecError),
    #[error("log integrity: {0}")]
    Log(String),
    #[error("key backup: {0}")]
    Backup(String),
}

/// Convenience result alias.
pub type Result<T> = std::result::Result<T, CoreError>;
