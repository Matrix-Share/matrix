//! Project Lifeline — wire protocol schemas and canonical encoding.
//!
//! This crate is the on-the-wire *contract* described in PRD §11 (data models)
//! and §12 (protocols). It deliberately contains **no cryptography and no I/O**:
//! it only defines the versioned data structures that every node must agree on,
//! plus their canonical CBOR encoding.
//!
//! Design rules baked in here:
//! * Canonical wire format is **CBOR** (compact); JSON representations use
//!   base64url for binary fields (PRD §11 preamble).
//! * Every top-level envelope carries an explicit version (`v`) so the wire
//!   format can evolve without ambiguity (NFR-9 interoperability).
//! * Binary fields are `Bytes` (see [`codec::Bytes`]) so they serialize as CBOR
//!   byte strings, not arrays of integers.

pub mod address;
pub mod codec;
pub mod pow;
pub mod types;

pub use address::Address;
pub use codec::{Bytes, CodecError};
pub use types::*;

/// Current wire-format version, stamped into every [`Bundle`]'s `v` field and
/// **checked at ingest** (see [`accepts_version`]).
///
/// Evolution policy (NFR-9 interoperability):
/// * **Additive, optional struct fields** do NOT bump this — CBOR encodes structs
///   as name-keyed maps and every optional field is `skip_serializing_if`, so
///   older nodes ignore unknown fields and newer nodes tolerate missing ones.
/// * **New enum values** ([`Priority`], [`PayloadKind`]) do NOT bump this — both
///   encode as integer discriminants and decode unknown values to a safe
///   fallback, so a new class rolls out gradually without partitioning the mesh.
/// * Only a **structurally incompatible** header change bumps this. Two nodes
///   whose versions differ reject each other's bundles at ingest rather than
///   mis-parsing them.
///
/// v2: `Priority`/`PayloadKind` moved from variant-name strings to
/// forward-compatible integer discriminants.
pub const WIRE_VERSION: u8 = 2;

/// Whether this node will accept a bundle stamped with wire version `v`.
///
/// Currently exact-match: the header layout is a single structural version, so a
/// mismatch means the peer may lay the header out differently and we must not
/// guess. (A future major/minor split would widen this to "same major".)
pub const fn accepts_version(v: u8) -> bool {
    v == WIRE_VERSION
}
