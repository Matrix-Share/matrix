//! An in-process broadcast medium + a memory-backed [`Interface`].
//!
//! [`SharedMedium`] stands in for a physical channel (a room full of phones on
//! BLE, an ultrasound-audible space, an internet relay) so the entire transport
//! stack is testable without hardware. A real BLE/LoRa/internet backend
//! replaces [`MemoryInterface`] while implementing the same [`Interface`] trait
//! and publishing the same [`InterfaceCaps`].

use crate::caps::InterfaceCaps;
use crate::interface::{Interface, PeerId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Shared state of one physical channel: who's attached and each attachment's
/// inbox. All attachments on a medium are mutually reachable (same "cell").
#[derive(Default)]
struct MediumInner {
    next_id: PeerId,
    attached: Vec<PeerId>,
    inbox: HashMap<PeerId, Vec<(PeerId, Vec<u8>)>>,
}

/// A cloneable handle to a physical channel.
#[derive(Clone, Default)]
pub struct SharedMedium(Rc<RefCell<MediumInner>>);

impl SharedMedium {
    pub fn new() -> Self {
        SharedMedium(Rc::new(RefCell::new(MediumInner::default())))
    }

    /// Attach a new interface with the given caps; returns a driver bound to
    /// this medium.
    pub fn attach(&self, caps: InterfaceCaps) -> MemoryInterface {
        let mut inner = self.0.borrow_mut();
        let id = inner.next_id;
        inner.next_id += 1;
        inner.attached.push(id);
        inner.inbox.insert(id, Vec::new());
        MemoryInterface {
            id,
            caps,
            medium: self.clone(),
        }
    }
}

/// An [`Interface`] backed by a [`SharedMedium`].
pub struct MemoryInterface {
    id: PeerId,
    caps: InterfaceCaps,
    medium: SharedMedium,
}

impl Interface for MemoryInterface {
    fn caps(&self) -> &InterfaceCaps {
        &self.caps
    }

    fn scan(&mut self) -> Vec<PeerId> {
        self.medium
            .0
            .borrow()
            .attached
            .iter()
            .copied()
            .filter(|&p| p != self.id)
            .collect()
    }

    fn send(&mut self, peer: PeerId, frame: &[u8]) -> crate::Result<()> {
        // Enforce the MTU the caps advertise — the engine depends on this to
        // know its fragmentation was correct.
        if frame.len() > self.caps.mtu {
            return Err(crate::TransportError::Mtu {
                frame: frame.len(),
                mtu: self.caps.mtu,
            });
        }
        let mut inner = self.medium.0.borrow_mut();
        if let Some(box_) = inner.inbox.get_mut(&peer) {
            box_.push((self.id, frame.to_vec()));
        }
        Ok(())
    }

    fn poll(&mut self) -> Vec<(PeerId, Vec<u8>)> {
        let mut inner = self.medium.0.borrow_mut();
        inner
            .inbox
            .get_mut(&self.id)
            .map(std::mem::take)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_interfaces_exchange_frames() {
        let med = SharedMedium::new();
        let mut a = med.attach(InterfaceCaps::ble());
        let mut b = med.attach(InterfaceCaps::ble());
        let peers = a.scan();
        assert_eq!(peers.len(), 1);
        a.send(peers[0], b"hello").unwrap();
        let got = b.poll();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, b"hello");
    }

    #[test]
    fn send_rejects_oversize_frame() {
        let med = SharedMedium::new();
        let mut a = med.attach(InterfaceCaps::ultrasound());
        let _b = med.attach(InterfaceCaps::ultrasound());
        let peers = a.scan();
        let big = vec![0u8; 1000]; // > ultrasound MTU 128
        assert!(a.send(peers[0], &big).is_err());
    }
}
