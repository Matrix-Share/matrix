//! Canonical CBOR encoding + base64url helpers, and a [`Bytes`] newtype.
//!
//! PRD §11: "canonical wire format is CBOR (compact)"; "all binary fields
//! base64url in JSON representations". We therefore:
//! * encode/decode structs to CBOR for the wire, and
//! * expose base64url helpers for the human/JSON representation and for
//!   deriving text addresses/fingerprints.

use base64::Engine;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fmt;

/// Errors from encoding/decoding wire structures.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("cbor encode: {0}")]
    Encode(String),
    #[error("cbor decode: {0}")]
    Decode(String),
    #[error("base64url decode: {0}")]
    Base64(#[from] base64::DecodeError),
}

/// The base64url engine used everywhere (URL-safe alphabet, **no padding**) so
/// values are safe in QR codes, filenames and URLs (PRD §11, FR-3).
pub const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Encode bytes as base64url (no padding).
pub fn b64url_encode(bytes: &[u8]) -> String {
    B64.encode(bytes)
}

/// Decode a base64url (no padding) string.
pub fn b64url_decode(s: &str) -> Result<Vec<u8>, CodecError> {
    Ok(B64.decode(s.as_bytes())?)
}

/// Serialize any wire struct to canonical CBOR bytes.
pub fn to_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>, CodecError> {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf).map_err(|e| CodecError::Encode(e.to_string()))?;
    Ok(buf)
}

/// Deserialize a wire struct from CBOR bytes.
pub fn from_cbor<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, CodecError> {
    ciborium::from_reader(bytes).map_err(|e| CodecError::Decode(e.to_string()))
}

/// A length-delimited binary blob that serializes as a CBOR byte string (and as
/// a base64url string in human/JSON form), matching PRD §11's representation of
/// every `b64url(...)` field.
///
/// Using this newtype (instead of `Vec<u8>`) is important: with `serde_bytes`
/// semantics the value becomes a compact CBOR *byte string* rather than an
/// array of integers, which both saves space and matches the spec.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Bytes(pub Vec<u8>);

impl Bytes {
    pub fn new(v: impl Into<Vec<u8>>) -> Self {
        Bytes(v.into())
    }
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
    pub fn to_b64url(&self) -> String {
        b64url_encode(&self.0)
    }
    pub fn from_b64url(s: &str) -> Result<Self, CodecError> {
        Ok(Bytes(b64url_decode(s)?))
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(v: Vec<u8>) -> Self {
        Bytes(v)
    }
}
impl From<&[u8]> for Bytes {
    fn from(v: &[u8]) -> Self {
        Bytes(v.to_vec())
    }
}
impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for Bytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Debug as a short base64url preview; full blobs are noisy in logs.
        let s = self.to_b64url();
        if s.len() <= 16 {
            write!(f, "Bytes({s})")
        } else {
            write!(f, "Bytes({}…, {}B)", &s[..16], self.0.len())
        }
    }
}

impl Serialize for Bytes {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // CBOR byte string on the wire; base64url string for human formats.
        if s.is_human_readable() {
            s.serialize_str(&self.to_b64url())
        } else {
            s.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Bytes;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("byte string or base64url text")
            }
            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Bytes, E> {
                Ok(Bytes(v.to_vec()))
            }
            fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Bytes, E> {
                Ok(Bytes(v))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Bytes, E> {
                Bytes::from_b64url(v).map_err(serde::de::Error::custom)
            }
            // ciborium hands byte strings to the seq visitor in some paths.
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Bytes, A::Error> {
                let mut out = Vec::new();
                while let Some(b) = seq.next_element::<u8>()? {
                    out.push(b);
                }
                Ok(Bytes(out))
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
    fn bytes_cbor_roundtrip() {
        let b = Bytes::new(vec![0u8, 1, 2, 250, 255]);
        let enc = to_cbor(&b).unwrap();
        let dec: Bytes = from_cbor(&enc).unwrap();
        assert_eq!(b, dec);
    }

    #[test]
    fn b64url_roundtrip() {
        let data = b"lifeline\x00\xff";
        let s = b64url_encode(data);
        assert!(!s.contains('=')); // no padding
        assert_eq!(b64url_decode(&s).unwrap(), data);
    }
}
