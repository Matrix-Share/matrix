//! MTU-aware fragmentation and reassembly (PRD §8.4; spectrum doc §3).
//!
//! Different networks have wildly different frame sizes — ultrasound carries
//! tens of bytes, LoRa ~222 B, internet ~64 KB. To keep a message *independent
//! of how it travels*, the engine splits each logical unit (a beacon, a bundle,
//! or a CRDT state blob) into [`Frame`]s no larger than the interface's usable
//! MTU, tags them with a shared message id, and the receiver reassembles them.
//!
//! This is deliberately simple and strictly bounded (fixed small header, hard
//! frame-count cap) — the framing layer is exactly where mesh messengers have
//! been broken before (Bridgefy, USENIX 2022), so it must never trust lengths
//! or allocate unboundedly.

use lifeline_proto::codec::{from_cbor, to_cbor};
use lifeline_proto::{Bytes, CodecError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Conservative per-frame header budget subtracted from an interface MTU when
/// computing usable payload. Covers the compact CBOR tuple framing (array
/// header + 16-byte mid + idx + total + kind + the data length prefix). Frames
/// are encoded as a positional tuple (not a map) specifically to keep this
/// overhead tiny so payloads fit small MTUs like ultrasound's 128 B.
pub const FRAME_HEADER_BYTES: usize = 36;

/// Hard cap on fragments per message — a defensive bound against a malicious
/// `total` field forcing huge allocations (framing-layer hardening).
pub const MAX_FRAGMENTS: u16 = 4096;

/// What a reassembled payload represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameKind {
    /// A node presence/identity beacon (discovery, FR-7/FR-8).
    Beacon,
    /// A DTN bundle fragment.
    Bundle,
    /// A CRDT shared-state blob (anti-entropy, §12.3).
    State,
    /// A selective-ACK control frame for lossy-link ARQ (see [`crate::arq`]).
    /// Always a single frame; its `data` carries the ACK's base + bitmap.
    Ack,
    /// A gossiped gateway announce `(GatewayAnnounce, distance)` — lets nodes
    /// build a gradient toward the nearest gateway (FR-36).
    Announce,
}

impl FrameKind {
    fn as_u8(self) -> u8 {
        match self {
            FrameKind::Beacon => 0,
            FrameKind::Bundle => 1,
            FrameKind::State => 2,
            FrameKind::Ack => 3,
            FrameKind::Announce => 4,
        }
    }
    fn from_u8(v: u8) -> Option<FrameKind> {
        match v {
            0 => Some(FrameKind::Beacon),
            1 => Some(FrameKind::Bundle),
            2 => Some(FrameKind::State),
            3 => Some(FrameKind::Ack),
            4 => Some(FrameKind::Announce),
            _ => None,
        }
    }
}

/// One fragment on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Message id shared by all fragments of one logical unit.
    pub mid: Bytes,
    /// Fragment index, `0..total`.
    pub idx: u16,
    /// Total fragment count.
    pub total: u16,
    pub kind: FrameKind,
    /// This fragment's slice of the payload.
    pub data: Bytes,
}

/// Compact positional wire form: `[mid, idx, total, kind, data]`. Encoding as a
/// CBOR array (not a map) omits field-name strings, shrinking the per-frame
/// overhead so payloads fit small-MTU links.
type FrameWire = (Bytes, u16, u16, u8, Bytes);

impl Frame {
    pub fn encode(&self) -> Result<Vec<u8>, CodecError> {
        let wire: FrameWire = (
            self.mid.clone(),
            self.idx,
            self.total,
            self.kind.as_u8(),
            self.data.clone(),
        );
        to_cbor(&wire)
    }
    pub fn decode(bytes: &[u8]) -> Result<Frame, CodecError> {
        let (mid, idx, total, kind_u8, data): FrameWire = from_cbor(bytes)?;
        let kind = FrameKind::from_u8(kind_u8)
            .ok_or_else(|| CodecError::Decode(format!("bad frame kind {kind_u8}")))?;
        Ok(Frame {
            mid,
            idx,
            total,
            kind,
            data,
        })
    }
}

/// Splits payloads into MTU-sized frames.
pub struct Fragmenter;

