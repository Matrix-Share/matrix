//! Sealing a payload into a [`Bundle`] and opening it (PRD §12.1 steps 1–2, 6).
//!
//! This is the only code that turns plaintext into wire bundles and back. Every
//! relay above L2 sees only the resulting ciphertext + routing header (FR-44).
//!
//! Envelope layout produced here:
//! * `ciphertext`  — the CBOR payload, sealed to the recipient's X25519 key.
//! * `src_sealed`  — the sender's identity (`sign_pub`, `kex_pub`, name),
//!   sealed to the same recipient so **only the recipient learns who sent it**
//!   (sealed sender, FR-45). Relays cannot.
//! * `sig`         — the sender's Ed25519 signature over the immutable header;
//!   verifiable by the recipient *after* unsealing the sender key, which is
//!   what defeats impersonation/MITM (FR-6 AC, FR-44).
//!
//! Mutable in-flight fields (`hops`, `copies_left`) are excluded from the signed
//! header so relays can legitimately update them without breaking the signature.

use crate::crypto::{SealedBox, SecureChannel};
use crate::identity::{verify_sig, Identity, INFO_MSG, INFO_SENDER};
use crate::{CoreError, Result};
use lifeline_proto::{
    codec, Address, Bundle, Bytes, IdentityPublic, Payload, Priority, WIRE_VERSION,
};
use serde::{Deserialize, Serialize};

/// Default bundle parameters (PRD Appendix B).
pub const DEFAULT_TTL_S: u64 = 7 * 24 * 3600; // 7 days
pub const DEFAULT_HOP_LIMIT: u16 = 32;
pub const DEFAULT_SPRAY_COPIES: u16 = 6;

/// Knobs for sealing a bundle.
#[derive(Debug, Clone)]
pub struct SealOptions {
    pub priority: Priority,
    pub ttl_s: u64,
    pub hop_limit: u16,
    pub copies_left: u16,
    pub created_at: u64,
}

impl SealOptions {
    /// Sensible defaults for a normal message at time `now`.
    pub fn normal(now: u64) -> Self {
        SealOptions {
            priority: Priority::Normal,
            ttl_s: DEFAULT_TTL_S,
            hop_limit: DEFAULT_HOP_LIMIT,
            copies_left: DEFAULT_SPRAY_COPIES,
            created_at: now,
        }
    }
    /// SOS defaults: highest priority (FR-40). SOS still uses normal TTL/spray.
    pub fn sos(now: u64) -> Self {
        SealOptions {
            priority: Priority::Sos,
            ..Self::normal(now)
        }
    }
    pub fn with_priority(mut self, p: Priority) -> Self {
        self.priority = p;
        self
    }
}

/// The sealed-sender inner record (never visible to relays).
#[derive(Serialize, Deserialize)]
struct SenderAuth {
    sign_pub: Bytes,
    kex_pub: Bytes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
}

/// Immutable header fields, in canonical order, that the sender signs.
/// (Excludes `hops`/`copies_left`, which relays mutate.)
#[derive(Serialize)]
struct HeaderForSig<'a> {
    v: u8,
    bundle_id: &'a Bytes,
    dst: &'a Address,
    priority: Priority,
    created_at: u64,
    ttl_s: u64,
    hop_limit: u16,
    ciphertext_hash: [u8; 32],
    src_sealed_hash: [u8; 32],
}

#[allow(clippy::too_many_arguments)]
fn header_signing_bytes(
    bundle_id: &Bytes,
    dst: &Address,
    priority: Priority,
    created_at: u64,
    ttl_s: u64,
    hop_limit: u16,
    ciphertext: &Bytes,
    src_sealed: &Bytes,
) -> Result<Vec<u8>> {
    let h = HeaderForSig {
        v: WIRE_VERSION,
        bundle_id,
        dst,
        priority,
        created_at,
        ttl_s,
        hop_limit,
        ciphertext_hash: crate::crypto::blake3_32(ciphertext.as_slice()),
        src_sealed_hash: crate::crypto::blake3_32(src_sealed.as_slice()),
    };
    Ok(codec::to_cbor(&h)?)
}

