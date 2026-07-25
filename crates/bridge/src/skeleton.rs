//! **Template for a new external network.** Copy this module, rename it (e.g.
//! `reticulum`, `meshtastic`, `matrix`, `relay`), and fill in the four methods.
//! That is the *entire* surface needed to give Lifeline connectivity over any new
//! message-passing network — the engine, routing, and crypto are untouched.
//!
//! The contract (see [`ExternalNet`]):
//! * map the network's native address (a Reticulum destination hash, a Meshtastic
//!   node id, a Matrix user id, a relay-assigned id) to a stable [`PeerId`] with
//!   [`peer_id_from_identity`];
//! * `publish` an opaque frame to one peer or broadcast it;
//! * `receive` frames the network delivered, tagged with their source peer;
//! * list currently-reachable `peers`.
//!
//! A real adapter opens sockets/serial/WebSocket in a background task; for an
//! async network, feed a [`lifeline_transport::ChannelInterface`] from that task
//! instead of implementing `ExternalNet` directly. Either way the engine sees a
//! normal interface.

use lifeline_transport::bridge::peer_id_from_identity;
use lifeline_transport::{ExternalNet, InterfaceCaps, PeerId};
use std::collections::VecDeque;

/// A minimal, compiling template adapter. Replace the body of each method with
/// calls into your network's client. As written it is an inert loopback so the
/// crate compiles and the shape is clear.
pub struct TemplateNet {
    caps: InterfaceCaps,
    /// Whatever handle your network client needs (socket, channel, session…).
    inbound: VecDeque<(PeerId, Vec<u8>)>,
}

impl TemplateNet {
    /// `name` identifies the overlay; `mtu` is how large a frame this network can
    /// carry in one message (drives Lifeline's fragmentation).
    pub fn new(name: &str, mtu: usize) -> Self {
        TemplateNet {
            caps: InterfaceCaps::overlay(name, mtu),
            inbound: VecDeque::new(),
        }
    }

    /// Example: how a real adapter would turn a remote identity into a peer id.
    pub fn peer_of(remote_identity: &[u8]) -> PeerId {
        peer_id_from_identity(remote_identity)
    }
}

impl ExternalNet for TemplateNet {
    fn caps(&self) -> &InterfaceCaps {
        &self.caps
    }

    fn publish(&mut self, _to: Option<PeerId>, frame: &[u8]) -> lifeline_transport::Result<()> {
        // TODO: hand `frame` to your network client — send to `to`, or broadcast
        // if `None`. Here we just loop it back so the template is self-contained.
        self.inbound.push_back((0, frame.to_vec()));
        Ok(())
    }

    fn receive(&mut self) -> Vec<(PeerId, Vec<u8>)> {
        // TODO: drain frames your network client has received, mapping each
        // sender's native address to a PeerId via `peer_id_from_identity`.
        self.inbound.drain(..).collect()
    }

    fn peers(&mut self) -> Vec<PeerId> {
        // TODO: return the peers currently reachable on your network.
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lifeline_transport::{BridgeInterface, Interface};

    #[test]
    fn template_compiles_and_bridges() {
        let mut iface = BridgeInterface::new(TemplateNet::new("template", 1024));
        iface.broadcast(b"frame").unwrap();
        assert_eq!(iface.poll(), vec![(0u64, b"frame".to_vec())]);
    }
}