impl Fragmenter {
    /// Fragment `payload` into frames whose `data` is at most `usable_mtu`
    /// bytes. `mid` must be unique per logical message. Returns an error if the
    /// MTU is too small or the payload would exceed [`MAX_FRAGMENTS`].
    pub fn fragment(
        kind: FrameKind,
        mid: Bytes,
        payload: &[u8],
        usable_mtu: usize,
    ) -> crate::Result<Vec<Frame>> {
        if usable_mtu == 0 {
            return Err(crate::TransportError::MtuTooSmall(usable_mtu));
        }
        // ceil division, with an empty payload still producing one frame.
        let total_usize = payload.len().div_ceil(usable_mtu).max(1);
        if total_usize > MAX_FRAGMENTS as usize {
            return Err(crate::TransportError::Mtu {
                frame: payload.len(),
                mtu: usable_mtu * MAX_FRAGMENTS as usize,
            });
        }
        let total = total_usize as u16;
        let mut frames = Vec::with_capacity(total_usize);
        for (idx, chunk) in payload.chunks(usable_mtu).enumerate() {
            frames.push(Frame {
                mid: mid.clone(),
                idx: idx as u16,
                total,
                kind,
                data: Bytes::new(chunk.to_vec()),
            });
        }
        if frames.is_empty() {
            frames.push(Frame {
                mid,
                idx: 0,
                total: 1,
                kind,
                data: Bytes::default(),
            });
        }
        Ok(frames)
    }
}

/// Collects fragments until a message is complete.
///
/// Bounded: rejects implausible `total`s and only tracks in-flight messages.
#[derive(Default)]
pub struct Reassembler {
    partial: HashMap<Bytes, Partial>,
}

struct Partial {
    total: u16,
    kind: FrameKind,
    chunks: HashMap<u16, Vec<u8>>,
}

impl Reassembler {
    pub fn new() -> Self {
        Reassembler::default()
    }

    /// Feed one frame. Returns `Some((kind, payload))` once all fragments of
    /// its message have arrived (and clears that message's state).
    pub fn accept(&mut self, frame: Frame) -> Option<(FrameKind, Vec<u8>)> {
        // Reject nonsense before allocating.
        if frame.total == 0 || frame.total > MAX_FRAGMENTS || frame.idx >= frame.total {
            return None;
        }
        let entry = self
            .partial
            .entry(frame.mid.clone())
            .or_insert_with(|| Partial {
                total: frame.total,
                kind: frame.kind,
                chunks: HashMap::new(),
            });
        // A conflicting `total` for the same mid is a malformed/attack stream.
        if entry.total != frame.total {
            return None;
        }
        entry
            .chunks
            .entry(frame.idx)
            .or_insert_with(|| frame.data.0);

        if entry.chunks.len() == entry.total as usize {
            let Partial {
                total,
                kind,
                chunks,
            } = self.partial.remove(&frame.mid).unwrap();
            let mut out = Vec::new();
            for i in 0..total {
                out.extend_from_slice(chunks.get(&i)?);
            }
            return Some((kind, out));
        }
        None
    }

    /// Number of messages currently mid-reassembly (for diagnostics / bounding).
    pub fn in_flight(&self) -> usize {
        self.partial.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_and_reassemble_large_over_tiny_mtu() {
        let payload: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        let usable = 30; // emulate a tiny ultrasound-ish MTU
        let mid = Bytes::new(vec![7; 16]);
        let frames = Fragmenter::fragment(FrameKind::Bundle, mid, &payload, usable).unwrap();
        assert!(frames.len() > 100, "should fragment into many frames");
        assert!(frames.iter().all(|f| f.data.len() <= usable));

        // Deliver out of order to prove the reassembler is order-independent.
        let mut re = Reassembler::new();
        let mut result = None;
        let mut shuffled = frames;
        shuffled.reverse();
        for f in shuffled {
            if let Some((kind, out)) = re.accept(f) {
                assert_eq!(kind, FrameKind::Bundle);
                result = Some(out);
            }
        }
        assert_eq!(result.unwrap(), payload);
        assert_eq!(re.in_flight(), 0);
    }

    #[test]
    fn single_frame_when_fits() {
        let frames =
            Fragmenter::fragment(FrameKind::Beacon, Bytes::new(vec![1; 16]), b"hi", 100).unwrap();
        assert_eq!(frames.len(), 1);
    }

    #[test]
    fn rejects_malicious_total() {
        let mut re = Reassembler::new();
        let bad = Frame {
            mid: Bytes::new(vec![1; 16]),
            idx: 0,
            total: u16::MAX,
            kind: FrameKind::Bundle,
            data: Bytes::new(vec![0; 4]),
        };
        assert!(re.accept(bad).is_none());
        assert_eq!(re.in_flight(), 0);
    }
}
