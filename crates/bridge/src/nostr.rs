//! A **Nostr** adapter for Lifeline (NIP-01 events over relays).
//!
//! Lifeline bundles are already opaque end-to-end ciphertext, so this adapter
//! doesn't add Nostr-level encryption for confidentiality — it carries each
//! frame as the content of a **real, signed Nostr event** (secp256k1 Schnorr,
//! `id = sha256(canonical array)`, per NIP-01) and lets Nostr relays store and
//! forward it. That gives Lifeline **global internet reach + offline mailboxing
//! over the already-adopted Nostr relay network**, with no `lifeline-relay` to
//! run and no engine change (it plugs into [`ExternalNet`]).
//!
//! Addressing:
//! * **Broadcast** frames (beacons/discovery) are published to a shared channel
//!   tag `["L","lifeline-mesh"]` that every Lifeline node subscribes to — this is
//!   how nodes discover each other and learn the `Lifeline-address ↔ Nostr-pubkey`
//!   mapping (the mapping is just the event's `pubkey`).
//! * **Directed** frames are `["p", <recipient nostr pubkey>]`-tagged so relays
//!   route them to that recipient's inbox.
//!
//! [`MockRelay`] stands in for a real relay so the whole path is testable without
//! network; a real WebSocket relay client is a thin drop-in over the same codec.

use crate::{hex, unhex};
use lifeline_proto::codec::{b64url_decode, b64url_encode};
use lifeline_transport::bridge::peer_id_from_identity;
use lifeline_transport::{ExternalNet, InterfaceCaps, PeerId};
use secp256k1::schnorr::Signature;
use secp256k1::{Keypair, Message, Secp256k1, SecretKey, XOnlyPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// Lifeline frames ride in this event kind (regular range 1000–9999 → relays
/// store it, so an offline recipient can fetch it on reconnect).
pub const LIFELINE_KIND: u32 = 1998;
const CHANNEL: &str = "lifeline-mesh";
/// MTU: a frame becomes one event's base64 content; 16 KiB keeps events a size
/// every relay accepts. Larger bundles fragment into several events.
const NOSTR_MTU: usize = 16 * 1024;

/// A Nostr identity (secp256k1 keypair) — the account is the x-only pubkey.
pub struct NostrIdentity {
    keypair: Keypair,
    pubkey_hex: String,
    pubkey_bytes: [u8; 32],
    secp: Secp256k1<secp256k1::All>,
}

impl NostrIdentity {
    /// Deterministic identity from a 32-byte seed (e.g. `HMAC(device_seed, geohash)`
    /// for BitChat-style per-channel pseudonyms, or a stored node seed).
    pub fn from_seed(seed: &[u8; 32]) -> Option<Self> {
        let secp = Secp256k1::new();
        let sk = SecretKey::from_slice(seed).ok()?;
        let keypair = Keypair::from_secret_key(&secp, &sk);
        let (xonly, _) = keypair.x_only_public_key();
        let pubkey_bytes = xonly.serialize();
        Some(NostrIdentity {
            keypair,
            pubkey_hex: hex(&pubkey_bytes),
            pubkey_bytes,
            secp,
        })
    }

    /// A fresh random identity (OS CSPRNG).
    pub fn generate() -> Self {
        let secp = Secp256k1::new();
        let (sk, _) = secp.generate_keypair(&mut rand::rngs::OsRng);
        Self::from_seed(&sk.secret_bytes()).expect("valid random key")
    }

    pub fn pubkey_hex(&self) -> &str {
        &self.pubkey_hex
    }
    pub fn pubkey_bytes(&self) -> [u8; 32] {
        self.pubkey_bytes
    }

    fn sign(&self, digest: &[u8; 32]) -> String {
        let msg = Message::from_digest(*digest);
        hex(&self
            .secp
            .sign_schnorr_no_aux_rand(&msg, &self.keypair)
            .serialize())
    }
}

/// A NIP-01 event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NostrEvent {
    pub id: String,
    pub pubkey: String,
    pub created_at: u64,
    pub kind: u32,
    pub tags: Vec<Vec<String>>,
    pub content: String,
    pub sig: String,
}

