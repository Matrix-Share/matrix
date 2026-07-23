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

/// Current wire-format version. Bump on any breaking change to the structs
/// below; nodes advertise and check this to stay interoperable (NFR-9).
pub const WIRE_VERSION: u8 = 1;
