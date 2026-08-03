//! Project Lifeline — L0/L1 transport abstraction (PRD §8.4; spectrum doc §3
//! "bind to no single transport"; network-layer L1).
//!
//! > *"Build a transport-abstraction protocol where a message is independent of
//! > how it travels — so the same encrypted packet can hop over BLE now, sound
//! > across a noisy room next, Wi-Fi Aware to the next cluster, and out through
//! > a LoRa or internet gateway when one appears."* — spectrum design doc
//!
//! This crate is what makes Lifeline **completely modular across network
//! types**. Every physical channel — BLE, Wi-Fi Aware, ultrasound, optical,
//! LoRa, internet — is a plug-in implementing one small [`Interface`] contract.
//! Because interfaces have wildly different MTUs (ultrasound tens of bytes, LoRa
//! ~222 B, internet ~64 KB), this crate also carries the **fragmentation and
//! reassembly** ([`frame`]) and **lossy-link ARQ** ([`arq`]) that let one message
//! physically cross any medium.
//!
//! It is a **lightweight seam**: it depends only on [`lifeline_proto`] (+
//! `lifeline_core` for its error type), *not* on the router, CRDTs, or the node
//! runtime — so implementing a new bearer needs the `Interface`/`ExternalNet`
//! contract and nothing else. The runtime that *drives* interfaces, the
//! `NodeEngine`, lives in the separate `lifeline-engine` crate.
//!
//! The concrete interfaces here run over an in-process [`SharedMedium`] so the
//! whole stack is testable without radios; a real BLE/LoRa/internet backend is
//! a drop-in that implements [`Interface`] with the same [`InterfaceCaps`].

pub mod arq;
pub mod ble;
pub mod bridge;
pub mod caps;
pub mod channel;
pub mod frame;
pub mod interface;
pub mod medium;
pub mod udp;

pub use arq::{ArqRx, ArqTx};
pub use bridge::{peer_id_from_identity, BridgeInterface, ExternalNet};
pub use caps::{InterfaceCaps, InterfaceKind, RangeClass, ThroughputClass};
pub use channel::{ChannelInterface, Outbound};
pub use frame::{Fragmenter, Frame, FrameKind, Reassembler};
pub use interface::{Interface, PeerId};
pub use medium::{MemoryInterface, SharedMedium};
pub use udp::{UdpInterface, DEFAULT_GROUP};

/// Errors from the transport layer.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("frame exceeds interface MTU ({frame} > {mtu})")]
    Mtu { frame: usize, mtu: usize },
    #[error("interface MTU {0} too small to carry any payload")]
    MtuTooSmall(usize),
    #[error("codec: {0}")]
    Codec(#[from] lifeline_proto::CodecError),
    #[error("core: {0}")]
    Core(#[from] lifeline_core::CoreError),
}

pub type Result<T> = std::result::Result<T, TransportError>;