/// The NIP-01 canonical id: `sha256` of the compact JSON array
/// `[0, pubkey, created_at, kind, tags, content]`.
fn event_id(
    pubkey: &str,
    created_at: u64,
    kind: u32,
    tags: &[Vec<String>],
    content: &str,
) -> [u8; 32] {
    let arr = serde_json::json!([0, pubkey, created_at, kind, tags, content]);
    let ser = serde_json::to_string(&arr).expect("json");
    let mut h = Sha256::new();
    h.update(ser.as_bytes());
    h.finalize().into()
}

impl NostrEvent {
    /// Build and sign an event authored by `id`.
    pub fn build(
        id: &NostrIdentity,
        kind: u32,
        tags: Vec<Vec<String>>,
        content: String,
        created_at: u64,
    ) -> Self {
        let pubkey = id.pubkey_hex().to_string();
        let idb = event_id(&pubkey, created_at, kind, &tags, &content);
        NostrEvent {
            id: hex(&idb),
            sig: id.sign(&idb),
            pubkey,
            created_at,
            kind,
            tags,
            content,
        }
    }

    /// Verify the id matches the content and the Schnorr signature is valid —
    /// exactly what a Nostr client does before trusting any relay-supplied event.
    pub fn verify(&self, secp: &Secp256k1<secp256k1::All>) -> bool {
        let idb = event_id(
            &self.pubkey,
            self.created_at,
            self.kind,
            &self.tags,
            &self.content,
        );
        if hex(&idb) != self.id {
            return false;
        }
        let (Some(pk), Some(sig_bytes)) = (unhex(&self.pubkey), unhex(&self.sig)) else {
            return false;
        };
        let (Ok(xonly), Ok(sig)) = (
            XOnlyPublicKey::from_slice(&pk),
            Signature::from_slice(&sig_bytes),
        ) else {
            return false;
        };
        secp.verify_schnorr(&sig, &Message::from_digest(idb), &xonly)
            .is_ok()
    }

    /// First value of the first tag whose name is `name`.
    fn tag_value(&self, name: &str) -> Option<&str> {
        self.tags
            .iter()
            .find(|t| t.first().map(String::as_str) == Some(name))
            .and_then(|t| t.get(1))
            .map(String::as_str)
    }
}

/// An in-memory Nostr relay: stores events, answers subscription filters. Stands
/// in for a real relay in tests; a real relay is a WebSocket server with the same
/// store-and-forward semantics.
#[derive(Clone, Default)]
pub struct MockRelay(Rc<RefCell<Vec<NostrEvent>>>);

impl MockRelay {
    pub fn new() -> Self {
        MockRelay(Rc::new(RefCell::new(Vec::new())))
    }
    fn publish(&self, ev: NostrEvent) {
        self.0.borrow_mut().push(ev);
    }
    /// Events at index ≥ `from` of `LIFELINE_KIND` that carry any of the wanted
    /// `(tag_name, value)` filters. Returns them plus the new cursor.
    fn query(&self, from: usize, wants: &[(&str, String)]) -> (Vec<NostrEvent>, usize) {
        let inner = self.0.borrow();
        let out = inner
            .iter()
            .skip(from)
            .filter(|e| e.kind == LIFELINE_KIND)
            .filter(|e| {
                wants
                    .iter()
                    .any(|(t, v)| e.tag_value(t) == Some(v.as_str()))
            })
            .cloned()
            .collect();
        (out, inner.len())
    }
}

/// The Nostr [`ExternalNet`] adapter. Wrap in `BridgeInterface` and
/// `engine.add_interface(...)`.
pub struct NostrNet {
    id: NostrIdentity,
    relay: MockRelay,
    caps: InterfaceCaps,
    cursor: usize,
    seen: HashSet<String>,
    peers: HashMap<PeerId, String>,
    clock: u64,
}

impl NostrNet {
    pub fn new(id: NostrIdentity, relay: MockRelay) -> Self {
        NostrNet {
            id,
            relay,
            caps: InterfaceCaps::overlay("nostr", NOSTR_MTU),
            cursor: 0,
            seen: HashSet::new(),
            peers: HashMap::new(),
            clock: 0,
        }
    }
}

