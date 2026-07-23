//! Sender-keys group encryption (PRD FR-12; Matrix Megolm / Signal sender-keys
//! model).
//!
//! Encrypting a group message once per *sender* (not once per recipient) is what
//! makes group chat scale. Each member owns a **sender key** — a symmetric chain
//! key that ratchets forward one step per message. The member distributes that
//! sender key to the group once (sealed to each member's X25519 key); thereafter
//! it encrypts each message with a fresh per-message key derived from its chain,
//! and every member who holds the sender key can decrypt.
//!
//! Properties:
//! * **O(1) encryption per message** regardless of group size.
//! * **Forward secrecy along the chain** — the KDF is one-way, so a member who
//!   joins at iteration `i` (receiving the chain key at `i`) cannot read earlier
//!   messages.
//! * **Authenticated** — every message is Ed25519-signed by its sender.
//! * **Reordering-tolerant** — skipped message keys are cached (bounded), which
//!   matters in a delay-tolerant mesh (out-of-order delivery is normal).
//!
//! This module is the group *cryptography*. Fanning a group message out to
//! members as bundles (and membership itself) live in `sync::SharedState`
//! (already converging) + the engine.

use crate::crypto::{self, SealedBox, SecureChannel, KEY_LEN, NONCE_LEN};
use crate::identity::{verify_sig, Identity};
use crate::{CoreError, Result};
use lifeline_proto::codec::{from_cbor, to_cbor};
use lifeline_proto::{Address, Bytes};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use x25519_dalek::PublicKey as XPublic;

/// Domain separators for the symmetric ratchet.
const INFO_MSG: &[u8] = b"lifeline/v1/group-msg";
const INFO_CHAIN: &[u8] = b"lifeline/v1/group-chain";
const INFO_DIST: &[u8] = b"lifeline/v1/group-senderkey";

/// Max messages we will skip-and-cache to tolerate reordering (bounds memory).
pub const MAX_SKIP: u32 = 2000;

/// Advance a chain key one step, yielding this step's message key and the next
/// chain key.
fn ratchet(chain_key: &[u8; KEY_LEN]) -> ([u8; KEY_LEN], [u8; KEY_LEN]) {
    let mk = crypto::hkdf_sha256(chain_key, &[], INFO_MSG, KEY_LEN);
    let nc = crypto::hkdf_sha256(chain_key, &[], INFO_CHAIN, KEY_LEN);
    let mut msg_key = [0u8; KEY_LEN];
    let mut next = [0u8; KEY_LEN];
    msg_key.copy_from_slice(&mk);
    next.copy_from_slice(&nc);
    (msg_key, next)
}

fn ad_bytes(group_id: &str, owner: &Address, iteration: u32) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(group_id.as_bytes());
    v.extend_from_slice(owner.as_bytes());
    v.extend_from_slice(&iteration.to_be_bytes());
    v
}

fn signing_bytes(
    group_id: &str,
    owner: &Address,
    iteration: u32,
    nonce: &[u8],
    ct: &[u8],
) -> Vec<u8> {
    let mut v = ad_bytes(group_id, owner, iteration);
    v.extend_from_slice(nonce);
    v.extend_from_slice(&crypto::blake3_32(ct));
    v
}

/// A group message on the wire (PRD FR-12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupMessage {
    pub group_id: String,
    pub owner: Address,
    pub iteration: u32,
    pub nonce: Bytes,
    pub ct: Bytes,
    pub sig: Bytes,
}

/// The private sending state a member keeps for a group.
pub struct SenderKeyState {
    group_id: String,
    chain_key: [u8; KEY_LEN],
    iteration: u32,
}

impl SenderKeyState {
    /// Create a fresh sender key for `group_id`.
    pub fn new(group_id: impl Into<String>) -> Self {
        let mut chain_key = [0u8; KEY_LEN];
        chain_key.copy_from_slice(&crypto::random_bytes(KEY_LEN));
        SenderKeyState {
            group_id: group_id.into(),
            chain_key,
            iteration: 0,
        }
    }

    fn step(&mut self) -> (u32, [u8; KEY_LEN]) {
        let it = self.iteration;
        let (mk, nc) = ratchet(&self.chain_key);
        self.chain_key = nc;
        self.iteration += 1;
        (it, mk)
    }

