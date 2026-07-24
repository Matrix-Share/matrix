//! Selective-repeat ARQ for lossy unicast links (Dhwani "noise robustness").
//!
//! The in-memory medium is reliable, but real ultrasound, LoRa and even
//! congested UDP drop frames — and a single lost fragment makes the whole
//! reassembly stall forever. This module adds **per-message reliability** on top
//! of [`crate::frame`]: the sender remembers the frames of each unicast bundle
//! and retransmits the ones that go unacknowledged, while the receiver replies
//! with a compact **selective ACK** (cumulative base + a windowed bitmap) so the
//! sender resends *only* what was actually lost — selective repeat, not
//! go-back-N.
//!
//! Design choices that keep it correct and MTU-safe:
//! * The sender blasts every frame once up front (happy-path latency is
//!   unchanged from best-effort), then retransmits unacked frames on an RTO
//!   timer. Retransmitting *all still-unacked* frames each round is always
//!   correct; the bitmap is a bandwidth optimisation, not a correctness
//!   requirement.
//! * The ACK reports a cumulative `base` (every index below it is implicitly
//!   acknowledged, TCP-style) plus a bounded bitmap for the next
//!   [`ACK_WINDOW`] indices, so an ACK frame fits even ultrasound's tiny MTU no
//!   matter how many fragments the message has.
//! * Only **Bundle** frames are made reliable. Beacons are broadcast (ACKing
//!   them would be an ACK storm) and CRDT State is periodic and self-healing, so
//!   both stay best-effort.

use crate::frame::{Frame, FrameKind};
use crate::interface::PeerId;
use lifeline_proto::codec::{from_cbor, to_cbor};
use lifeline_proto::Bytes;
use std::collections::{HashMap, VecDeque};

/// Indices covered by an ACK's selective bitmap, above the cumulative base.
/// 256 → a 32-byte bitmap, which fits inside every interface's usable MTU.
pub const ACK_WINDOW: u16 = 256;

/// Ticks an unacked message waits before its frames are retransmitted.
pub const DEFAULT_RTO: u64 = 3;

/// Retransmission rounds before the sender gives up on a message (the DTN-layer
/// re-spray remains the coarse backstop above this).
pub const DEFAULT_MAX_ROUNDS: u16 = 12;

/// Completed message-ids remembered per link so a late retransmission (whose ACK
/// was lost) is re-acknowledged instead of re-delivered.
const COMPLETED_MEMORY: usize = 1024;

fn set_bit(buf: &mut Vec<u8>, i: usize) {
    let byte = i / 8;
    if byte >= buf.len() {
        buf.resize(byte + 1, 0);
    }
    buf[byte] |= 1 << (i % 8);
}

fn get_bit(buf: &[u8], i: usize) -> bool {
    let byte = i / 8;
    byte < buf.len() && (buf[byte] >> (i % 8)) & 1 == 1
}

/// Selective-ACK wire form: `[base, total, bitmap]`. `base` = one past the
/// highest contiguously-received index; every index `< base` is acknowledged.
/// `bitmap` bit `j` marks index `base + j` as received (for the next
/// [`ACK_WINDOW`] indices).
type AckWire = (u16, u16, Bytes);

/// Encode a selective ACK as a single self-contained control [`Frame`] (its
/// `mid` names the message being acknowledged).
pub fn encode_ack(mid: &Bytes, base: u16, total: u16, bitmap: Vec<u8>) -> Option<Vec<u8>> {
    let wire: AckWire = (base, total, Bytes::new(bitmap));
    let data = to_cbor(&wire).ok()?;
    Frame {
        mid: mid.clone(),
        idx: 0,
        total: 1,
        kind: FrameKind::Ack,
        data: Bytes::new(data),
    }
    .encode()
    .ok()
}

/// A message the sender is keeping alive until every fragment is acknowledged.
struct TxMsg {
    frames: Vec<Vec<u8>>,
    acked: Vec<bool>,
    last_round: u64,
    rounds: u16,
}

