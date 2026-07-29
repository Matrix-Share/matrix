//! **Central registry of cryptographic domain-separation labels.**
//!
//! Every AEAD `info`/`ad` string and every "bytes to sign" prefix used anywhere
//! in `core` is declared here, once. Scattering these across modules invited two
//! failure modes the review flagged: the *same* name (`INFO_MSG`) meaning two
//! different things in two files, and silent drift when a new subsystem reuses a
//! label. Keeping them in one exhaustive list makes a collision obvious at a
//! glance and a version bump a single edit.
//!
//! **Do not change an existing value** without bumping its version suffix — these
//! bytes are mixed into ciphertexts and signatures, so altering one invalidates
//! every artifact ever produced with it. New labels must be unique. Two labels
//! (`ONION_LAYER_AD`, `PREKEY_AD`) predate the `/v1` convention and are kept
//! verbatim so existing artifacts still verify; new labels should follow the
//! `lifeline/v1/<purpose>` shape.

/// Sealed-box `info` for a message payload (`message::seal_bundle`).
pub const MESSAGE: &[u8] = b"lifeline/v1/message";
/// Sealed-box `info` for the sealed-sender envelope (`message`).
pub const SEALED_SENDER: &[u8] = b"lifeline/v1/sealed-sender";
/// KDF label for identity subkey derivation (`Identity::derive_subkey`).
pub const SUBKEY: &[u8] = b"lifeline/subkey/v1";
/// AEAD `ad` for an encrypted key backup (`identity::KeyBackup`).
pub const BACKUP: &[u8] = b"lifeline/v1/backup";

/// Sender-keys ratchet: message-key derivation (`group`).
pub const GROUP_MSG: &[u8] = b"lifeline/v1/group-msg";
/// Sender-keys ratchet: next-chain-key derivation (`group`).
pub const GROUP_CHAIN: &[u8] = b"lifeline/v1/group-chain";
/// Sealed-box `info` for a sender-key distribution message (`group`).
pub const GROUP_SENDERKEY: &[u8] = b"lifeline/v1/group-senderkey";

/// Signature domain for a delivery receipt (`receipt`).
pub const DELIVERY_RECEIPT: &[u8] = b"lifeline/v1/delivery-receipt";
/// Sealed-box `info` for an onion layer (`onion`).
pub const ONION_SEAL: &[u8] = b"lifeline/v1/onion";
/// AEAD `ad` for an onion layer (`onion`). Pre-`/v1`; kept verbatim.
pub const ONION_LAYER_AD: &[u8] = b"lifeline/onion-layer";
/// AEAD `ad` for the at-rest state vault (`vault`).
pub const VAULT: &[u8] = b"lifeline/v1/vault";
/// Signature domain for a relay-credit proof (`relay_proof`).
pub const RELAY_CREDIT: &[u8] = b"lifeline/v1/relay-credit";
/// Signature domain for a signed alert (`alert`).
pub const ALERT: &[u8] = b"lifeline/v1/alert";

/// HKDF salt for deriving a geocast region's X25519 key from its geohash
/// (`geocast`). Anyone who knows the geohash derives the same key.
/// Rotating rendezvous address for recipient-unlinkable DTN addressing (G2).
pub const RENDEZVOUS: &[u8] = b"lifeline/v1/rendezvous";

/// Identity key-rotation certificate (old key attests a successor) (G4).
pub const KEY_ROTATION: &[u8] = b"lifeline/v1/key-rotation";

/// Identity revocation certificate (retire a key with no successor) (G4).
pub const KEY_REVOCATION: &[u8] = b"lifeline/v1/key-revocation";

pub const GEOCAST_KEY: &[u8] = b"lifeline/v1/geocast-key";
/// Prefix for deriving a geocast region's network address from its geohash.
pub const GEOCAST_ADDR: &[u8] = b"lifeline/v1/geocast-addr";

/// Signature domain for a signed prekey (`prekey`).
pub const PREKEY_SIGN: &[u8] = b"lifeline/v1/prekey";
/// Sealed-box `info` for a prekey-sealed message (`prekey`).
pub const PREKEY_SEAL: &[u8] = b"lifeline/v1/prekey-seal";
/// AEAD `ad` for a prekey-sealed message (`prekey`). Pre-`/v1`; kept verbatim.
pub const PREKEY_AD: &[u8] = b"lifeline/prekey";

#[cfg(test)]
mod tests {
    use super::*;

    /// Every declared label must be distinct — a collision would let one
    /// subsystem's artifact be mistaken for another's.
    #[test]
    fn all_domain_labels_are_unique() {
        let all: &[&[u8]] = &[
            MESSAGE,
            SEALED_SENDER,
            SUBKEY,
            BACKUP,
            GROUP_MSG,
            GROUP_CHAIN,
            GROUP_SENDERKEY,
            DELIVERY_RECEIPT,
            ONION_SEAL,
            ONION_LAYER_AD,
            VAULT,
            RELAY_CREDIT,
            ALERT,
            PREKEY_SIGN,
            PREKEY_SEAL,
            PREKEY_AD,
            GEOCAST_KEY,
            GEOCAST_ADDR,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a, b, "duplicate domain-separation label: {:?}", a);
            }
        }
    }
}
