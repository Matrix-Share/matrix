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
    /// CBOR-encoded forward-secret prekey ring (`core::prekey::PrekeyRingState`),
    /// so in-flight prekey-sealed messages survive a restart (FR-44). Opaque here;
    /// the whole `PersistedState` is encrypted at rest by `core::vault`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prekeys: Option<lifeline_proto::Bytes>,
    /// Group ids this node participates in (FR-12), so group threads survive a
    /// restart. Membership itself is rebuilt from the engine's group state.
    #[serde(default)]
    pub groups: Vec<String>,
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
    /// Send a message via onion routing over auto-selected relays (FR-49): hides
    /// who is talking to whom. Receipt-less by design.
    SendPrivate { to: String, body: String },
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
    /// Broadcast a text message to every contact (ask the mesh to propagate it).
    Broadcast { body: String },
    /// Broadcast "I'm safe" to all contacts (FR-41).
    Safe { note: Option<String> },
    /// Share location with a specific contact (FR-43).
    Location {
        to: String,
        lat: f64,
        lon: f64,
        acc_m: Option<u32>,
    },
    /// Share this node's location with *every* contact at once — the
    /// "find each other in a crowd" broadcast (FR-43). Also sets this node's own
    /// position so it can render distances/bearings to contacts who reply.
    LocationAll {
        lat: f64,
        lon: f64,
        acc_m: Option<u32>,
    },
    /// Share this node's location with only the members of one group — a scoped
    /// share ("share with this group only") for location privacy. Also sets this
    /// node's own position so distances render.
    LocationGroup {
        group: String,
        lat: f64,
        lon: f64,
        acc_m: Option<u32>,
    },
    /// Add a **point of interest** (wayfinding) at a location — water, a stage,
    /// medical, "our tent". Stored locally and, if `share`, broadcast to every
    /// contact so the whole crew can navigate to it.
    AddPoi {
        name: String,
        category: String,
        lat: f64,
        lon: f64,
        share: bool,
    },
    /// Raise a **strobe beacon**: ask the crew's phones to pulse a synchronized
    /// glow for `seconds` at `bpm` (clamped seizure-safe), so people can spot
    /// each other in a crowd. Broadcast to every contact; also armed locally.
    Strobe { bpm: u16, seconds: u16 },
    /// Flush persistent state and stop the engine loop (graceful shutdown). Sent
    /// by `main` when the process receives SIGTERM/Ctrl-C.
    Shutdown,
    /// Panic / duress wipe (G3): securely erase the node's on-disk secrets
    /// (`identity.json`, `state.vault`) and stop the engine **without** flushing,
    /// so returning drops the live engine + identity and their `zeroize`-on-drop
    /// scrubs all in-memory secrets. One decisive, irreversible action for a
    /// coerced or seized device. There is no confirmation and no undo.
    Panic,
    /// Create a group (sender-keys, FR-12) with a chosen id/name.
    CreateGroup { id: String },
    /// Add a known contact (by address) to a group and distribute our sender key.
    AddGroupMember { group: String, addr: String },
    /// Send a message to every member of a group.
    SendGroup { group: String, body: String },
    /// Block a contact (endpoint moderation, FR-48): silently drop their messages.
    Block { addr: String },
    /// Unblock a previously blocked contact.
    Unblock { addr: String },
    /// Set this node's current GPS position, required for geocast *receive* (a
    /// node only accepts a geocast whose region cell matches its own position).
    SetPosition { lat: f64, lon: f64 },
    /// Geocast: broadcast a message to everyone within `radius_m` of a point,
    /// addressed by region (geohash) rather than identity (FR — area alerts).
    Geocast {
        lat: f64,
        lon: f64,
        radius_m: f64,
        body: String,
    },
    /// Join a **place channel** by geohash cell (join-by-place): receive messages
    /// posted to that cell without the posters being contacts.
    JoinPlace { geohash: String },
    /// Leave a place channel.
    LeavePlace { geohash: String },
    /// Post a message to a joined place channel.
    SendPlace { geohash: String, body: String },
}