impl TxMsg {
    fn all_acked(&self) -> bool {
        self.acked.iter().all(|&a| a)
    }
}

/// Sender half: tracks in-flight unicast messages and reissues lost frames.
#[derive(Default)]
pub struct ArqTx {
    msgs: HashMap<(PeerId, Bytes), TxMsg>,
    retransmits: u64,
}

impl ArqTx {
    pub fn new() -> Self {
        ArqTx::default()
    }

    /// Start tracking a just-sent message so its lost frames can be retransmitted.
    /// `frames` are the already-encoded fragments in index order.
    pub fn track(&mut self, peer: PeerId, mid: Bytes, frames: Vec<Vec<u8>>, now: u64) {
        if frames.is_empty() {
            return;
        }
        let acked = vec![false; frames.len()];
        self.msgs.insert(
            (peer, mid),
            TxMsg {
                frames,
                acked,
                last_round: now,
                rounds: 0,
            },
        );
    }

    /// Apply a selective ACK from `peer`. Frames below `base` and those flagged
    /// in `bitmap` are marked acknowledged; a fully-acked message is dropped.
    pub fn on_ack(&mut self, peer: PeerId, raw: &Frame) {
        let Ok((base, _total, bitmap)) = from_cbor::<AckWire>(&raw.data.0) else {
            return;
        };
        let key = (peer, raw.mid.clone());
        let Some(msg) = self.msgs.get_mut(&key) else {
            return;
        };
        let n = msg.acked.len();
        for a in msg.acked.iter_mut().take(base as usize).take(n) {
            *a = true;
        }
        for j in 0..ACK_WINDOW as usize {
            let idx = base as usize + j;
            if idx < n && get_bit(&bitmap.0, j) {
                msg.acked[idx] = true;
            }
        }
        if msg.all_acked() {
            self.msgs.remove(&key);
        }
    }

    /// Frames whose message is past its RTO: `(peer, encoded_frame)` to resend.
    /// Advances each due message's retransmit round and abandons any that have
    /// exhausted `max_rounds`.
    pub fn due(&mut self, now: u64, rto: u64, max_rounds: u16) -> Vec<(PeerId, Vec<u8>)> {
        let mut out = Vec::new();
        let mut give_up = Vec::new();
        for (key, msg) in self.msgs.iter_mut() {
            if now.saturating_sub(msg.last_round) < rto {
                continue;
            }
            if msg.rounds >= max_rounds {
                give_up.push(key.clone());
                continue;
            }
            msg.rounds += 1;
            msg.last_round = now;
            for (i, frame) in msg.frames.iter().enumerate() {
                if !msg.acked[i] {
                    out.push((key.0, frame.clone()));
                    self.retransmits += 1;
                }
            }
        }
        for key in give_up {
            self.msgs.remove(&key);
        }
        out
    }

    /// Messages still awaiting full acknowledgement (diagnostics / tests).
    pub fn pending(&self) -> usize {
        self.msgs.len()
    }

    /// Total frame retransmissions performed (diagnostics).
    pub fn retransmits(&self) -> u64 {
        self.retransmits
    }
}

/// What [`ArqRx::accept`] produced for one inbound frame.
pub struct RxOut {
    /// A fully reassembled payload, if this frame completed its message.
    pub payload: Option<(FrameKind, Vec<u8>)>,
    /// A selective ACK to transmit back to the source peer, if one is due.
    pub ack: Option<Vec<u8>>,
}

/// Per-message reassembly progress on the receiver.
struct RxProgress {
    total: u16,
    have: Vec<bool>,
    chunks: HashMap<u16, Vec<u8>>,
}

/// Receiver half: reassembles fragments and emits selective ACKs for reliable
/// (Bundle) messages, suppressing re-delivery of already-completed ones.
#[derive(Default)]
pub struct ArqRx {
    partial: HashMap<Bytes, RxProgress>,
    completed: HashMap<Bytes, u16>,
    completed_order: VecDeque<Bytes>,
}

