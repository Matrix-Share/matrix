//! Native **Bluetooth LE** bearer — the platform-independent core.
//!
//! This is the "primary transport" the whole offline story rests on: two phones
//! that have never touched a tower still exchange bundles over BLE. The design
//! splits cleanly into two halves so the hard, reusable logic can be written and
//! tested without a radio:
//!
//! * **Engine-facing side** — reuses the existing [`crate::ChannelInterface`]
//!   with [`InterfaceCaps::ble`](crate::InterfaceCaps::ble). The engine already
//!   fragments each logical unit to the BLE `mtu` (244 B) and hands us frames; it
//!   never needs to know a radio is underneath.
//! * **Radio-facing side** — this module. A [`BleDriver`] pumps frames between
//!   those channels and a [`GattPort`], performing the one thing BLE specifically
//!   needs: **ATT-MTU segmentation**. A GATT write carries only `ATT_MTU − 3`
//!   bytes (as little as 20 on an unnegotiated link), so a 244-byte frame must be
//!   split into ordered segments and reassembled on the far side.
//!
//! [`GattPort`] is the seam a real radio implements: `btleplug` on desktop
//! (BlueZ / CoreBluetooth / WinRT), or CoreBluetooth / Android BLE on phones via
//! the mobile app. Those backends are inherently hardware-bound and live behind a
//! feature; everything in this file is pure logic and is unit-tested against an
//! in-memory GATT fabric. See [`docs/ble-transport.md`](https://github.com/matrix-share/matrix/blob/main/docs/ble-transport.md).

use crate::channel::Outbound;
use crate::interface::PeerId;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

/// 128-bit GATT **service** UUID that identifies a Lifeline node in a BLE
/// advertisement. Peers scan for this to discover each other; it is a fixed,
/// project-specific v4 UUID (not registered with the Bluetooth SIG — fine for a
/// custom service).
pub const SERVICE_UUID: &str = "6c69666e-11e5-11ee-be56-0242ac120002";

/// The **characteristic** on [`SERVICE_UUID`] that carries frame segments. The
/// central writes segments to it (write-without-response) and subscribes to its
/// notifications to receive segments back — a symmetric bidirectional pipe.
pub const FRAME_CHAR_UUID: &str = "6c69666e-11e5-11ee-be56-0242ac120003";

/// One-byte segment header prepended to every GATT write. Kept to a single byte
/// because on an unnegotiated link only ~20 payload bytes are available.
const SEG_HEADER: usize = 1;

/// Header bit marking the final segment of a frame.
const FLAG_FINAL: u8 = 0x01;

/// The smallest usable ATT payload we will ever segment for — BLE's default
/// `ATT_MTU` is 23, leaving 20 bytes after the 3-byte ATT write header. A real
/// backend should negotiate a larger MTU (up to 517) and report it via
/// [`GattPort::att_payload`], but we stay correct even if it never does.
pub const MIN_ATT_PAYLOAD: usize = 20;

/// Hard cap on a single peer's in-flight reassembly buffer. A peer that streams
/// non-final segments forever must never grow our memory without bound — the
/// exact framing-layer footgun that has broken other BLE messengers. Generously
/// above one max fragmented unit, but finite.
pub const MAX_REASSEMBLY: usize = 64 * 1024;

/// Split one engine frame into ATT-sized segments, each `[flag] ++ chunk`, where
/// the final segment has [`FLAG_FINAL`] set. `att_payload` is the total bytes a
/// single GATT write can carry (including our 1-byte header); it is clamped up to
/// at least `SEG_HEADER + 1` so progress is always made. An empty frame yields a
/// single header-only final segment (so it round-trips to an empty frame).
pub fn segment(frame: &[u8], att_payload: usize) -> Vec<Vec<u8>> {
    let chunk_len = att_payload.saturating_sub(SEG_HEADER).max(1);
    if frame.is_empty() {
        return vec![vec![FLAG_FINAL]];
    }
    let chunks: Vec<&[u8]> = frame.chunks(chunk_len).collect();
    let last = chunks.len() - 1;
    chunks
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| {
            let mut seg = Vec::with_capacity(SEG_HEADER + chunk.len());
            seg.push(if i == last { FLAG_FINAL } else { 0 });
            seg.extend_from_slice(chunk);
            seg
        })
        .collect()
}

/// Reassembles per-peer segment streams back into whole frames. A BLE GATT
/// connection delivers writes/notifications reliably and in order per
/// characteristic, so a one-bit "final" marker is sufficient — no sequence
/// numbers. Bounded by [`MAX_REASSEMBLY`] per peer.
#[derive(Default)]
pub struct SegmentReassembler {
    buf: HashMap<PeerId, Vec<u8>>,
}

