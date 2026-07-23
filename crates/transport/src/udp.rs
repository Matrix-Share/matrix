//! A real, infrastructureless UDP/multicast [`Interface`].
//!
//! Unlike the relay-backed [`crate::ChannelInterface`], this transport needs **no
//! server**: nodes on the same LAN discover each other and exchange frames
//! directly over UDP multicast (with optional explicit seed peers for networks
//! where multicast is filtered). It is the closest software stand-in for the
//! app's true peer-to-peer nature — kill every server and two laptops on the
//! same Wi-Fi still mesh.
//!
//! Design:
//! * Presence/beacon and broadcast frames go to a **multicast group**; targeted
//!   offers are **unicast** to a peer's address.
//! * Each datagram is prefixed with a 4-byte instance id (the process id) so a
//!   node ignores its own multicast loopback.
//! * `PeerId` encodes the peer's `SocketAddrV4` directly, so no peer table is
//!   needed to unicast back.
//!
//! The socket is non-blocking; the engine's `poll()` drains it each tick — no
//! extra threads.

use crate::caps::{InterfaceCaps, InterfaceKind, RangeClass, ThroughputClass};
use crate::interface::{Interface, PeerId};
use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::sync::atomic::{AtomicU32, Ordering};

/// Process-wide counter so every interface gets a distinct instance id (two
/// interfaces in one process must not filter each other's traffic as "self").
static INSTANCE_SEQ: AtomicU32 = AtomicU32::new(0);

/// Default multicast group for Lifeline LAN discovery (administratively scoped).
pub const DEFAULT_GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 42, 99);

/// Datagram payload cap kept below the typical 1500-byte path MTU to avoid IP
/// fragmentation (leaves room for IP/UDP headers + our 4-byte instance tag).
const UDP_MTU: usize = 1196;

fn addr_to_id(a: SocketAddrV4) -> PeerId {
    ((u32::from(*a.ip()) as u64) << 16) | (a.port() as u64)
}

fn id_to_addr(id: PeerId) -> SocketAddrV4 {
    let ip = Ipv4Addr::from((id >> 16) as u32);
    let port = (id & 0xFFFF) as u16;
    SocketAddrV4::new(ip, port)
}

/// A UDP-backed transport interface.
pub struct UdpInterface {
    caps: InterfaceCaps,
    socket: UdpSocket,
    instance: u32,
    /// Multicast group:port used for broadcast/discovery (if joined).
    group: Option<SocketAddrV4>,
    /// Explicit peers to also reach on broadcast (multicast-less networks).
    seeds: Vec<SocketAddrV4>,
    /// Peers we have received datagrams from.
    heard: HashSet<PeerId>,
    buf: Vec<u8>,
}

impl UdpInterface {
    /// Bind a UDP interface on `port`. If `join_group` is set, join that
    /// multicast group for zero-config discovery. `seeds` are explicit unicast
    /// peers to also reach (useful when multicast is unavailable).
    pub fn bind(
        port: u16,
        join_group: Option<Ipv4Addr>,
        seeds: Vec<SocketAddrV4>,
    ) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port))?;
        socket.set_nonblocking(true)?;
        let group = if let Some(g) = join_group {
            socket.join_multicast_v4(&g, &Ipv4Addr::UNSPECIFIED)?;
            socket.set_multicast_loop_v4(true)?;
            Some(SocketAddrV4::new(g, port))
        } else {
            None
        };
        Ok(UdpInterface {
            caps: InterfaceCaps {
                kind: InterfaceKind::Internet, // IP-based, LAN-scoped
                name: "udp".into(),
                mtu: UDP_MTU,
                throughput: ThroughputClass::High,
                range: RangeClass::Local,
                full_duplex: true,
                bridges_offmesh: false,
                region: None,
                max_power_mw: None,
            },
            socket,
            // Unique per interface: pid-mixed base + a process-wide counter, so
            // self-loopback is filtered while distinct interfaces still talk.
            instance: std::process::id()
                .wrapping_mul(2_654_435_761)
                .wrapping_add(INSTANCE_SEQ.fetch_add(1, Ordering::Relaxed)),
            group,
            seeds,
            heard: HashSet::new(),
            buf: vec![0u8; 2048],
        })
    }

    /// The local address the socket is bound to (useful to seed peers in tests).
    pub fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.socket.local_addr()
    }

    /// Add an explicit unicast peer to reach on broadcast (for networks where
    /// multicast is filtered, or to bootstrap discovery).
    pub fn add_seed(&mut self, addr: SocketAddrV4) {
        if !self.seeds.contains(&addr) {
            self.seeds.push(addr);
        }
    }

    fn tagged(&self, frame: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + frame.len());
        out.extend_from_slice(&self.instance.to_le_bytes());
        out.extend_from_slice(frame);
        out
    }

    fn send_to(&self, frame: &[u8], dst: SocketAddrV4) {
        let _ = self.socket.send_to(&self.tagged(frame), dst);
    }
}

