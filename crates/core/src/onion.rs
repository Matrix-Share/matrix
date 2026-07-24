//! Onion routing for metadata privacy (PRD FR-49, §8.9; gateway doc §5
//! "metadata privacy — multi-hop/onion wrapping so a gateway doesn't learn who
//! is talking to whom").
//!
//! End-to-end encryption already hides message *content* from relays, but a
//! plain bundle still exposes its destination address so relays can route it.
//! Onion routing hides the *route* too: the sender wraps the message in one
//! encryption layer per relay on a chosen path. Each relay can peel exactly one
//! layer — learning only the **next hop**, never the origin or (except the last
//! relay) the final recipient.
//!
//! Each layer is a stateless ephemeral-static [`SealedBox`] to that relay's
//! X25519 key (the same audited construction used for message sealing), so
//! peeling requires the relay's secret and nothing else — it works fully
//! offline, hop by hop.

use crate::crypto::{self, SealedBox, SecureChannel};
use crate::{CoreError, Result};
use lifeline_proto::codec::{from_cbor, to_cbor};
use lifeline_proto::{Address, Bytes, IdentityPublic};
use serde::{Deserialize, Serialize};

/// Domain separator for onion layers.
const INFO_ONION: &[u8] = b"lifeline/v1/onion";
const ONION_AD: &[u8] = b"lifeline/onion-layer";

/// One decrypted onion layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OnionLayer {
    /// The next hop to forward `inner` to, or `None` if we are the recipient.
    next: Option<Address>,
    /// The next (still-encrypted) layer, or — when `next` is `None` — the final
    /// plaintext payload for the recipient.
    inner: Bytes,
}

/// Result of peeling one onion layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Peeled {
    /// Forward `inner` (the remaining onion) to `next`.
    Forward { next: Address, inner: Bytes },
    /// You are the final recipient; here is the message.
    Deliver { payload: Vec<u8> },
}

/// Build an onion for the route `relays[0] → relays[1] → … → recipient`.
///
/// Returns `(first_hop, onion)`: send `onion` to `first_hop`. With an empty
/// `relays` slice the onion is a single layer to the recipient (a direct,
/// still-metadata-minimal send).
pub fn build_onion(
    relays: &[IdentityPublic],
    recipient: &IdentityPublic,
    payload: &[u8],
) -> Result<(Address, Bytes)> {
    // Innermost layer: sealed to the recipient, carrying the real payload.
    let recipient_kex = crypto::x25519_pub_from_slice(recipient.kex_pub.as_slice())?;
    let inner_layer = OnionLayer {
        next: None,
        inner: Bytes::new(payload.to_vec()),
    };
    let mut blob = SealedBox::seal(
        &recipient_kex,
        ONION_AD,
        &to_cbor(&inner_layer)?,
        INFO_ONION,
    );
    let mut target = recipient.id.clone();

    // Wrap one layer per relay, from the last relay outward to the first.
    for relay in relays.iter().rev() {
        let relay_kex = crypto::x25519_pub_from_slice(relay.kex_pub.as_slice())?;
        let layer = OnionLayer {
            next: Some(target.clone()),
            inner: Bytes::new(blob),
        };
        blob = SealedBox::seal(&relay_kex, ONION_AD, &to_cbor(&layer)?, INFO_ONION);
        target = relay.id.clone();
    }
    Ok((target, Bytes::new(blob)))
}

/// Peel one onion layer with this node's identity. Fails if the layer is not
/// sealed to us (or is tampered), so a relay off the path cannot learn anything.
pub fn peel_onion(me: &crate::Identity, onion: &[u8]) -> Result<Peeled> {
    let raw = SealedBox::open(me.kex_secret(), ONION_AD, onion, INFO_ONION)?;
    let layer: OnionLayer = from_cbor(&raw).map_err(|_| CoreError::Decrypt)?;
    match layer.next {
        Some(next) => Ok(Peeled::Forward {
            next,
            inner: layer.inner,
        }),
        None => Ok(Peeled::Deliver {
            payload: layer.inner.0,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Identity;

    #[test]
    fn multi_hop_onion_reveals_only_next_hop() {
        let alice = Identity::generate(0);
        let r1 = Identity::generate(0);
        let r2 = Identity::generate(0);
        let bob = Identity::generate(0);
        let _ = &alice; // sender is anonymous inside the onion by construction

        let relays = [r1.public(), r2.public()];
        let (first, onion) = build_onion(&relays, &bob.public(), b"rendezvous at dawn").unwrap();
        assert_eq!(first, *r1.address(), "first hop is R1");

        // R1 peels: learns only that the next hop is R2 (not Bob).
        let p1 = peel_onion(&r1, onion.as_slice()).unwrap();
        let inner1 = match p1 {
            Peeled::Forward { next, inner } => {
                assert_eq!(next, *r2.address());
                assert_ne!(next, *bob.address(), "R1 must not learn the recipient");
                inner
            }
            _ => panic!("R1 should forward"),
        };

        // R2 peels: learns the next hop is Bob.
        let p2 = peel_onion(&r2, inner1.as_slice()).unwrap();
        let inner2 = match p2 {
            Peeled::Forward { next, inner } => {
                assert_eq!(next, *bob.address());
                inner
            }
            _ => panic!("R2 should forward"),
        };

        // Bob peels the last layer: gets the message.
        match peel_onion(&bob, inner2.as_slice()).unwrap() {
            Peeled::Deliver { payload } => assert_eq!(payload, b"rendezvous at dawn"),
            _ => panic!("Bob should deliver"),
        }
    }

    #[test]
    fn wrong_relay_cannot_peel() {
        let r1 = Identity::generate(0);
        let bob = Identity::generate(0);
        let eve = Identity::generate(0);
        let (_first, onion) = build_onion(&[r1.public()], &bob.public(), b"secret").unwrap();
        // The outer layer is sealed to R1; Eve (off-path) cannot peel it.
        assert!(peel_onion(&eve, onion.as_slice()).is_err());
    }

    #[test]
    fn tampered_onion_rejected() {
        let r1 = Identity::generate(0);
        let bob = Identity::generate(0);
        let (_first, onion) = build_onion(&[r1.public()], &bob.public(), b"secret").unwrap();
        let mut bad = onion.0.clone();
        let last = bad.len() - 1;
        bad[last] ^= 0x01;
        assert!(peel_onion(&r1, &bad).is_err());
    }

    #[test]
    fn empty_path_is_direct_to_recipient() {
        let bob = Identity::generate(0);
        let (first, onion) = build_onion(&[], &bob.public(), b"hi").unwrap();
        assert_eq!(first, *bob.address());
        match peel_onion(&bob, onion.as_slice()).unwrap() {
            Peeled::Deliver { payload } => assert_eq!(payload, b"hi"),
            _ => panic!("direct delivery"),
        }
    }
}
