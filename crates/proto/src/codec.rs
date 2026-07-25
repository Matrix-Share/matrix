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

/// Hard ceiling on the size of a single CBOR document we will decode from an
/// untrusted peer. Bounds worst-case allocation regardless of the parser's
/// internals; matches the 16 MiB decompression ceiling in `core::compress`.
pub const MAX_WIRE_BYTES: usize = 16 * 1024 * 1024;

/// Maximum CBOR container-nesting depth we will decode. `ciborium` 0.2 has **no
/// recursion limit**, so deeply-nested input would recurse the deserializer into
/// a stack overflow (a remote crash). Our real structures nest ~4–5 deep; 128 is
/// generous headroom.
const MAX_CBOR_DEPTH: usize = 128;

/// Deserialize a wire struct from CBOR bytes.
///
/// Hardened for untrusted input: rejects oversized documents and structurally
/// bounds nesting depth (via [`guard_cbor`]) *before* handing bytes to
/// `ciborium`, so a hostile peer cannot drive an allocation bomb or a
/// stack-overflow through the decoder.
pub fn from_cbor<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, CodecError> {
    guard_cbor(bytes)?;
    ciborium::from_reader(bytes).map_err(|e| CodecError::Decode(e.to_string()))
}

/// Iterative structural pre-scan of a CBOR document: verifies it is well-framed
/// and bounds its nesting depth and size without recursing (so the guard itself
/// can never overflow). Does not validate types — that is `ciborium`'s job — only
/// that decoding it is safe to attempt.
fn guard_cbor(bytes: &[u8]) -> Result<(), CodecError> {
    if bytes.len() > MAX_WIRE_BYTES {
        return Err(CodecError::Decode(format!(
            "input too large: {} bytes (max {MAX_WIRE_BYTES})",
            bytes.len()
        )));
    }
    // Sentinel for an indefinite-length container (closed by a `break`, 0xFF).
    const INDEF: u64 = u64::MAX;
    let mut pos = 0usize;
    // Remaining data items expected at each open level; the document is one item.
    let mut stack: Vec<u64> = vec![1];

    while let Some(&top) = stack.last() {
        if top == 0 {
            stack.pop();
            continue;
        }
        let ib = *bytes
            .get(pos)
            .ok_or_else(|| CodecError::Decode("truncated CBOR".into()))?;
        pos += 1;

        // `break` (0xFF) closes the nearest indefinite-length container and is not
        // itself a data item.
        if ib == 0xFF {
            if top != INDEF {
                return Err(CodecError::Decode("unexpected CBOR break".into()));
            }
            stack.pop();
            continue;
        }
        // This byte begins a data item at the current level; account for it.
        if top != INDEF {
            *stack.last_mut().unwrap() -= 1;
        }

        let major = ib >> 5;
        let minor = ib & 0x1f;
        // Decode the argument (length/value); `None` = indefinite (minor 31).
        let arg: Option<u64> = match minor {
            0..=23 => Some(minor as u64),
            24 => Some(read_uint(bytes, &mut pos, 1)?),
            25 => Some(read_uint(bytes, &mut pos, 2)?),
            26 => Some(read_uint(bytes, &mut pos, 4)?),
            27 => Some(read_uint(bytes, &mut pos, 8)?),
            31 => None,
            _ => return Err(CodecError::Decode("reserved CBOR additional-info".into())),
        };

        match major {
            0 | 1 | 7 => {} // uint / nint / simple+float: leaf, no payload/children
            2 | 3 => match arg {
                // byte/text string: skip its content bytes …
                Some(len) => {
                    let len = len as usize;
                    pos = pos
                        .checked_add(len)
                        .filter(|p| *p <= bytes.len())
                        .ok_or_else(|| CodecError::Decode("CBOR string overruns input".into()))?;
                }
                // … or an indefinite string = chunks until `break`.
                None => push(&mut stack, INDEF)?,
            },
            4 => match arg {
                // array of `n` items.
                Some(n) => push(&mut stack, n)?,
                None => push(&mut stack, INDEF)?,
            },
            5 => match arg {
                // map of `n` pairs = 2·n items.
                Some(n) => push(&mut stack, n.saturating_mul(2))?,
                None => push(&mut stack, INDEF)?,
            },
            6 => push(&mut stack, 1)?, // tag: exactly one tagged item follows
            _ => unreachable!("major type is 3 bits"),
        }
    }
    Ok(())
}

/// Push a new container level, enforcing the depth bound.
fn push(stack: &mut Vec<u64>, remaining: u64) -> Result<(), CodecError> {
    if stack.len() >= MAX_CBOR_DEPTH {
        return Err(CodecError::Decode(format!(
            "CBOR nested deeper than {MAX_CBOR_DEPTH}"
        )));
    }
    stack.push(remaining);
    Ok(())
}

/// Read a big-endian unsigned integer of `n` bytes, advancing `pos`.
fn read_uint(bytes: &[u8], pos: &mut usize, n: usize) -> Result<u64, CodecError> {
    let end = pos
        .checked_add(n)
        .filter(|e| *e <= bytes.len())
        .ok_or_else(|| CodecError::Decode("truncated CBOR argument".into()))?;
    let mut v = 0u64;
    for &b in &bytes[*pos..end] {
        v = (v << 8) | b as u64;
    }
    *pos = end;
    Ok(v)
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
            // `deserialize_byte_buf` (owned) — NOT `deserialize_bytes` — because
            // ciborium's borrowed byte path is capped at its 4 KiB scratch buffer
            // and errors on larger byte strings. The owned path handles blobs of
            // any size (fragments carrying big attachments, ciphertext, etc.).
            d.deserialize_byte_buf(V)
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

    #[test]
    fn guard_accepts_normal_documents() {
        // Real, moderately-nested structures must pass the guard unchanged.
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct Nested {
            a: Vec<(u32, String)>,
            b: Option<Vec<Vec<u8>>>,
            c: Bytes,
        }
        let v = Nested {
            a: vec![(1, "x".into()), (2, "y".into())],
            b: Some(vec![vec![1, 2, 3], vec![]]),
            c: Bytes::new(vec![9; 5000]), // > ciborium's 4 KiB scratch, still fine
        };
        let enc = to_cbor(&v).unwrap();
        assert_eq!(from_cbor::<Nested>(&enc).unwrap(), v);
    }

    #[test]
    fn guard_rejects_deeply_nested_input() {
        // A hand-built tower of arrays deeper than MAX_CBOR_DEPTH: each 0x81 is
        // "array of 1 item". ciborium would recurse into a stack overflow; the
        // guard rejects it first, iteratively.
        let deep = vec![0x81u8; MAX_CBOR_DEPTH + 50];
        let err = from_cbor::<ciborium::value::Value>(&deep).unwrap_err();
        assert!(matches!(err, CodecError::Decode(m) if m.contains("nested deeper")));
    }

    #[test]
    fn guard_rejects_oversized_and_truncated_input() {
        // Oversized.
        let big = vec![0u8; MAX_WIRE_BYTES + 1];
        assert!(from_cbor::<Bytes>(&big).is_err());
        // Truncated: byte-string header claims 10 bytes, none follow.
        let truncated = vec![0x4a]; // major 2 (bytes), len 10
        assert!(from_cbor::<Bytes>(&truncated).is_err());
    }
}
