//! Wire structures — direct encodings of PRD §11 (data models & schemas).
//!
//! Field names are kept terse (`v`, `dst`, `ttl_s`, …) to match the compact
//! CBOR wire and the JSON examples in the PRD. Every binary field uses
//! [`Bytes`] so it becomes a CBOR byte string. `Option` fields use
//! `skip_serializing_if` so absent values cost nothing on the wire.

use crate::address::Address;
use crate::codec::Bytes;
use serde::{Deserialize, Serialize};

/// Message priority classes (PRD FR-14): `SOS(0) > ALERT(1) > NORMAL(2) > BULK(3)`.
/// Lower numeric value = higher priority, so `Ord` sorts SOS first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Priority {
    /// One-tap SOS; preempts all other traffic at every hop (FR-27, FR-40).
    Sos = 0,
    /// Authority/relief alerts and "I'm safe" broadcasts.
    Alert = 1,
    /// Ordinary user messages.
    Normal = 2,
    /// Best-effort background traffic (bulk sync, large attachments).
    Bulk = 3,
}

impl Priority {
    pub fn from_u8(v: u8) -> Option<Priority> {
        match v {
            0 => Some(Priority::Sos),
            1 => Some(Priority::Alert),
            2 => Some(Priority::Normal),
            3 => Some(Priority::Bulk),
            _ => None,
        }
    }
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Lifecycle state of a message from the *sender's* point of view (PRD FR-11).
/// `composing → queued → carrying → in_transit → delivered → read`, plus the
/// terminal `expired` / `failed`. NFR-3: no silent loss — every message ends in
/// one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageState {
    Composing,
    Queued,
    Carrying,
    InTransit,
    /// Delivered *and* a valid signed receipt was matched — verifiable (FR-30..34).
    Delivered,
    Read,
    Expired,
    Failed,
}

/// The public half of an identity (PRD §11.1). The address is derived from
/// `sign_pub` by the `core` crate. Private keys never appear on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityPublic {
    /// Network address = `b64url(hash(sign_pub)[..16])`.
    pub id: Address,
    /// Ed25519 signing public key.
    pub sign_pub: Bytes,
    /// X25519 key-exchange public key.
    pub kex_pub: Bytes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub created_at: u64,
}

/// The message envelope carried by relays (PRD §11.2). Everything a relay needs
/// to route lives here; the actual message is inside `ciphertext` and is opaque
/// to every node except the recipient (FR-44). `src_sealed` hides the real
/// sender from relays (sealed sender, FR-45).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bundle {
    /// Wire version (`WIRE_VERSION`).
    pub v: u8,
    /// Unique id; used for dedup at every relay (FR-26).
    pub bundle_id: Bytes,
    /// Recipient network address.
    pub dst: Address,
    /// Sealed-sender blob: encrypts the true sender identity to the recipient.
    pub src_sealed: Bytes,
    /// Priority class (FR-14).
    pub priority: Priority,
    pub created_at: u64,
    /// Time-to-live in seconds; bundle is dropped once elapsed (FR-26). Default 7 days.
    pub ttl_s: u64,
    /// Max hops before drop (FR-26).
    pub hop_limit: u16,
    /// Hops taken so far.
    pub hops: u16,
    /// Spray-and-wait copy budget remaining (FR-24).
    pub copies_left: u16,
    /// Double-Ratchet-encrypted payload (opaque to relays).
    pub ciphertext: Bytes,
    /// Sender signature over the immutable header fields.
    pub sig: Bytes,
    /// Proof-of-work "postage" gating admission for NORMAL/BULK traffic
    /// (Hashcash-style, FR-46, §12.5). Absent for exempt classes (SOS). Not part
    /// of the signed header — it binds to `bundle_id`, which *is* signed, so it
    /// cannot be reused for another bundle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postage: Option<Bytes>,
    /// Erasure-coding fragment header (FR-28). When present, `ciphertext` holds
    /// one Reed-Solomon shard of a larger bundle rather than the whole payload;
    /// the recipient collects any `k` of `n` fragments (by `group`) and
    /// reconstructs the original. Not part of the signed header — tampering it
    /// only breaks reconstruction, which the end-to-end signature then rejects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frag: Option<FragHeader>,
}

/// Per-fragment erasure header (PRD FR-28; network-layer Problem C).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FragHeader {
    /// Group id shared by all fragments of a message (the original bundle id).
    pub group: Bytes,
    /// This shard's index, `0..n`.
    pub idx: u16,
    /// Data shards required to reconstruct.
    pub k: u16,
    /// Total shards produced.
    pub n: u16,
    /// Length of the original (pre-coding) ciphertext, for trimming padding.
    pub orig_len: u32,
}

impl Bundle {
    /// Is this bundle past its TTL relative to `now` (unix seconds)?
    pub fn is_expired(&self, now: u64) -> bool {
        now.saturating_sub(self.created_at) >= self.ttl_s
    }
    /// Has the bundle exhausted its hop budget?
    pub fn hops_exhausted(&self) -> bool {
        self.hops >= self.hop_limit
    }
}

/// The decrypted payload — only ever visible on the two endpoints (PRD §11.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Payload {
    #[serde(rename = "type")]
    pub kind: PayloadKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coords: Option<Coords>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battery_pct: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attach: Option<AttachChunk>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
}

/// Payload discriminant (PRD §11.3 `type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadKind {
    Text,
    Sos,
    Safe,
    Location,
    Alert,
    Receipt,
    GroupOp,
    AttachChunk,
}