/// Recompute the canonical header signing bytes for an existing bundle.
///
/// Used at open time and by [`crate::receipt`] to verify a bundle's sender
/// signature during offline delivery proof (§12.4).
pub fn header_signing_bytes_of(b: &Bundle) -> Result<Vec<u8>> {
    header_signing_bytes(
        &b.bundle_id,
        &b.dst,
        b.priority,
        b.created_at,
        b.ttl_s,
        b.hop_limit,
        &b.ciphertext,
        &b.src_sealed,
    )
}

/// Seal `payload` from `sender` to `recipient` (identified by public record).
///
/// `recipient` must carry the recipient's `sign_pub` (address binding) and
/// `kex_pub` (encryption). Returns a ready-to-route [`Bundle`].
pub fn seal_bundle(
    sender: &Identity,
    recipient: &IdentityPublic,
    payload: &Payload,
    opts: &SealOptions,
) -> Result<Bundle> {
    let recipient_kex = crate::crypto::x25519_pub_from_slice(recipient.kex_pub.as_slice())?;

    // Unique bundle id for dedup (FR-26). Random 16 bytes → collision-safe.
    let bundle_id = Bytes::new(crate::crypto::random_bytes(16));

    // 1. Encrypt payload to recipient, binding the bundle id as AD.
    let payload_bytes = codec::to_cbor(payload)?;
    let ciphertext = Bytes::new(SealedBox::seal(
        &recipient_kex,
        bundle_id.as_slice(),
        &payload_bytes,
        INFO_MSG,
    ));

    // 2. Seal the sender identity to the recipient (sealed sender, FR-45).
    let sender_auth = SenderAuth {
        sign_pub: Bytes::new(sender.verifying_key().as_bytes().to_vec()),
        kex_pub: Bytes::new(sender.kex_public().as_bytes().to_vec()),
        display_name: sender.public().display_name,
    };
    let sender_auth_bytes = codec::to_cbor(&sender_auth)?;
    let src_sealed = Bytes::new(SealedBox::seal(
        &recipient_kex,
        bundle_id.as_slice(),
        &sender_auth_bytes,
        INFO_SENDER,
    ));

    // 3. Sign the immutable header.
    let signing_bytes = header_signing_bytes(
        &bundle_id,
        &recipient.id,
        opts.priority,
        opts.created_at,
        opts.ttl_s,
        opts.hop_limit,
        &ciphertext,
        &src_sealed,
    )?;
    let sig = sender.sign(&signing_bytes);

    // Mint PoW postage sized to the priority (SOS exempt) so relays that gate
    // admission will accept it (FR-46, §12.5).
    let postage = lifeline_proto::pow::mint_for_priority(bundle_id.as_slice(), opts.priority);

    Ok(Bundle {
        v: WIRE_VERSION,
        bundle_id,
        dst: recipient.id.clone(),
        src_sealed,
        priority: opts.priority,
        created_at: opts.created_at,
        ttl_s: opts.ttl_s,
        hop_limit: opts.hop_limit,
        hops: 0,
        copies_left: opts.copies_left,
        ciphertext,
        sig,
        postage,
        frag: None,
    })
}

/// The result of opening a bundle at the recipient.
#[derive(Debug, Clone)]
pub struct Opened {
    /// The verified sender's public identity (address re-derived from `sign_pub`).
    pub sender: IdentityPublic,
    /// The decrypted payload.
    pub payload: Payload,
}

