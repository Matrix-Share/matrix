//! Wires the live [`lifeline_bridge::ws`] Nostr client into the running node as
//! an **extra bearer**, alongside the LAN/UDP transport and the `lifeline-relay`
//! client. Enabled with the `nostr` cargo feature and the `LIFELINE_NOSTR_RELAY`
//! env var (comma-separated relay URLs).
//!
//! The std↔tokio bridging is handled generically by
//! [`crate::async_bearer::spawn_async_bearer`]; this module supplies only the
//! Nostr-specific event loop: one reconnecting WebSocket client per relay
//! (exponential backoff), a shared `PeerId → pubkey` directory, and outbound
//! fan-out across relays. The engine sees an ordinary `ChannelInterface` and
//! never learns any of the bearer is asynchronous.
//!
//! The node's Nostr keypair is derived from the long-term identity via
//! [`lifeline_core::identity::Identity::derive_subkey`] — stable across restarts
//! (so our offline mailbox address persists) yet unlinkable from the public
//! Lifeline identity.

use crate::async_bearer::{spawn_async_bearer, BearerChannels};
use lifeline_bridge::nostr::NostrIdentity;
use lifeline_bridge::ws::{self, ClientChannels, Exit};
use lifeline_transport::{ChannelInterface, InterfaceCaps, PeerId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::sync::mpsc as tmpsc;

/// Domain tag for [`Identity::derive_subkey`] — keeps the Nostr key independent
/// of any other derived subkey.
const NOSTR_DOMAIN: &[u8] = b"nostr-bearer";

/// A frame becomes one event's base64 content; 16 KiB keeps events a size every
/// relay accepts. Larger bundles fragment upstream.
const NOSTR_MTU: usize = 16 * 1024;

/// Derive the node's stable Nostr secret from its long-term identity.
pub fn seed_from_identity(identity: &lifeline_core::identity::Identity) -> [u8; 32] {
    identity.derive_subkey(NOSTR_DOMAIN)
}

/// Spawn the Nostr bearer over `relay_urls` and return the engine-facing
/// interface. Runs on `handle` (the node's Tokio runtime).
pub fn spawn(handle: &Handle, relay_urls: Vec<String>, seed: [u8; 32]) -> ChannelInterface {
    spawn_async_bearer(
        handle,
        InterfaceCaps::overlay("nostr", NOSTR_MTU),
        move |channels| run_nostr(relay_urls, seed, channels),
    )
}

/// The Nostr bearer event loop: one reconnecting client per relay sharing a
/// learned-peer directory, with engine outbound fanned across all relays.
async fn run_nostr(relay_urls: Vec<String>, seed: [u8; 32], channels: BearerChannels) {
    let BearerChannels {
        mut outbound,
        inbound,
    } = channels;

    // Learned PeerId → nostr-pubkey directory, shared across every relay so a
    // peer discovered on one relay is addressable on the others.
    let ws_peers = Arc::new(Mutex::new(HashMap::new()));

    // One reconnecting client per relay; collect their outbound senders.
    let mut relay_out = Vec::with_capacity(relay_urls.len());
    for url in relay_urls {
        let (o_tx, o_rx) = tmpsc::unbounded_channel::<(Option<PeerId>, Vec<u8>)>();
        relay_out.push(o_tx);
        let ch = ClientChannels {
            outbound: o_rx,
            inbound: inbound.clone(),
            peers: ws_peers.clone(),
        };
        tokio::spawn(connect_loop(url, seed, ch));
    }

    // Engine outbound → every relay client. The router dedups by bundle id, so
    // multi-relay duplicates are harmless and improve delivery.
    while let Some(frame) = outbound.recv().await {
        for s in &relay_out {
            let _ = s.send(frame.clone());
        }
    }
}

/// One relay: connect, pump, and reconnect with exponential backoff until the
/// bearer shuts down.
async fn connect_loop(url: String, seed: [u8; 32], mut channels: ClientChannels) {
    let Some(id) = NostrIdentity::from_seed(&seed) else {
        tracing::error!("nostr: invalid derived seed; bearer disabled for {url}");
        return;
    };
    tracing::info!(
        "nostr: bearer enabled via {url} (pubkey {})",
        id.pubkey_hex()
    );
    let mut backoff = 1u64;
    loop {
        match ws::run(&url, &id, &mut channels).await {
            Ok(Exit::EngineGone) => {
                tracing::info!("nostr: engine shut down; closing {url}");
                return;
            }
            Ok(Exit::RelayClosed) => {
                tracing::warn!("nostr: relay {url} disconnected; reconnecting");
                backoff = 1; // we were connected — retry promptly
            }
            Err(e) => {
                tracing::debug!("nostr: connect to {url} failed: {e}");
            }
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}
