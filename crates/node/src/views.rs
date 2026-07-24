//! Serializable views shared between the engine thread and the HTTP/WS API, plus
//! the command type the API sends to the engine.

use serde::{Deserialize, Serialize};

/// What the node persists (encrypted at rest via `core::vault`) so contacts and
/// chat history survive restarts (FR-9, FR-15).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedState {
    /// Contact directory as shareable invite codes (`b64url(cbor(IdentityPublic))`).
    pub contact_codes: Vec<String>,
    pub messages: Vec<MsgView>,
}

/// A command from the API (browser) to the engine thread.
#[derive(Debug, Clone)]
pub enum Command {
    /// Send a message to a known address.
    Send {
        to: String,
        body: String,
        priority: u8,
    },
    /// Add a contact from a shared identity code (`b64url(cbor(IdentityPublic))`).
    AddContact { code: String },
    /// One-tap SOS with optional GPS + battery (FR-40).
    Sos {
        lat: Option<f64>,
        lon: Option<f64>,
        acc_m: Option<u32>,
        battery_pct: Option<u8>,
        note: Option<String>,
    },
}

/// The full UI snapshot, serialized to the browser as JSON.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Snapshot {
    pub identity: IdentityView,
    pub directory: Vec<PeerView>,
    pub messages: Vec<MsgView>,
    pub status: StatusView,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct IdentityView {
    /// Base64url network address.
    pub address: String,
    pub name: String,
    /// Shareable identity code = `b64url(cbor(IdentityPublic))`.
    pub code: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeerView {
    pub address: String,
    pub name: String,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsgView {
    pub id: String,
    /// "in", "in-sos", or "out".
    pub dir: String,
    pub peer: String,
    pub peer_name: String,
    pub body: String,
    pub ts: u64,
    /// "sent" | "verified" | "received".
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct StatusView {
    pub relay_connected: bool,
    pub peer_count: usize,
    pub interfaces: Vec<String>,
    pub forwarded_copies: u64,
    pub store_len: usize,
    pub sent: usize,
    pub verified: usize,
    pub received: usize,
    // --- Diagnostics (FR-53) ---
    pub store_bytes: u64,
    pub duplicates: u64,
    pub dropped_expired: u64,
    pub dropped_nopostage: u64,
    pub custody_transfers: u64,
    pub known_gateways: usize,
    pub retries: u64,
}
