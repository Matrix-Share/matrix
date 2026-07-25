//! **External-network adapters** — Lifeline's connectivity to other networks.
//!
//! Each adapter implements [`lifeline_transport::ExternalNet`] and is wrapped in
//! `BridgeInterface` to become a first-class engine [`lifeline_transport::Interface`].
//! The engine treats it exactly like a radio, so adding connectivity to a new
//! network is *one trait implementation* — nothing in `core`, `router`, or the
//! `NodeEngine` changes.
//!
//! Provided here:
//! * [`nostr`] — a real Nostr adapter (secp256k1 Schnorr events, bundle↔event
//!   codec, relay-backed store-and-forward). Lets Lifeline nodes reach each other
//!   over the global, already-adopted Nostr relay network.
//! * [`skeleton`] — a documented, compiling template for the next network
//!   (Reticulum, Meshtastic, Matrix, a plain relay): copy it, fill in the TODOs.

pub mod nostr;
pub mod skeleton;

/// Lowercase-hex encode (Nostr's wire encoding for pubkeys/ids/sigs).
pub(crate) fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Decode lowercase/uppercase hex; `None` on bad input.
pub(crate) fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}
