//! A channel-backed [`Interface`] for wiring the engine to a real network.
//!
//! The in-process [`crate::MemoryInterface`] is great for tests but lives in one
//! thread. A production node runs the engine on its own thread and talks to a
//! network task (a TCP/WebSocket relay client) over plain channels. This
//! interface is that bridge: the engine `send`s frames into an outbound queue
//! the network task drains, and `poll`s frames the network task pushed inbound.
//!
//! It is `Send` (unlike `MemoryInterface`, which uses `Rc`), so the engine that
//! holds it can run on a dedicated OS thread while the async network runtime
//! runs elsewhere.

use crate::caps::InterfaceCaps;
use crate::interface::{Interface, PeerId};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

/// A frame the engine wants to transmit. `to == None` means broadcast to all
/// peers currently in range (the network task fans it out).
#[derive(Debug, Clone)]
pub struct Outbound {
    pub to: Option<PeerId>,
    pub frame: Vec<u8>,
}

/// The engine-facing half of a network link.
pub struct ChannelInterface {
    caps: InterfaceCaps,
    /// Frames leaving the node (drained by the network task).
    outbound: Sender<Outbound>,
    /// Frames arriving from the network (filled by the network task).
    inbound: Receiver<(PeerId, Vec<u8>)>,
    /// Peers the network task currently considers reachable.
    peers: Arc<Mutex<Vec<PeerId>>>,
}

impl ChannelInterface {
    pub fn new(
        caps: InterfaceCaps,
        outbound: Sender<Outbound>,
        inbound: Receiver<(PeerId, Vec<u8>)>,
        peers: Arc<Mutex<Vec<PeerId>>>,
    ) -> Self {
        ChannelInterface {
            caps,
            outbound,
            inbound,
            peers,
        }
    }
}

impl Interface for ChannelInterface {
    fn caps(&self) -> &InterfaceCaps {
        &self.caps
    }

    fn scan(&mut self) -> Vec<PeerId> {
        self.peers.lock().map(|p| p.clone()).unwrap_or_default()
    }

    fn send(&mut self, peer: PeerId, frame: &[u8]) -> crate::Result<()> {
        if frame.len() > self.caps.mtu {
            return Err(crate::TransportError::Mtu {
                frame: frame.len(),
                mtu: self.caps.mtu,
            });
        }
        // A closed channel just means the network task is gone; drop silently.
        let _ = self.outbound.send(Outbound {
            to: Some(peer),
            frame: frame.to_vec(),
        });
        Ok(())
    }

    fn broadcast(&mut self, frame: &[u8]) -> crate::Result<()> {
        if frame.len() > self.caps.mtu {
            return Err(crate::TransportError::Mtu {
                frame: frame.len(),
                mtu: self.caps.mtu,
            });
        }
        let _ = self.outbound.send(Outbound {
            to: None,
            frame: frame.to_vec(),
        });
        Ok(())
    }

    fn poll(&mut self) -> Vec<(PeerId, Vec<u8>)> {
        let mut out = Vec::new();
        while let Ok(item) = self.inbound.try_recv() {
            out.push(item);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn send_and_poll_via_channels() {
        let (out_tx, out_rx) = channel::<Outbound>();
        let (in_tx, in_rx) = channel::<(PeerId, Vec<u8>)>();
        let peers = Arc::new(Mutex::new(vec![1u64, 2]));
        let mut iface = ChannelInterface::new(InterfaceCaps::internet(), out_tx, in_rx, peers);

        assert_eq!(iface.scan(), vec![1, 2]);
        iface.broadcast(b"beacon").unwrap();
        assert!(matches!(out_rx.recv().unwrap(), Outbound { to: None, .. }));

        in_tx.send((2, b"hello".to_vec())).unwrap();
        let got = iface.poll();
        assert_eq!(got, vec![(2u64, b"hello".to_vec())]);
    }
}
