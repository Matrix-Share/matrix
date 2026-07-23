//! Project Lifeline — L5 CRDT sync (PRD §8.6, §12.3, "web3 done right").
//!
//! Shared state that must survive partitions — group membership, blocklists,
//! presence, delivery status — is modelled as **Conflict-free Replicated Data
//! Types**. Every replica can mutate locally while offline; when links reappear
//! replicas exchange state and **merge deterministically to an identical
//! result**, with no coordinator and no consensus (FR-33, Shapiro 2011,
//! Kleppmann "local-first" 2019).
//!
//! Provided:
//! * [`VersionVector`] — per-actor causal context.
//! * [`OrSwot`] — an Observed-Remove Set (add-wins) without tombstones
//!   (Riak-style ORSWOT); used for group membership and blocklists.
//! * [`Lww`] — a Last-Writer-Wins register with deterministic tie-break; used
//!   for presence.
//! * [`SharedState`] — the aggregate a node syncs on contact.
//!
//! All merges are **commutative, associative, and idempotent**, so the order
//! and multiplicity of syncs never changes the outcome — which is exactly what
//! a delay-tolerant, partition-prone mesh needs.

mod lww;
mod orswot;
mod state;
mod vclock;

pub use lww::Lww;
pub use orswot::{Dot, OrSwot};
pub use state::{Presence, SharedState};
pub use vclock::VersionVector;