/// GPS coordinates attached to SOS / location payloads (PRD §11.3, FR-40/43).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Coords {
    pub lat: f64,
    pub lon: f64,
    /// Horizontal accuracy in metres.
    pub acc_m: u32,
}

/// One chunk of a chunked attachment (PRD §11.3 / FR-13).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttachChunk {
    pub id: String,
    pub idx: u32,
    pub total: u32,
    pub bytes: Bytes,
}

/// End-to-end delivery receipt (PRD §11.4). Emitted by the recipient on decrypt
/// and diffused back to the sender; `verify()` checks it offline (FR-31, §12.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryReceipt {
    pub bundle_id: Bytes,
    pub recipient: Address,
    pub delivered_at: u64,
    /// `Sign_R(bundle_id || delivered_at)`.
    pub sig: Bytes,
}

/// Relay-level custody receipt (PRD §11.5 / FR-25). A node signs this to accept
/// responsibility for a bundle before the previous holder may drop its copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustodyReceipt {
    pub bundle_id: Bytes,
    pub custodian: Address,
    pub at: u64,
    pub sig: Bytes,
}

/// Signed gateway announce (PRD §11.6 / FR-36). Propagates a few hops; nodes
/// cache the best per gateway by `seq` and decay by `expires_at` (§12.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayAnnounce {
    pub gateway: Address,
    /// Capabilities, e.g. `["internet","lora_in865"]`.
    pub caps: Vec<String>,
    /// Coarse load 0..=1, encoded as 0..=255 for a compact integer wire.
    pub load_q: u8,
    /// Monotonic freshness counter.
    pub seq: u64,
    pub expires_at: u64,
    pub sig: Bytes,
}

impl GatewayAnnounce {
    /// Load as a 0.0..=1.0 float.
    pub fn load(&self) -> f32 {
        self.load_q as f32 / 255.0
    }
    pub fn from_load(load: f32) -> u8 {
        (load.clamp(0.0, 1.0) * 255.0).round() as u8
    }
    pub fn is_fresh(&self, now: u64) -> bool {
        now < self.expires_at
    }
    pub fn has_cap(&self, cap: &str) -> bool {
        self.caps.iter().any(|c| c == cap)
    }
}

/// One entry in an identity's append-only, hash-linked log (PRD §11.7 / FR-30).
/// Chaining `prev = hash(prev_entry)` makes the log tamper-evident (SSB-style,
/// Tarr 2019) and is the substrate for verifiable delivery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub seq: u64,
    /// Hash of the previous entry (empty for the genesis entry).
    pub prev: Bytes,
    pub event: LogEvent,
    /// The bundle this entry refers to.
    pub ref_id: Bytes,
    pub at: u64,
    /// Author signature over the entry (excluding this field).
    pub sig: Bytes,
}

/// Kind of event recorded in the log (PRD §11.7 `event`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogEvent {
    Sent,
    Received,
    Receipt,
}

/// A stored contact (PRD §11.8 / FR-9). Verification state gates trust and
/// endpoint moderation (FR-48).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub address: Address,
    pub sign_pub: Bytes,
    pub kex_pub: Bytes,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// True once keys were confirmed out-of-band (QR TOFU, FR-6).
    pub verified: bool,
    pub blocked: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{from_cbor, to_cbor};
    use crate::WIRE_VERSION;

    fn addr(b: u8) -> Address {
        Address::from_hash_bytes([b; 16])
    }

    #[test]
    fn priority_orders_sos_first() {
        assert!(Priority::Sos < Priority::Alert);
        assert!(Priority::Alert < Priority::Normal);
        assert!(Priority::Normal < Priority::Bulk);
    }

    #[test]
    fn bundle_cbor_roundtrip() {
        let b = Bundle {
            v: WIRE_VERSION,
            bundle_id: Bytes::new(vec![1, 2, 3, 4]),
            dst: addr(0xAA),
            src_sealed: Bytes::new(vec![9; 32]),
            priority: Priority::Sos,
            created_at: 1_690_000_000,
            ttl_s: 604_800,
            hop_limit: 32,
            hops: 0,
            copies_left: 6,
            ciphertext: Bytes::new(vec![7; 64]),
            sig: Bytes::new(vec![5; 64]),
            postage: Some(Bytes::new(vec![0; 8])),
            frag: None,
        };
        let enc = to_cbor(&b).unwrap();
        let dec: Bundle = from_cbor(&enc).unwrap();
        assert_eq!(b, dec);
    }

    #[test]
    fn expiry_and_hops() {
        let mut b = Bundle {
            v: WIRE_VERSION,
            bundle_id: Bytes::new(vec![1]),
            dst: addr(1),
            src_sealed: Bytes::default(),
            priority: Priority::Normal,
            created_at: 1000,
            ttl_s: 100,
            hop_limit: 3,
            hops: 0,
            copies_left: 6,
            ciphertext: Bytes::default(),
            sig: Bytes::default(),
            postage: None,
            frag: None,
        };
        assert!(!b.is_expired(1050));
        assert!(b.is_expired(1100));
        b.hops = 3;
        assert!(b.hops_exhausted());
    }

    #[test]
    fn payload_roundtrip_with_options() {
        let p = Payload {
            kind: PayloadKind::Sos,
            body: Some("help".into()),
            coords: Some(Coords {
                lat: 12.9,
                lon: 77.5,
                acc_m: 8,
            }),
            battery_pct: Some(42),
            attach: None,
            group_id: None,
        };
        let enc = to_cbor(&p).unwrap();
        let dec: Payload = from_cbor(&enc).unwrap();
        assert_eq!(p, dec);
    }
}