impl SegmentReassembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one raw segment from `peer`. Returns the whole frame once its final
    /// segment arrives. Drops the peer's buffer (returning `None`) if it would
    /// exceed [`MAX_REASSEMBLY`], or if the segment is empty/malformed.
    pub fn accept(&mut self, peer: PeerId, seg: &[u8]) -> Option<Vec<u8>> {
        let (&flag, data) = seg.split_first()?;
        let entry = self.buf.entry(peer).or_default();
        if entry.len() + data.len() > MAX_REASSEMBLY {
            // Overflow: abandon this peer's stream rather than grow unboundedly.
            self.buf.remove(&peer);
            return None;
        }
        entry.extend_from_slice(data);
        if flag & FLAG_FINAL != 0 {
            return self.buf.remove(&peer);
        }
        None
    }

    /// Number of peers with a partially-received frame (diagnostics/bounding).
    pub fn in_flight(&self) -> usize {
        self.buf.len()
    }
}

/// The platform seam a real BLE radio implements. One `GattPort` represents this
/// node's live BLE state: the peers it is connected to, how many payload bytes it
/// can push per write to each, an outbound write, and a drain of inbound
/// segments. A `btleplug` (desktop) or CoreBluetooth/Android (mobile) backend
/// implements this; [`BleDriver`] contains all the framing on top of it.
pub trait GattPort {
    /// Peers currently connected over BLE (their stable [`PeerId`]).
    fn connected_peers(&self) -> Vec<PeerId>;

    /// Usable bytes per GATT write to `peer` (its negotiated `ATT_MTU − 3`).
    /// Implementations should return at least [`MIN_ATT_PAYLOAD`].
    fn att_payload(&self, peer: PeerId) -> usize;

    /// Write one already-segmented chunk (`≤ att_payload(peer)` bytes) to `peer`.
    fn write(&mut self, peer: PeerId, bytes: &[u8]) -> crate::Result<()>;

    /// Drain inbound segments received since the last call, tagged with the peer
    /// they came from.
    fn drain(&mut self) -> Vec<(PeerId, Vec<u8>)>;
}

/// Bridges the engine's [`Outbound`]/inbound channels to a [`GattPort`], adding
/// ATT-MTU segmentation on the way out and reassembly on the way in. Construct it
/// on the radio thread with the *other* ends of the channels held by a
/// [`crate::ChannelInterface`], then call [`pump`](BleDriver::pump) in a loop.
pub struct BleDriver<P: GattPort> {
    port: P,
    outbound: Receiver<Outbound>,
    inbound: Sender<(PeerId, Vec<u8>)>,
    peers: Arc<Mutex<Vec<PeerId>>>,
    reasm: SegmentReassembler,
}

impl<P: GattPort> BleDriver<P> {
    pub fn new(
        port: P,
        outbound: Receiver<Outbound>,
        inbound: Sender<(PeerId, Vec<u8>)>,
        peers: Arc<Mutex<Vec<PeerId>>>,
    ) -> Self {
        Self {
            port,
            outbound,
            inbound,
            peers,
            reasm: SegmentReassembler::new(),
        }
    }

