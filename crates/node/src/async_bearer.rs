//! Reusable glue for wiring an **asynchronous** bearer (one whose I/O lives on
//! the Tokio runtime — a WebSocket relay, a Matrix client, libp2p…) into the
//! synchronous engine.
//!
//! The engine only ever speaks to a bearer through a [`ChannelInterface`] (std
//! channels). A synchronous bearer (Meshtastic-MQTT) implements `ExternalNet`
//! and needs none of this. An async one has to bridge the std↔tokio boundary,
//! and every async bearer needs the *same* bridge: fan the engine's outbound
//! frames onto a Tokio channel, drain the bearer's inbound frames back to the
//! engine, and project learned peers into the interface's peer list.
//!
//! [`spawn_async_bearer`] encapsulates exactly that. A bearer author writes only
//! its event loop (`run`) against [`BearerChannels`]; the boilerplate lives here
//! once. `node::nostr_bridge` is the first user.

use lifeline_transport::{ChannelInterface, InterfaceCaps, Outbound, PeerId};
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;
use tokio::sync::mpsc as tmpsc;

/// The Tokio-side channels a running async bearer speaks to the engine over.
pub struct BearerChannels {
    /// Frames the engine wants sent: `None` = broadcast, `Some(peer)` = directed.
    pub outbound: tmpsc::UnboundedReceiver<(Option<PeerId>, Vec<u8>)>,
    /// Frames received from the network, delivered to the engine. Clone it to
    /// fan multiple connections into one inbound stream.
    pub inbound: tmpsc::UnboundedSender<(PeerId, Vec<u8>)>,
}

/// Spawn an async bearer on `handle` and return the engine-facing
/// [`ChannelInterface`] to `add_interface` onto the engine.
///
/// `run` is the bearer's whole event loop; it receives [`BearerChannels`] and
/// runs until the engine drops the interface (its `outbound` closes) or the
/// bearer decides to stop. Everything else — the std↔tokio forwarding threads
/// and the peer projection the engine's `scan()` reads — is handled here.
pub fn spawn_async_bearer<F, Fut>(handle: &Handle, caps: InterfaceCaps, run: F) -> ChannelInterface
where
    F: FnOnce(BearerChannels) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    // Engine-facing std channels + the interface it drives.
    let (out_tx, out_rx) = std::sync::mpsc::channel::<Outbound>();
    let (in_tx, in_rx) = std::sync::mpsc::channel::<(PeerId, Vec<u8>)>();
    let peers = Arc::new(Mutex::new(Vec::<PeerId>::new()));
    let iface = ChannelInterface::new(caps, out_tx, in_rx, peers.clone());

    // Bearer-facing tokio channels.
    let (async_out_tx, async_out_rx) = tmpsc::unbounded_channel::<(Option<PeerId>, Vec<u8>)>();
    let (async_in_tx, mut async_in_rx) = tmpsc::unbounded_channel::<(PeerId, Vec<u8>)>();

    // Engine outbound (std) → bearer (tokio). A blocking recv on its own thread.
    std::thread::Builder::new()
        .name("lifeline-bearer-out".into())
        .spawn(move || {
            while let Ok(out) = out_rx.recv() {
                if async_out_tx.send((out.to, out.frame)).is_err() {
                    break; // bearer gone
                }
            }
        })
        .expect("spawn async-bearer outbound thread");

    // Bearer inbound (tokio) → engine (std), learning peers for `scan()`.
    handle.spawn(async move {
        while let Some((peer, frame)) = async_in_rx.recv().await {
            {
                let mut p = peers.lock().unwrap();
                if !p.contains(&peer) {
                    p.push(peer);
                }
            }
            if in_tx.send((peer, frame)).is_err() {
                break; // engine gone
            }
        }
    });

    // The bearer's own event loop.
    handle.spawn(run(BearerChannels {
        outbound: async_out_rx,
        inbound: async_in_tx,
    }));

    iface
}
