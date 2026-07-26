//! **Geocast** — sealing a message to a *region* instead of an identity.
//!
//! A geocast reaches "anyone near a location" — strangers you hold no key for
//! ("SOS to anyone within 2 km"). The trick: derive a **deterministic keypair
//! from the region's geohash**, so any node that knows the geohash (i.e. any node
//! *in* the region computes the same one) can open the message. The sender seals
//! to that region key with the ordinary [`seal_bundle`](crate::message::seal_bundle);
//! a receiver in the region opens it with the derived region secret.
//!
//! This reuses the entire E2E path unchanged — the "recipient" is simply the
//! region. Sender identity is still authenticated (sealed-sender), so recipients
//! learn *who* sent it. The honest tradeoff, identical to BitChat's per-geohash
//! keys: a geocast is **not confidential against anyone who knows or guesses the
//! location** — which is the intended semantics of a regional broadcast.

use crate::crypto::hkdf_sha256;
use crate::message::{open_bundle_kex, Opened};
use crate::{domain, Result};
use lifeline_proto::{Address, Bundle, Bytes, IdentityPublic};
use x25519_dalek::{PublicKey as XPublic, StaticSecret as XSecret};
use zeroize::Zeroize;

/// The deterministic X25519 secret for a geohash `region`.
fn region_secret(region: &str) -> XSecret {
    let derived = hkdf_sha256(region.as_bytes(), domain::GEOCAST_KEY, b"x25519", 32);
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&derived);
    let secret = XSecret::from(seed);
    seed.zeroize();
    secret
}

/// The network address a geocast to `region` is sent to. A node matches its own
/// region cell against a bundle's `dst` with this — no wire field needed.
pub fn region_dst(region: &str) -> Address {
    let mut input = Vec::with_capacity(domain::GEOCAST_ADDR.len() + region.len());
    input.extend_from_slice(domain::GEOCAST_ADDR);
    input.extend_from_slice(region.as_bytes());
    let h = crate::crypto::blake3_32(&input);
    Address::from_hash_slice(&h).expect("32 >= 16")
}

/// The synthetic recipient a sender seals a geocast to (feed this to
/// [`seal_bundle`](crate::message::seal_bundle)). The region does not sign, so
/// `sign_pub` is empty and unused.
pub fn region_recipient(region: &str) -> IdentityPublic {
    let public = XPublic::from(&region_secret(region));
    IdentityPublic {
        id: region_dst(region),
        sign_pub: Bytes::new(Vec::new()),
        kex_pub: Bytes::new(public.as_bytes().to_vec()),
        display_name: None,
        created_at: 0,
        prekey: None,
    }
}

/// Open a bundle geocast to `region`, using the derived region secret. Fails if
/// the bundle isn't addressed to (or sealed to) this region.
pub fn open_region(region: &str, bundle: &Bundle) -> Result<Opened> {
    open_bundle_kex(&region_dst(region), &region_secret(region), bundle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{seal_bundle, SealOptions};
    use crate::Identity;
    use lifeline_proto::{Payload, PayloadKind};

    fn text(body: &str) -> Payload {
        Payload {
            kind: PayloadKind::Text,
            body: Some(body.into()),
            coords: None,
            battery_pct: None,
            attach: None,
            group_id: None,
        }
    }

    #[test]
    fn region_key_is_deterministic_and_addressable() {
        // Same geohash → same address + key; different geohash → different.
        assert_eq!(region_dst("u4pruyd"), region_dst("u4pruyd"));
        assert_ne!(region_dst("u4pruyd"), region_dst("u4pruye"));
        assert_eq!(
            region_recipient("gcpvj0").kex_pub,
            region_recipient("gcpvj0").kex_pub
        );
    }

    #[test]
    fn anyone_in_the_region_can_open_a_geocast() {
        let sender = Identity::generate(0);
        let region = "gcpvj0"; // a geohash cell
        let recipient = region_recipient(region);
        let opts = SealOptions::normal(0);
        let bundle = seal_bundle(
            &sender,
            &recipient,
            &text("shelter open at the school"),
            &opts,
        )
        .expect("seal");

        // A node that knows the region (i.e. is in it) opens it and learns the sender.
        let opened = open_region(region, &bundle).expect("open");
        assert_eq!(
            opened.payload.body.as_deref(),
            Some("shelter open at the school")
        );
        assert_eq!(opened.sender.id, sender.address().clone());

        // A node in a *different* region cannot open it.
        assert!(open_region("gcpvj1", &bundle).is_err());
    }
}
