//! Network addresses.
//!
//! PRD FR-2 / §11.1: "the user's network **address is derived from the public
//! key**". An [`Address`] is the base64url text of a 16-byte hash truncation of
//! the identity's signing public key. It is the routing destination (`dst`),
//! the log author id, and the contact key — there is no separate registrar
//! (self-sovereign identity, PRD G3).
//!
//! This crate does not hash keys itself (no crypto here); the `core` crate
//! constructs an `Address` via [`Address::from_hash_bytes`] using an audited
//! hash. This module only defines the *shape* and text encoding of an address.

use crate::codec::{b64url_decode, b64url_encode, CodecError};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Length in bytes of the truncated public-key hash that forms an address.
/// 16 bytes / 128 bits is ample against collision for a mesh's node count while
/// keeping addresses short enough to eyeball as a fingerprint (FR-3).
pub const ADDRESS_LEN: usize = 16;

/// A node/identity network address: `b64url(hash(pubkey)[..16])`.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Address([u8; ADDRESS_LEN]);

impl Address {
    /// Build an address from raw 16-byte hash output.
    pub fn from_hash_bytes(bytes: [u8; ADDRESS_LEN]) -> Self {
        Address(bytes)
    }

    /// Build an address from a hash slice, truncating/validating to 16 bytes.
    pub fn from_hash_slice(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < ADDRESS_LEN {
            return None;
        }
        let mut a = [0u8; ADDRESS_LEN];
        a.copy_from_slice(&bytes[..ADDRESS_LEN]);
        Some(Address(a))
    }

    pub fn as_bytes(&self) -> &[u8; ADDRESS_LEN] {
        &self.0
    }

    /// Canonical text form used on the wire's JSON view and in UIs.
    pub fn to_text(&self) -> String {
        b64url_encode(&self.0)
    }

    /// Parse from canonical text form.
    pub fn from_text(s: &str) -> Result<Self, CodecError> {
        let raw = b64url_decode(s)?;
        Self::from_hash_slice(&raw)
            .ok_or_else(|| CodecError::Decode(format!("address wrong length: {}", raw.len())))
    }

    /// Short human fingerprint (first 4 base64url chars, e.g. for compact UI).
    pub fn short(&self) -> String {
        let t = self.to_text();
        t.chars().take(6).collect()
    }

    /// XOR distance to another address — the Kademlia metric used by the L3
    /// discovery layer and the gateway gradient (PRD §12.2, Maymounkov 2002).
    pub fn xor_distance(&self, other: &Address) -> [u8; ADDRESS_LEN] {
        let mut d = [0u8; ADDRESS_LEN];
        for ((slot, a), b) in d.iter_mut().zip(&self.0).zip(&other.0) {
            *slot = a ^ b;
        }
        d
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_text())
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address({})", self.short())
    }
}

impl Serialize for Address {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            s.serialize_str(&self.to_text())
        } else {
            s.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Address;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("16-byte address or base64url text")
            }
            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Address, E> {
                Address::from_hash_slice(v)
                    .ok_or_else(|| E::custom(format!("address wrong length: {}", v.len())))
            }
            fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Address, E> {
                self.visit_bytes(&v)
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Address, E> {
                Address::from_text(v).map_err(E::custom)
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Address, A::Error> {
                let mut out = Vec::new();
                while let Some(b) = seq.next_element::<u8>()? {
                    out.push(b);
                }
                Address::from_hash_slice(&out)
                    .ok_or_else(|| serde::de::Error::custom("address wrong length"))
            }
        }
        if d.is_human_readable() {
            d.deserialize_str(V)
        } else {
            d.deserialize_bytes(V)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_roundtrip() {
        let a = Address::from_hash_bytes([7u8; ADDRESS_LEN]);
        let t = a.to_text();
        assert_eq!(Address::from_text(&t).unwrap(), a);
    }

    #[test]
    fn xor_distance_self_is_zero() {
        let a = Address::from_hash_bytes([9u8; ADDRESS_LEN]);
        assert_eq!(a.xor_distance(&a), [0u8; ADDRESS_LEN]);
    }
}
