//! Project Lifeline — network simulator (`/sim`, PRD §13.1).
//!
//! The simulator is how the PRD's delivery acceptance criteria are *proven*
//! without hardware (PRD §16 exit criteria, NFR-3): it builds a world of real
//! [`lifeline_core`] identities driving real [`lifeline_router`] DTN routers,
//! partitions them into clusters with no radio overlap, moves "data mule"
//! devices between clusters, optionally lights up internet gateways, and
//! measures end-to-end delivery **and** cryptographic delivery proof.
//!
//! Everything below the transport is the production code path — the simulator
//! only stands in for the physical L0 transports and mobility. A message that
//! "delivers" here exercises: seal → spray-and-wait → store-carry-forward →
//! (mule carry | gateway bridge) → decrypt → signed receipt → offline
//! verification.

pub mod bench;
pub mod containment;
pub mod mobility;
pub mod scenarios;
mod world;

pub use world::{DeliveryReport, Mule, RoutingStrategy, World};