impl ArqRx {
    pub fn new() -> Self {
        ArqRx::default()
    }

    /// Feed one decoded data frame. Returns any completed payload plus, for
    /// reliable Bundle frames, a selective ACK to send back to the sender.
    pub fn accept(&mut self, frame: Frame) -> RxOut {
        // Defensive bounds (mirrors the reassembler's hardening).
        if frame.total == 0 || frame.total > crate::frame::MAX_FRAGMENTS || frame.idx >= frame.total
        {
            return RxOut {
                payload: None,
                ack: None,
            };
        }
        let want_ack = frame.kind == FrameKind::Bundle;

        // A retransmission of an already-completed message: re-ACK (base=total)
        // so the sender can stop, but do not reprocess the payload.
        if let Some(&total) = self.completed.get(&frame.mid) {
            let ack = want_ack
                .then(|| encode_ack(&frame.mid, total, total, Vec::new()))
                .flatten();
            return RxOut { payload: None, ack };
        }

        let entry = self
            .partial
            .entry(frame.mid.clone())
            .or_insert_with(|| RxProgress {
                total: frame.total,
                have: vec![false; frame.total as usize],
                chunks: HashMap::new(),
            });
        // A conflicting `total` for the same mid is a malformed/attack stream.
        if entry.total != frame.total {
            return RxOut {
                payload: None,
                ack: None,
            };
        }
        if !entry.have[frame.idx as usize] {
            entry.have[frame.idx as usize] = true;
            entry.chunks.insert(frame.idx, frame.data.0.clone());
        }

        // Complete?
        if entry.chunks.len() == entry.total as usize {
            let progress = self.partial.remove(&frame.mid).unwrap();
            let mut out = Vec::new();
            for i in 0..progress.total {
                match progress.chunks.get(&i) {
                    Some(c) => out.extend_from_slice(c),
                    None => {
                        return RxOut {
                            payload: None,
                            ack: None,
                        }
                    }
                }
            }
            self.remember_completed(frame.mid.clone(), progress.total);
            let ack = want_ack
                .then(|| encode_ack(&frame.mid, progress.total, progress.total, Vec::new()))
                .flatten();
            return RxOut {
                payload: Some((frame.kind, out)),
                ack,
            };
        }

        // Still incomplete: build a selective ACK (cumulative base + window bitmap).
        let ack = if want_ack {
            let (base, bitmap) = self.selective_ack(&frame.mid);
            encode_ack(&frame.mid, base, frame.total, bitmap)
        } else {
            None
        };
        RxOut { payload: None, ack }
    }

    fn selective_ack(&self, mid: &Bytes) -> (u16, Vec<u8>) {
        let Some(p) = self.partial.get(mid) else {
            return (0, Vec::new());
        };
        // base = first not-yet-received index (everything below is contiguous).
        let base = p.have.iter().position(|&h| !h).unwrap_or(p.have.len()) as u16;
        let mut bitmap = Vec::new();
        for j in 0..ACK_WINDOW as usize {
            let idx = base as usize + j;
            if idx >= p.have.len() {
                break;
            }
            if p.have[idx] {
                set_bit(&mut bitmap, j);
            }
        }
        (base, bitmap)
    }

    fn remember_completed(&mut self, mid: Bytes, total: u16) {
        if self.completed.insert(mid.clone(), total).is_none() {
            self.completed_order.push_back(mid);
            if self.completed_order.len() > COMPLETED_MEMORY {
                if let Some(old) = self.completed_order.pop_front() {
                    self.completed.remove(&old);
                }
            }
        }
    }

