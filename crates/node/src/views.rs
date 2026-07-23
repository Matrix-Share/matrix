//! Serializable views shared between the engine thread and the HTTP/WS API, plus
//! the command type the API sends to the engine.

use serde::Serialize;

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

#[derive(Debug, Clone, Serialize)]
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
}