/// Open a bundle addressed to `recipient`, verifying sender authenticity.
///
/// Steps (PRD §12.1.6, §12.4):
/// 1. Unseal the sender identity.
/// 2. Verify the header signature with the unsealed sender key — this is the
///    anti-impersonation check (a MITM cannot forge it, FR-6 AC).
/// 3. Decrypt the payload.
pub fn open_bundle(recipient: &Identity, bundle: &Bundle) -> Result<Opened> {
    if bundle.v != WIRE_VERSION {
        return Err(CoreError::Codec(lifeline_proto::CodecError::Decode(
            format!("unsupported wire version {}", bundle.v),
        )));
    }
    if &bundle.dst != recipient.address() {
        return Err(CoreError::Decrypt); // not for us
    }

    // 1. Unseal sender identity.
    let sender_auth_bytes = SealedBox::open(
        recipient.kex_secret(),
        bundle.bundle_id.as_slice(),
        bundle.src_sealed.as_slice(),
        INFO_SENDER,
    )?;
    let sender_auth: SenderAuth = codec::from_cbor(&sender_auth_bytes)?;

    // 2. Verify header signature with the sender's real key.
    let signing_bytes = header_signing_bytes_of(bundle)?;
    verify_sig(
        sender_auth.sign_pub.as_slice(),
        &signing_bytes,
        bundle.sig.as_slice(),
    )?;

    // Re-derive the sender address from its signing key (binding check).
    let sender_vk_arr: [u8; 32] = sender_auth
        .sign_pub
        .as_slice()
        .try_into()
        .map_err(|_| CoreError::BadKey("sender sign_pub len".into()))?;
    let sender_vk = ed25519_dalek::VerifyingKey::from_bytes(&sender_vk_arr)
        .map_err(|e| CoreError::BadKey(e.to_string()))?;
    let sender_address = Identity::derive_address(&sender_vk);

    // 3. Decrypt payload.
    let payload_bytes = SealedBox::open(
        recipient.kex_secret(),
        bundle.bundle_id.as_slice(),
        bundle.ciphertext.as_slice(),
        INFO_MSG,
    )?;
    let payload: Payload = codec::from_cbor(&payload_bytes)?;

    Ok(Opened {
        sender: IdentityPublic {
            id: sender_address,
            sign_pub: sender_auth.sign_pub,
            kex_pub: sender_auth.kex_pub,
            display_name: sender_auth.display_name,
            created_at: 0,
        },
        payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lifeline_proto::{Coords, PayloadKind};

    fn sos_payload() -> Payload {
        Payload {
            kind: PayloadKind::Sos,
            body: Some("trapped, 2nd floor".into()),
            coords: Some(Coords {
                lat: 12.97,
                lon: 77.59,
                acc_m: 6,
            }),
            battery_pct: Some(19),
            attach: None,
            group_id: None,
        }
    }

    #[test]
    fn seal_open_roundtrip_and_sender_authenticated() {
        let alice = Identity::generate(1000);
        let bob = Identity::generate(1000);
        let bundle = seal_bundle(
            &alice,
            &bob.public(),
            &sos_payload(),
            &SealOptions::sos(2000),
        )
        .unwrap();

        // Relay-visible fields carry no plaintext.
        assert_eq!(bundle.dst, *bob.address());
        assert_eq!(bundle.priority, Priority::Sos);

        let opened = open_bundle(&bob, &bundle).unwrap();
        assert_eq!(opened.payload, sos_payload());
        // Sealed sender: recipient recovers Alice's real address.
        assert_eq!(&opened.sender.id, alice.address());
    }

    #[test]
    fn wrong_recipient_cannot_open() {
        let alice = Identity::generate(1000);
        let bob = Identity::generate(1000);
        let eve = Identity::generate(1000);
        let bundle = seal_bundle(
            &alice,
            &bob.public(),
            &sos_payload(),
            &SealOptions::normal(2000),
        )
        .unwrap();
        assert!(open_bundle(&eve, &bundle).is_err());
    }

    #[test]
    fn tampered_header_is_rejected() {
        let alice = Identity::generate(1000);
        let bob = Identity::generate(1000);
        let mut bundle = seal_bundle(
            &alice,
            &bob.public(),
            &sos_payload(),
            &SealOptions::normal(2000),
        )
        .unwrap();
        // Attacker downgrades priority — signature must fail.
        bundle.priority = Priority::Bulk;
        assert!(open_bundle(&bob, &bundle).is_err());
    }

    #[test]
    fn mutable_fields_do_not_break_signature() {
        let alice = Identity::generate(1000);
        let bob = Identity::generate(1000);
        let mut bundle = seal_bundle(
            &alice,
            &bob.public(),
            &sos_payload(),
            &SealOptions::normal(2000),
        )
        .unwrap();
        // A relay legitimately updates hops/copies_left in flight.
        bundle.hops = 5;
        bundle.copies_left = 2;
        assert!(open_bundle(&bob, &bundle).is_ok());
    }
}