    /// One servicing pass: publish the current peer set, flush every queued
    /// outbound frame as ATT segments, and reassemble inbound segments into
    /// frames the engine can poll. Returns `true` if any work was done (so a
    /// caller can back off when idle).
    pub fn pump(&mut self) -> bool {
        let mut did_work = false;

        // 1. Publish who we can reach so the engine's scan() sees them.
        let connected = self.port.connected_peers();
        if let Ok(mut p) = self.peers.lock() {
            if *p != connected {
                *p = connected.clone();
                did_work = true;
            }
        }

        // 2. Flush outbound frames: segment each to the target peer's ATT MTU.
        while let Ok(Outbound { to, frame }) = self.outbound.try_recv() {
            did_work = true;
            let targets = match to {
                Some(peer) => vec![peer],
                None => self.port.connected_peers(),
            };
            for peer in targets {
                let payload = self.port.att_payload(peer).max(MIN_ATT_PAYLOAD);
                for seg in segment(&frame, payload) {
                    let _ = self.port.write(peer, &seg);
                }
            }
        }

        // 3. Reassemble inbound segments and hand whole frames to the engine.
        for (peer, seg) in self.port.drain() {
            did_work = true;
            if let Some(frame) = self.reasm.accept(peer, &seg) {
                let _ = self.inbound.send((peer, frame));
            }
        }

        did_work
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::mpsc::channel;

    #[test]
    fn segments_round_trip_over_a_tiny_att_mtu() {
        let frame: Vec<u8> = (0..500u32).map(|i| (i % 251) as u8).collect();
        let att = 20; // unnegotiated BLE link
        let segs = segment(&frame, att);
        assert!(segs.len() > 20, "500 B over ~19 B/seg is many segments");
        assert!(segs.iter().all(|s| s.len() <= att));
        // Exactly one final segment, and it's the last.
        assert_eq!(segs.iter().filter(|s| s[0] & FLAG_FINAL != 0).count(), 1);
        assert!(segs.last().unwrap()[0] & FLAG_FINAL != 0);

        let mut re = SegmentReassembler::new();
        let mut out = None;
        for s in &segs {
            if let Some(f) = re.accept(7, s) {
                out = Some(f);
            }
        }
        assert_eq!(out.unwrap(), frame);
        assert_eq!(re.in_flight(), 0);
    }

    #[test]
    fn single_and_empty_frames_round_trip() {
        for frame in [b"hi".to_vec(), Vec::new()] {
            let segs = segment(&frame, 244);
            assert_eq!(segs.len(), 1);
            let mut re = SegmentReassembler::new();
            let got = segs.iter().find_map(|s| re.accept(1, s));
            assert_eq!(got.unwrap(), frame);
        }
    }

    #[test]
    fn reassembly_is_bounded_against_a_runaway_stream() {
        let mut re = SegmentReassembler::new();
        // Non-final segments that together exceed the cap must be dropped, not
        // buffered forever.
        let seg = {
            let mut s = vec![0u8]; // no FINAL flag
            s.extend_from_slice(&[0xAB; 1024]);
            s
        };
        // Feed far more than the cap; the buffer must reset at the ceiling
        // rather than complete or grow past it.
        let rounds = MAX_REASSEMBLY / 1024 + 4;
        let mut accepted_any = false;
        for _ in 0..rounds {
            if re.accept(9, &seg).is_some() {
                accepted_any = true;
            }
        }
        assert!(!accepted_any, "a never-final stream never completes");
        // After a legitimate final segment, reassembly still works (buffer wasn't
        // corrupted) and stays well under the cap.
        let mut done = vec![FLAG_FINAL];
        done.extend_from_slice(b"end");
        let out = re
            .accept(9, &done)
            .expect("final segment completes the frame");
        assert!(out.len() <= MAX_REASSEMBLY);
        assert_eq!(re.in_flight(), 0, "no dangling buffers once drained");
    }

    /// An in-memory two-node "air": each node writes segments into the queue the
    /// other drains. Proves the full engine↔BLE data path (segmentation over a
    /// tiny MTU, reassembly, peer tagging) without any radio.
    struct MockGattPort {
        peer: PeerId,
        att: usize,
        tx: Arc<Mutex<VecDeque<Vec<u8>>>>, // segments we send to `peer`
        rx: Arc<Mutex<VecDeque<Vec<u8>>>>, // segments arriving from `peer`
    }
    impl GattPort for MockGattPort {
        fn connected_peers(&self) -> Vec<PeerId> {
            vec![self.peer]
        }
        fn att_payload(&self, _peer: PeerId) -> usize {
            self.att
        }
        fn write(&mut self, _peer: PeerId, bytes: &[u8]) -> crate::Result<()> {
            self.tx.lock().unwrap().push_back(bytes.to_vec());
            Ok(())
        }
        fn drain(&mut self) -> Vec<(PeerId, Vec<u8>)> {
            let mut q = self.rx.lock().unwrap();
            q.drain(..).map(|seg| (self.peer, seg)).collect()
        }
    }

    #[test]
    fn driver_delivers_a_large_frame_end_to_end_over_mock_ble() {
        // Shared directed links between node 1 (A) and node 2 (B).
        let ab = Arc::new(Mutex::new(VecDeque::new())); // A -> B
        let ba = Arc::new(Mutex::new(VecDeque::new())); // B -> A

        // Engine-facing channels for each node (as ChannelInterface would hold).
        let (a_out_tx, a_out_rx) = channel::<Outbound>();
        let (a_in_tx, _a_in_rx) = channel::<(PeerId, Vec<u8>)>();
        let (_b_out_tx, b_out_rx) = channel::<Outbound>();
        let (b_in_tx, b_in_rx) = channel::<(PeerId, Vec<u8>)>();

        let mut driver_a = BleDriver::new(
            MockGattPort {
                peer: 2,
                att: 24,
                tx: ab.clone(),
                rx: ba.clone(),
            },
            a_out_rx,
            a_in_tx,
            Arc::new(Mutex::new(Vec::new())),
        );
        let mut driver_b = BleDriver::new(
            MockGattPort {
                peer: 1,
                att: 24,
                tx: ba.clone(),
                rx: ab.clone(),
            },
            b_out_rx,
            b_in_tx,
            Arc::new(Mutex::new(Vec::new())),
        );

        // A's engine wants to send a big frame to peer 2 (B).
        let frame: Vec<u8> = (0..1000u32).map(|i| (i * 7 % 251) as u8).collect();
        a_out_tx
            .send(Outbound {
                to: Some(2),
                frame: frame.clone(),
            })
            .unwrap();

        driver_a.pump(); // segments A->B written into `ab`
        driver_b.pump(); // B drains `ab`, reassembles, pushes inbound

        // B's engine receives the exact frame, tagged as coming from peer 1 (A).
        let (from, got) = b_in_rx.try_recv().unwrap();
        assert_eq!(from, 1);
        assert_eq!(got, frame);
    }
}
