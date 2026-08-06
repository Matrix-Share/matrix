//! Project Lifeline — the **node runtime** (PRD L1–L5, §12.1).
//!
//! [`NodeEngine`] is the orchestrator that composes the lower layers into a
//! running mesh node: it drives any number of [`lifeline_transport::Interface`]
//! bearers concurrently, seals/opens messages via `lifeline_core`, routes with
//! `lifeline_router`'s DTN store-carry-forward, and reconciles shared state with
//! `lifeline_sync`'s CRDTs. It carries the *same* opaque
//! [`lifeline_proto::Bundle`] over every bearer, fragmenting per-interface MTU.
//!
//! This lives in its own crate (separate from the lightweight
//! [`lifeline_transport`] *seam*) so that implementing a new bearer only needs
//! the `Interface`/`ExternalNet` contract, not the whole routing + CRDT + crypto
//! runtime. The engine depends downward on transport/router/sync/core; nothing
//! below depends on it.
//!
//! > **Roadmap:** `NodeEngine` is still a large single type spanning many
//! > concerns (groups, onion, custody, gateways, prekeys, content-fetch, ARQ,
//! > attribution…). Decomposing it into per-concern services driven by `tick()`
//! > is tracked in [`ARCHITECTURE.md`](https://github.com/matrix-share/matrix/blob/main/ARCHITECTURE.md).

pub mod engine;

pub use engine::{CustodyRole, EngineConfig, Inbound, NodeEngine};