impl ExternalNet for NostrNet {
    fn caps(&self) -> &InterfaceCaps {
        &self.caps
    }

    fn publish(&mut self, to: Option<PeerId>, frame: &[u8]) -> lifeline_transport::Result<()> {
        self.clock += 1;
        let content = b64url_encode(frame);
        let tags = match to {
            None => vec![vec!["L".into(), CHANNEL.into()]],
            Some(peer) => {
                let Some(pk) = self.peers.get(&peer) else {
                    return Ok(()); // unknown peer's Nostr key not learned yet — drop
                };
                vec![
                    vec!["L".into(), "lifeline".into()],
                    vec!["p".into(), pk.clone()],
                ]
            }
        };
        let ev = NostrEvent::build(&self.id, LIFELINE_KIND, tags, content, self.clock);
        self.relay.publish(ev);
        Ok(())
    }

    fn receive(&mut self) -> Vec<(PeerId, Vec<u8>)> {
        // Subscribe to the shared discovery channel + anything p-tagged to us.
        let wants = [
            ("L", CHANNEL.to_string()),
            ("p", self.id.pubkey_hex().to_string()),
        ];
        let (events, cursor) = self.relay.query(self.cursor, &wants);
        self.cursor = cursor;
        let secp = Secp256k1::new();
        let mut out = Vec::new();
        for ev in events {
            if ev.pubkey == self.id.pubkey_hex() {
                continue; // our own event
            }
            if !self.seen.insert(ev.id.clone()) {
                continue; // dedup
            }
            if !ev.verify(&secp) {
                continue; // reject a forged/tampered event (as any Nostr client does)
            }
            let (Ok(frame), Some(pk)) = (b64url_decode(&ev.content), unhex(&ev.pubkey)) else {
                continue;
            };
            let peer = peer_id_from_identity(&pk);
            self.peers.insert(peer, ev.pubkey.clone());
            out.push((peer, frame));
        }
        out
    }

    fn peers(&mut self) -> Vec<PeerId> {
        self.peers.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_valid_signed_nostr_events() {
        let id = NostrIdentity::from_seed(&[7u8; 32]).unwrap();
        let ev = NostrEvent::build(
            &id,
            LIFELINE_KIND,
            vec![vec!["L".into(), CHANNEL.into()]],
            b64url_encode(b"hello mesh"),
            123,
        );
        let secp = Secp256k1::new();
        assert!(ev.verify(&secp), "a well-formed event must verify");
        // Tamper the content → id/sig no longer match.
        let mut bad = ev.clone();
        bad.content = b64url_encode(b"tampered");
        assert!(!bad.verify(&secp), "a tampered event must be rejected");
        // Distinct seeds → distinct pubkeys.
        let id2 = NostrIdentity::from_seed(&[8u8; 32]).unwrap();
        assert_ne!(id.pubkey_hex(), id2.pubkey_hex());
    }

    #[test]
    fn two_adapters_exchange_frames_via_a_relay() {
        let relay = MockRelay::new();
        let mut alice = NostrNet::new(NostrIdentity::from_seed(&[1u8; 32]).unwrap(), relay.clone());
        let mut bob = NostrNet::new(NostrIdentity::from_seed(&[2u8; 32]).unwrap(), relay.clone());

        // Alice broadcasts (discovery); Bob receives it and learns Alice as a peer.
        alice.publish(None, b"alice-beacon").unwrap();
        let got = bob.receive();
        assert_eq!(got.len(), 1);
        let alice_peer = got[0].0;
        assert_eq!(got[0].1, b"alice-beacon");

        // Bob now sends Alice a directed frame; the relay holds it until Alice polls.
        bob.publish(Some(alice_peer), b"private-reply").unwrap();
        let got = alice.receive();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, b"private-reply");
        // Alice did not receive her own beacon back.
        assert!(!alice.receive().iter().any(|(_, f)| f == b"alice-beacon"));
    }
}