    /// Messages currently mid-reassembly (diagnostics / bounding).
    pub fn in_flight(&self) -> usize {
        self.partial.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Fragmenter;

    /// Deterministic lossy link: drop a frame whenever the pseudo-random stream
    /// says so, then run sender/receiver ARQ to convergence.
    #[test]
    fn selective_repeat_recovers_from_heavy_loss() {
        let payload: Vec<u8> = (0..4000u32).map(|i| (i % 251) as u8).collect();
        let mid = Bytes::new(vec![9; 16]);
        let usable = 90; // ultrasound-ish tiny MTU → ~45 fragments
        let frames: Vec<Vec<u8>> =
            Fragmenter::fragment(FrameKind::Bundle, mid.clone(), &payload, usable)
                .unwrap()
                .iter()
                .map(|f| f.encode().unwrap())
                .collect();
        assert!(frames.len() > 40);

        let mut tx = ArqTx::new();
        let mut rx = ArqRx::new();
        let peer: PeerId = 1;
        let mut lcg: u32 = 0x1234_5678; // deterministic PRNG for drop decisions
        let mut drop = || {
            lcg = lcg.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (lcg >> 24) % 10 < 4 // ~40% loss
        };

        // Round 0: initial blast (lossy), then track for retransmit.
        let mut delivered: Option<Vec<u8>> = None;
        let mut acks_back: Vec<Frame> = Vec::new();
        for f in &frames {
            if !drop() {
                let frame = Frame::decode(f).unwrap();
                let out = rx.accept(frame);
                if let Some((_, p)) = out.payload {
                    delivered = Some(p);
                }
                if let Some(a) = out.ack {
                    if !drop() {
                        acks_back.push(Frame::decode(&a).unwrap());
                    }
                }
            }
        }
        tx.track(peer, mid.clone(), frames.clone(), 0);
        for a in acks_back.drain(..) {
            tx.on_ack(peer, &a);
        }

        // Retransmit rounds until the receiver has the whole payload.
        let mut now = DEFAULT_RTO;
        let mut rounds = 0;
        while delivered.is_none() && rounds < 100 {
            let due = tx.due(now, DEFAULT_RTO, DEFAULT_MAX_ROUNDS);
            for (_p, enc) in due {
                if !drop() {
                    let frame = Frame::decode(&enc).unwrap();
                    let out = rx.accept(frame);
                    if let Some((_, p)) = out.payload {
                        delivered = Some(p);
                    }
                    if let Some(a) = out.ack {
                        if !drop() {
                            tx.on_ack(peer, &Frame::decode(&a).unwrap());
                        }
                    }
                }
            }
            now += DEFAULT_RTO;
            rounds += 1;
        }
        assert_eq!(delivered.expect("payload eventually reassembled"), payload);
    }

    #[test]
    fn completed_message_is_reacked_not_redelivered() {
        let payload = b"small".to_vec();
        let mid = Bytes::new(vec![3; 16]);
        let frames: Vec<Vec<u8>> =
            Fragmenter::fragment(FrameKind::Bundle, mid.clone(), &payload, 4)
                .unwrap()
                .iter()
                .map(|f| f.encode().unwrap())
                .collect();
        let mut rx = ArqRx::new();
        let mut deliveries = 0;
        // Deliver twice over (a duplicate full retransmission).
        for _ in 0..2 {
            for f in &frames {
                let out = rx.accept(Frame::decode(f).unwrap());
                if out.payload.is_some() {
                    deliveries += 1;
                }
            }
        }
        assert_eq!(
            deliveries, 1,
            "a completed message is delivered exactly once"
        );
    }

    #[test]
    fn ack_fits_tiny_mtu_even_for_max_fragments() {
        // A selective ACK must fit an ultrasound frame regardless of message size.
        let mid = Bytes::new(vec![1; 16]);
        let mut bitmap = Vec::new();
        set_bit(&mut bitmap, ACK_WINDOW as usize - 1);
        let enc = encode_ack(&mid, 2000, 4096, bitmap).unwrap();
        assert!(
            enc.len() <= 128,
            "ack {} bytes must fit ultrasound MTU",
            enc.len()
        );
    }
}
