//! Project Lifeline — L4 delay-tolerant router (PRD §8.5, §12).
//!
//! This crate is transport-agnostic: it never talks to a radio. It decides
//! **what to store, what to hand to a peer, and what to drop**, given a stream
//! of opportunistic contacts. A transport layer (BLE, Wi-Fi Aware, internet, or
//! the simulator) drives it by reporting contacts and moving bundles.
//!
//! Implemented DTN mechanisms:
//! * **Store-carry-forward** (FR-23): bundles are held with a TTL until a
//!   forwarding opportunity appears — including being physically carried by a
//!   moving device ("data mule", UC3).
//! * **Binary spray-and-wait** (FR-24, Spyropoulos 2005): a bounded copy budget
//!   `copies_left` is split on each contact, so total copies never exceed the
//!   configured budget — reliability of epidemic routing at a fraction of the
//!   overhead.
//! * **Custody transfer** (FR-25): a node emits a signed custody receipt when
//!   it accepts a bundle, so hand-offs are acknowledged, not silent.
//! * **TTL, hop limit, dedup** (FR-26): expired or over-hopped bundles are
//!   dropped; duplicates are suppressed by `bundle_id`.
//! * **Strict priority queueing** (FR-27, §12.5): SOS preempts everything at
//!   every relay.
//! * **Gateway gradient** (FR-29, §12.2): nodes cache signed gateway announces
//!   and forward gateway-bound traffic "downhill" toward the nearest gateway.
//! * **Store cap + priority/TTL-aware LRU eviction** (FR-52).
//!
//! Cryptography lives in `lifeline-core`; this crate treats `ciphertext` as
//! opaque and only reads routing header fields (FR-38: relays handle ciphertext
//! only).

mod attribution;
mod bounded;
mod gateway;
mod reputation;
mod router;
mod store;

pub use gateway::{GatewayCache, GATEWAY_ANNOUNCE_HOPS};
pub use reputation::{Reputation, DEMOTE_THRESHOLD};
pub use router::{DtnRouter, IngestOutcome, PeerInfo, RouterConfig, RouterStats};
pub use store::DEFAULT_STORE_CAP_BYTES;