/// The full UI snapshot, serialized to the browser as JSON.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Snapshot {
    pub identity: IdentityView,
    pub directory: Vec<PeerView>,
    pub messages: Vec<MsgView>,
    pub groups: Vec<GroupView>,
    pub status: StatusView,
    /// Contacts who have shared a live location, nearest-first — the data behind
    /// the "Nearby / find each other" view. Empty until someone shares.
    #[serde(default)]
    pub nearby: Vec<NearbyView>,
    /// This node's own last-known position, if set (from GPS). Present enables
    /// the Nearby view to show distance + direction to each contact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub my_pos: Option<PosView>,
    /// Shared points of interest (water, stages, medical, "our tent"), each with
    /// distance + direction from this node — the wayfinding view. Nearest-first.
    #[serde(default)]
    pub pois: Vec<PoiView>,
    /// An active strobe beacon, if one is running — the crew's phones pulse a
    /// synchronized glow. Present only while `start .. start+seconds` is current.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strobe: Option<StrobeView>,
    /// Geohash **place channels** this node has joined (join-by-place). Messages on
    /// them thread under `place:<geohash>` in `messages`.
    #[serde(default)]
    pub places: Vec<String>,
}

/// An active strobe beacon: a synchronized crowd-finding glow. Every phone
/// computes the same brightness from `(now - start) * bpm` against its wall
/// clock, so the pulses line up without a server.
#[derive(Debug, Clone, Serialize)]
pub struct StrobeView {
    /// Unix seconds the strobe started (the shared phase origin).
    pub start: u64,
    /// Pulses per minute (clamped seizure-safe, ≤ 180 = 3 Hz).
    pub bpm: u16,
    /// How long the strobe runs, in seconds.
    pub seconds: u16,
    /// Display name of who raised it (`"you"` if this node did).
    pub from: String,
}

/// A point of interest to navigate to, annotated like [`NearbyView`] with
/// distance + direction from this node.
#[derive(Debug, Clone, Serialize)]
pub struct PoiView {
    pub id: String,
    pub name: String,
    /// Short category slug: `water`, `food`, `medical`, `stage`, `toilet`,
    /// `tent`, `car`, `other`.
    pub category: String,
    pub lat: f64,
    pub lon: f64,
    /// Unix seconds this POI was added/received.
    pub at: u64,
    /// Who shared it: `"me"` for a POI this node added, else the sharer's name.
    pub from: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_m: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearing_deg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compass: Option<String>,
}

/// This node's own position (lat/lon + when it was set).
#[derive(Debug, Clone, Serialize)]
pub struct PosView {
    pub lat: f64,
    pub lon: f64,
    /// Unix seconds when this fix was recorded.
    pub at: u64,
}

/// A contact's last-shared location, with distance + direction from *this* node.
/// Distance/bearing are `None` until we have our own position to measure from.
#[derive(Debug, Clone, Serialize)]
pub struct NearbyView {
    pub address: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    /// Unix seconds when this contact's location was last received.
    pub at: u64,
    /// Straight-line metres from this node; `None` if our own position is unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_m: Option<f64>,
    /// Compass bearing to the contact in degrees (0 = N); `None` if unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearing_deg: Option<f64>,
    /// 8-point compass label ("N", "NE", …) to the contact; `None` if unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compass: Option<String>,
}

/// A group the node participates in (FR-12), with its current members.
#[derive(Debug, Clone, Serialize)]
pub struct GroupView {
    pub id: String,
    pub members: Vec<GroupMemberView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupMemberView {
    pub address: String,
    pub name: String,
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
    /// True if this contact is blocked (their messages are dropped, FR-48).
    pub blocked: bool,
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
    /// Lossy-link ARQ frame retransmissions (FR — Dhwani noise robustness).
    pub arq_retransmits: u64,
    /// Whether this node is operating as a gateway (FR-35).
    pub is_gateway: bool,
    /// Hops to the nearest known gateway (FR-36); `None` if none known.
    pub gradient: Option<u16>,
}