impl Interface for UdpInterface {
    fn caps(&self) -> &InterfaceCaps {
        &self.caps
    }

    fn scan(&mut self) -> Vec<PeerId> {
        self.heard.iter().copied().collect()
    }

    fn send(&mut self, peer: PeerId, frame: &[u8]) -> crate::Result<()> {
        if frame.len() > self.caps.mtu {
            return Err(crate::TransportError::Mtu {
                frame: frame.len(),
                mtu: self.caps.mtu,
            });
        }
        self.send_to(frame, id_to_addr(peer));
        Ok(())
    }

    fn broadcast(&mut self, frame: &[u8]) -> crate::Result<()> {
        if frame.len() > self.caps.mtu {
            return Err(crate::TransportError::Mtu {
                frame: frame.len(),
                mtu: self.caps.mtu,
            });
        }
        if let Some(g) = self.group {
            self.send_to(frame, g);
        }
        for &seed in &self.seeds {
            self.send_to(frame, seed);
        }
        // When there is no multicast group, also reach everyone we've heard.
        if self.group.is_none() {
            let heard: Vec<PeerId> = self.heard.iter().copied().collect();
            for id in heard {
                self.send_to(frame, id_to_addr(id));
            }
        }
        Ok(())
    }

    fn poll(&mut self) -> Vec<(PeerId, Vec<u8>)> {
        let mut out = Vec::new();
        loop {
            match self.socket.recv_from(&mut self.buf) {
                Ok((n, src)) => {
                    if n < 4 {
                        continue;
                    }
                    let inst =
                        u32::from_le_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]);
                    if inst == self.instance {
                        continue; // our own multicast loopback
                    }
                    let data = self.buf[4..n].to_vec();
                    if let std::net::SocketAddr::V4(v4) = src {
                        let id = addr_to_id(v4);
                        self.heard.insert(id);
                        out.push((id, data));
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_id_roundtrip() {
        let a = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 42), 7005);
        assert_eq!(id_to_addr(addr_to_id(a)), a);
    }

    #[test]
    fn unicast_seed_exchange() {
        // Two sockets on loopback, seeded to each other, no multicast.
        let mut a = UdpInterface::bind(0, None, vec![]).unwrap();
        let mut b = UdpInterface::bind(0, None, vec![]).unwrap();
        let a_addr = match a.local_addr().unwrap() {
            std::net::SocketAddr::V4(v) => SocketAddrV4::new(Ipv4Addr::LOCALHOST, v.port()),
            _ => unreachable!(),
        };
        let b_addr = match b.local_addr().unwrap() {
            std::net::SocketAddr::V4(v) => SocketAddrV4::new(Ipv4Addr::LOCALHOST, v.port()),
            _ => unreachable!(),
        };
        a.seeds.push(b_addr);
        b.seeds.push(a_addr);

        a.broadcast(b"hello-from-a").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let got = b.poll();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, b"hello-from-a");
        // b can now unicast back using the learned peer id.
        b.send(got[0].0, b"reply").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let back = a.poll();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].1, b"reply");
    }
}
