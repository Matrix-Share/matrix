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
//!   over the global, already-adopted Nostr relay network. A live WebSocket relay
//!   client (`ws` feature) hits real relays; see [`ws`].
//! * [`meshtastic`] (`meshtastic` feature) — a real Meshtastic adapter: genuine
//!   `ServiceEnvelope`/`MeshPacket` protobuf carried over MQTT, so Lifeline rides
//!   actual LoRa mesh hardware and public Meshtastic brokers. Live MQTT client
//!   behind the `mqtt` feature.
//! * [`skeleton`] — a documented, compiling template for the next network
//!   (Reticulum, Matrix, a plain relay): copy it, fill in the TODOs.

pub mod nostr;
pub mod skeleton;

/// Meshtastic adapter (real `ServiceEnvelope`/`MeshPacket` protobuf over MQTT).
/// Behind the `meshtastic` feature; the live MQTT transport needs `mqtt` too.
#[cfg(feature = "meshtastic")]
pub mod meshtastic;

/// Live WebSocket Nostr relay client (async). Behind the `ws` feature so the
/// core codec + adapters build without the async/TLS stack.
#[cfg(feature = "ws")]
pub mod ws;

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
