//! The **external-network seam** — how Lifeline plugs into *any* message-passing
//! network (Nostr relays, a Reticulum transport node, Meshtastic, a plain relay)
//! and gains connectivity without the engine knowing anything about it.
//!
//! Implement [`ExternalNet`] for a network and wrap it in [`BridgeInterface`]:
//! it becomes a first-class [`Interface`] the [`crate::NodeEngine`] drives exactly
//! like a radio. The adapter only shuttles **opaque frame bytes** to and from the
//! network and reports reachable peers — it never inspects a Lifeline bundle, so
//! end-to-end encryption and delivery proof are untouched.
//!
//! This is the extension point for "more connectivity": each new network is one
//! `ExternalNet` implementation, nothing else changes.

use crate::caps::InterfaceCaps;
use crate::interface::{Interface, PeerId};

/// An adapter to one external network. The generic [`BridgeInterface`] turns any
/// implementor into an engine [`Interface`].
///
/// Contract:
/// * `publish(None, frame)` broadcasts to everyone reachable (used for beacons /
///   discovery); `publish(Some(peer), frame)` sends to one peer.
/// * `receive` drains frames the network delivered since the last call, each with
///   the [`PeerId`] of its source (stable per remote identity, so the engine can
///   later address that peer).
/// * `peers` lists currently-reachable peers (those we can `publish(Some(..))` to).
///
/// Implementors map the network's native addressing (a Nostr pubkey, a Reticulum
/// destination hash, a relay-assigned id) to a stable [`PeerId`] — typically a
/// hash of the remote's public identity.
///
/// (No `Send` bound — like [`Interface`], so an in-process/single-thread adapter
/// is fine. A real async adapter that runs on its own thread simply feeds a
/// [`crate::ChannelInterface`], which is `Send`.)
pub trait ExternalNet {
    /// This network's self-description (kind/MTU/range for fragmentation + UI).
    fn caps(&self) -> &InterfaceCaps;
    /// Send `frame` to `to` (a specific peer) or broadcast (`to == None`).
    fn publish(&mut self, to: Option<PeerId>, frame: &[u8]) -> crate::Result<()>;
    /// Drain frames received since the last call, with their source peer.
    fn receive(&mut self) -> Vec<(PeerId, Vec<u8>)>;
    /// Peers currently reachable on this network.
    fn peers(&mut self) -> Vec<PeerId>;
}

/// Adapts any [`ExternalNet`] into an [`Interface`] the engine can drive. Adding
/// a new kind of connectivity is: implement `ExternalNet`, hand it to
/// `BridgeInterface::new`, and `engine.add_interface(Box::new(it))`.
pub struct BridgeInterface<N: ExternalNet> {
    net: N,
}

impl<N: ExternalNet> BridgeInterface<N> {
    pub fn new(net: N) -> Self {
        BridgeInterface { net }
    }

    /// Borrow the underlying adapter (diagnostics / driving its own I/O).
    pub fn net_mut(&mut self) -> &mut N {
        &mut self.net
    }
}

impl<N: ExternalNet> Interface for BridgeInterface<N> {
    fn caps(&self) -> &InterfaceCaps {
        self.net.caps()
    }

    fn scan(&mut self) -> Vec<PeerId> {
        self.net.peers()
    }

    fn send(&mut self, peer: PeerId, frame: &[u8]) -> crate::Result<()> {
        // Honour the advertised MTU so the engine's fragmentation stays correct.
        if frame.len() > self.net.caps().mtu {
            return Err(crate::TransportError::Mtu {
                frame: frame.len(),
                mtu: self.net.caps().mtu,
            });
        }
        self.net.publish(Some(peer), frame)
    }

    fn broadcast(&mut self, frame: &[u8]) -> crate::Result<()> {
        if frame.len() > self.net.caps().mtu {
            return Err(crate::TransportError::Mtu {
                frame: frame.len(),
                mtu: self.net.caps().mtu,
            });
        }
        self.net.publish(None, frame)
    }

    fn poll(&mut self) -> Vec<(PeerId, Vec<u8>)> {
        self.net.receive()
    }
}

/// Derive a stable [`PeerId`] from a remote's public identity bytes (e.g. a Nostr
/// x-only pubkey or a Reticulum destination hash). Adapters use this so the same
/// remote always maps to the same peer id across sessions.
pub fn peer_id_from_identity(identity: &[u8]) -> PeerId {
    // FNV-1a over the identity bytes — stable, fast, dependency-free. Mapping a
    // 32-byte pubkey to a u64 peer id; collisions are negligible at mesh scale.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in identity {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::InterfaceCaps;
    use std::collections::VecDeque;

    /// A trivial loopback `ExternalNet` — proves the seam: anything you publish
    /// comes back as if from peer 1. Demonstrates that implementing one trait is
    /// all a new network needs.
    struct Loopback {
        caps: InterfaceCaps,
        inbox: VecDeque<(PeerId, Vec<u8>)>,
    }
    impl ExternalNet for Loopback {
        fn caps(&self) -> &InterfaceCaps {
            &self.caps
        }
        fn publish(&mut self, _to: Option<PeerId>, frame: &[u8]) -> crate::Result<()> {
            self.inbox.push_back((1, frame.to_vec()));
            Ok(())
        }
        fn receive(&mut self) -> Vec<(PeerId, Vec<u8>)> {
            self.inbox.drain(..).collect()
        }
        fn peers(&mut self) -> Vec<PeerId> {
            vec![1]
        }
    }

    #[test]
    fn bridge_exposes_external_net_as_interface() {
        let net = Loopback {
            caps: InterfaceCaps::overlay("loop", 1024),
            inbox: VecDeque::new(),
        };
        let mut iface = BridgeInterface::new(net);
        assert_eq!(iface.scan(), vec![1]);
        iface.broadcast(b"hi").unwrap();
        assert_eq!(iface.poll(), vec![(1u64, b"hi".to_vec())]);
        // MTU is enforced through the bridge.
        assert!(iface.send(1, &vec![0u8; 2048]).is_err());
    }

    #[test]
    fn peer_id_is_stable_and_distinct() {
        assert_eq!(
            peer_id_from_identity(b"alice"),
            peer_id_from_identity(b"alice")
        );
        assert_ne!(
            peer_id_from_identity(b"alice"),
            peer_id_from_identity(b"bob")
        );
    }
}