    /// Encrypt `plaintext` as the next group message from `owner`.
    pub fn encrypt(&mut self, owner: &Identity, plaintext: &[u8]) -> GroupMessage {
        let (iteration, mk) = self.step();
        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(&crypto::random_bytes(NONCE_LEN));
        let ad = ad_bytes(&self.group_id, owner.address(), iteration);
        let ct = crypto::aead_seal(&mk, &nonce, &ad, plaintext);
        let sig = owner.sign(&signing_bytes(
            &self.group_id,
            owner.address(),
            iteration,
            &nonce,
            &ct,
        ));
        GroupMessage {
            group_id: self.group_id.clone(),
            owner: owner.address().clone(),
            iteration,
            nonce: Bytes::new(nonce.to_vec()),
            ct: Bytes::new(ct),
            sig,
        }
    }

    /// Build a distribution message so another member can decrypt from here on.
    pub fn distribution(&self, owner: &Identity) -> SenderKeyDistribution {
        SenderKeyDistribution {
            group_id: self.group_id.clone(),
            owner: owner.address().clone(),
            sign_pub: Bytes::new(owner.verifying_key().as_bytes().to_vec()),
            chain_key: Bytes::new(self.chain_key.to_vec()),
            iteration: self.iteration,
        }
    }
}

/// A sender key handed to a group member (sealed to them before transit).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderKeyDistribution {
    pub group_id: String,
    pub owner: Address,
    pub sign_pub: Bytes,
    pub chain_key: Bytes,
    pub iteration: u32,
}

impl SenderKeyDistribution {
    /// Seal this distribution to a member's X25519 key for transit.
    pub fn seal_to(&self, member_kex_pub: &[u8]) -> Result<Vec<u8>> {
        let pk = crypto::x25519_pub_from_slice(member_kex_pub)?;
        let bytes = to_cbor(self)?;
        Ok(SealedBox::seal(
            &pk,
            self.group_id.as_bytes(),
            &bytes,
            INFO_DIST,
        ))
    }
}

/// The private receiving state a member keeps for one *sender* in a group.
pub struct ReceiverKeyState {
    group_id: String,
    owner: Address,
    sign_pub: [u8; 32],
    chain_key: [u8; KEY_LEN],
    iteration: u32,
    /// Cached keys for messages that arrived out of order (bounded).
    skipped: BTreeMap<u32, [u8; KEY_LEN]>,
}

impl ReceiverKeyState {
    /// Build a receiver state from a distribution sealed to us.
    pub fn open_sealed(recipient: &Identity, group_id: &str, sealed: &[u8]) -> Result<Self> {
        let bytes = SealedBox::open(
            recipient.kex_secret(),
            group_id.as_bytes(),
            sealed,
            INFO_DIST,
        )?;
        let dist: SenderKeyDistribution = from_cbor(&bytes)?;
        Self::from_distribution(&dist)
    }

    /// Build a receiver state directly from a distribution.
    pub fn from_distribution(dist: &SenderKeyDistribution) -> Result<Self> {
        let sign_pub: [u8; 32] = dist
            .sign_pub
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::BadKey("group sign_pub len".into()))?;
        let chain_key: [u8; KEY_LEN] = dist
            .chain_key
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::BadKey("group chain_key len".into()))?;
        Ok(ReceiverKeyState {
            group_id: dist.group_id.clone(),
            owner: dist.owner.clone(),
            sign_pub,
            chain_key,
            iteration: dist.iteration,
            skipped: BTreeMap::new(),
        })
    }

    fn step(&mut self) -> (u32, [u8; KEY_LEN]) {
        let it = self.iteration;
        let (mk, nc) = ratchet(&self.chain_key);
        self.chain_key = nc;
        self.iteration += 1;
        (it, mk)
    }

    /// Decrypt a group message from this sender, tolerating reordering.
    pub fn decrypt(&mut self, msg: &GroupMessage) -> Result<Vec<u8>> {
        if msg.group_id != self.group_id || msg.owner != self.owner {
            return Err(CoreError::Decrypt);
        }
        // Authenticate the sender first.
        verify_sig(
            &self.sign_pub,
            &signing_bytes(
                &msg.group_id,
                &msg.owner,
                msg.iteration,
                msg.nonce.as_slice(),
                msg.ct.as_slice(),
            ),
            msg.sig.as_slice(),
        )?;

        let mk = if msg.iteration < self.iteration {
            // Older than our chain position — must be a cached skipped key.
            self.skipped
                .remove(&msg.iteration)
                .ok_or(CoreError::Decrypt)?
        } else {
            if msg.iteration - self.iteration > MAX_SKIP {
                return Err(CoreError::Decrypt);
            }
            // Ratchet forward, caching keys for messages we skipped over.
            while self.iteration < msg.iteration {
                let (it, mk) = self.step();
                self.skipped.insert(it, mk);
            }
            let (_it, mk) = self.step();
            mk
        };

        let nonce: [u8; NONCE_LEN] = msg
            .nonce
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::Decrypt)?;
        let ad = ad_bytes(&msg.group_id, &msg.owner, msg.iteration);
        crypto::aead_open(&mk, &nonce, &ad, msg.ct.as_slice())
    }
}

