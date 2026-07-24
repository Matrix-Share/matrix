//! Payload compression for scarce-bandwidth bearers (gateway doc §4: a shared
//! gateway is "a straw, not a firehose" — make everything compressed).
//!
//! Compression happens **before encryption**: ciphertext is high-entropy and
//! effectively incompressible, so the only place a payload can be shrunk is on
//! the plaintext, inside the end-to-end envelope. The compressed form is sealed
//! and authenticated with the rest of the payload, so relays learn nothing and
//! cannot tamper with the framing.
//!
//! Framing is one leading marker byte:
//! * `0x00` — the bytes that follow are the raw payload.
//! * `0x01` — the bytes that follow are DEFLATE-compressed.
//!
//! We only keep the compressed form when it is actually smaller (short SOS texts
//! and already-compressed blobs stay raw), so compression never inflates a
//! message.

use crate::{CoreError, Result};
use miniz_oxide::deflate::compress_to_vec;
use miniz_oxide::inflate::decompress_to_vec;

const RAW: u8 = 0x00;
const DEFLATE: u8 = 0x01;

/// Below this size, compression's header overhead isn't worth it — stay raw.
const MIN_COMPRESS: usize = 64;

/// DEFLATE level (0..=10). 6 is a good speed/ratio balance for text-ish payloads.
const LEVEL: u8 = 6;

/// Frame `payload` for sealing: prepend a marker and compress if that is smaller.
pub fn frame(payload: &[u8]) -> Vec<u8> {
    if payload.len() >= MIN_COMPRESS {
        let compressed = compress_to_vec(payload, LEVEL);
        // +1 for each marker byte cancels out; compare the bodies directly.
        if compressed.len() < payload.len() {
            let mut out = Vec::with_capacity(compressed.len() + 1);
            out.push(DEFLATE);
            out.extend_from_slice(&compressed);
            return out;
        }
    }
    let mut out = Vec::with_capacity(payload.len() + 1);
    out.push(RAW);
    out.extend_from_slice(payload);
    out
}

/// Reverse [`frame`]: strip the marker and decompress if needed.
pub fn unframe(framed: &[u8]) -> Result<Vec<u8>> {
    let (&marker, body) = framed.split_first().ok_or(CoreError::Decrypt)?;
    match marker {
        RAW => Ok(body.to_vec()),
        DEFLATE => decompress_to_vec(body)
            .map_err(|_| CoreError::Log("payload decompression failed".into())),
        _ => Err(CoreError::Log("unknown payload framing marker".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compressible_payload_shrinks_and_roundtrips() {
        let text = "flood is receding, move to higher ground. ".repeat(50);
        let framed = frame(text.as_bytes());
        assert_eq!(framed[0], DEFLATE, "repetitive text should compress");
        assert!(framed.len() < text.len(), "compressed form is smaller");
        assert_eq!(unframe(&framed).unwrap(), text.as_bytes());
    }

    #[test]
    fn tiny_or_random_payload_stays_raw_and_roundtrips() {
        let tiny = b"SOS";
        let framed = frame(tiny);
        assert_eq!(framed[0], RAW);
        assert_eq!(unframe(&framed).unwrap(), tiny);

        // Framing must NEVER inflate a payload by more than the one marker byte,
        // whichever branch wins.
        let data: Vec<u8> = (0..4096u32)
            .map(|i| (i.wrapping_mul(2654435761) >> 19) as u8)
            .collect();
        let framed = frame(&data);
        assert!(framed.len() <= data.len() + 1, "framing never inflates");
        assert_eq!(unframe(&framed).unwrap(), data);
    }

    #[test]
    fn corrupt_framing_is_rejected() {
        assert!(unframe(&[]).is_err());
        assert!(unframe(&[0x09, 1, 2, 3]).is_err()); // unknown marker
        assert!(unframe(&[DEFLATE, 0xff, 0xff, 0xff]).is_err()); // bad deflate stream
    }
}
