//! The [`Interface`] contract every transport implements (PRD §8.4).
//!
//! This is the seam that makes the whole system modular: BLE, Wi-Fi Aware,
//! ultrasound, optical, LoRa, and internet each become a `Box<dyn Interface>`
//! the [`crate::NodeEngine`] drives identically. A real backend implements the
//! same five methods over a radio or socket; the engine never changes.

use crate::caps::InterfaceCaps;

/// A transport-local handle for a peer currently in contact (meaning is
/// interface-specific: a BLE device id, a socket, a medium slot…).
pub type PeerId = u64;

/// The transport contract. Kept small and object-safe so any medium can plug in.
pub trait Interface {
    /// Static self-description (kind, MTU, range, region/power). Read by the
    /// engine for fragmentation and by compliance checks for lawful operation.
    fn caps(&self) -> &InterfaceCaps;

    /// Peers currently reachable on this interface (BLE scan, NAN discovery,
    /// connected sockets…). May change every tick as devices move.
    fn scan(&mut self) -> Vec<PeerId>;

    /// Send one already-encoded frame to `peer`. Implementations MUST reject a
    /// frame larger than [`InterfaceCaps::mtu`] — the engine relies on this to
    /// guarantee it fragmented correctly.
    fn send(&mut self, peer: PeerId, frame: &[u8]) -> crate::Result<()>;

    /// Broadcast a frame to every peer currently in range (default: send to
    /// each `scan()` result). Overridable for media with native broadcast.
    fn broadcast(&mut self, frame: &[u8]) -> crate::Result<()> {
        for peer in self.scan() {
            self.send(peer, frame)?;
        }
        Ok(())
    }

    /// Drain inbound frames received since the last poll, with their source peer.
    fn poll(&mut self) -> Vec<(PeerId, Vec<u8>)>;
}