/// Convenience: seal a distribution built from `state` to `member_kex_pub`.
pub fn seal_sender_key(
    owner: &Identity,
    state: &SenderKeyState,
    member_kex_pub: &XPublic,
) -> Result<Vec<u8>> {
    state.distribution(owner).seal_to(member_kex_pub.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kex_pub(id: &Identity) -> Vec<u8> {
        id.kex_public().as_bytes().to_vec()
    }

    #[test]
    fn three_members_exchange_group_messages() {
        let alice = Identity::generate(0);
        let bob = Identity::generate(0);
        let carol = Identity::generate(0);

        // Alice creates a sender key and distributes it to Bob and Carol.
        let mut alice_sk = SenderKeyState::new("relief-team");
        let dist = alice_sk.distribution(&alice);
        let mut bob_rx = ReceiverKeyState::from_distribution(&dist).unwrap();
        let mut carol_rx = ReceiverKeyState::from_distribution(&dist).unwrap();

        for i in 0..5u32 {
            let text = format!("update {i}");
            let msg = alice_sk.encrypt(&alice, text.as_bytes());
            assert_eq!(bob_rx.decrypt(&msg).unwrap(), text.as_bytes());
            assert_eq!(carol_rx.decrypt(&msg).unwrap(), text.as_bytes());
        }
        // Silence unused-var warnings for keys used only via addresses.
        let _ = (kex_pub(&bob), kex_pub(&carol));
    }

    #[test]
    fn out_of_order_messages_decrypt() {
        let alice = Identity::generate(0);
        let mut sk = SenderKeyState::new("g");
        let dist = sk.distribution(&alice);
        let mut rx = ReceiverKeyState::from_distribution(&dist).unwrap();

        let m0 = sk.encrypt(&alice, b"zero");
        let m1 = sk.encrypt(&alice, b"one");
        let m2 = sk.encrypt(&alice, b"two");

        // Deliver 2, then 0, then 1 — all must decrypt via skipped-key cache.
        assert_eq!(rx.decrypt(&m2).unwrap(), b"two");
        assert_eq!(rx.decrypt(&m0).unwrap(), b"zero");
        assert_eq!(rx.decrypt(&m1).unwrap(), b"one");
    }

    #[test]
    fn sealed_distribution_roundtrip() {
        let alice = Identity::generate(0);
        let bob = Identity::generate(0);
        let mut sk = SenderKeyState::new("g");
        let sealed = seal_sender_key(&alice, &sk, &bob.kex_public()).unwrap();
        let mut bob_rx = ReceiverKeyState::open_sealed(&bob, "g", &sealed).unwrap();
        let msg = sk.encrypt(&alice, b"hello group");
        assert_eq!(bob_rx.decrypt(&msg).unwrap(), b"hello group");
    }

    #[test]
    fn member_joining_late_cannot_read_history() {
        let alice = Identity::generate(0);
        let mut sk = SenderKeyState::new("g");

        let m0 = sk.encrypt(&alice, b"secret history");
        // Dave joins now — gets the chain key at its *current* position.
        let dist = sk.distribution(&alice);
        let mut dave_rx = ReceiverKeyState::from_distribution(&dist).unwrap();
        let m1 = sk.encrypt(&alice, b"after dave joined");

        // Dave can read the post-join message but not the pre-join one.
        assert_eq!(dave_rx.decrypt(&m1).unwrap(), b"after dave joined");
        assert!(
            dave_rx.decrypt(&m0).is_err(),
            "must not read pre-join history"
        );
    }

    #[test]
    fn tampered_or_forged_message_rejected() {
        let alice = Identity::generate(0);
        let eve = Identity::generate(0);
        let mut sk = SenderKeyState::new("g");
        let dist = sk.distribution(&alice);
        let mut rx = ReceiverKeyState::from_distribution(&dist).unwrap();

        let mut msg = sk.encrypt(&alice, b"authentic");
        // Flip a ciphertext byte.
        msg.ct.0[0] ^= 0x01;
        assert!(rx.decrypt(&msg).is_err());

        // Forge the signature with Eve's key over the same fields.
        let mut msg2 = sk.encrypt(&alice, b"authentic2");
        msg2.sig = eve.sign(&signing_bytes(
            &msg2.group_id,
            &msg2.owner,
            msg2.iteration,
            msg2.nonce.as_slice(),
            msg2.ct.as_slice(),
        ));
        assert!(rx.decrypt(&msg2).is_err(), "forged signature must fail");
    }
}
