//! Wires the live [`lifeline_bridge::ws`] Nostr client into the running node as
//! an **extra bearer**, alongside the LAN/UDP transport and the `lifeline-relay`
//! client. Enabled with the `nostr` cargo feature and the `LIFELINE_NOSTR_RELAY`
//! env var (comma-separated relay URLs).
//!
//! The engine speaks to this bearer through an ordinary [`ChannelInterface`]
//! (std channels), exactly as it does the TCP relay. This module bridges those
//! std channels to the async `ws::run` tasks (one per relay, each with reconnect
//! + exponential backoff) running on the node's Tokio runtime:
//!
//! ```text
//!   engine ──ChannelInterface──▶ out_rx ─(std thread, fan-out)─▶ ws::run(relay_1)
//!                                                              └▶ ws::run(relay_2)
//!   engine ◀──ChannelInterface── in_tx  ◀─(tokio task)─── agg_in_rx ◀── all relays
//! ```
//!
//! The node's Nostr keypair is derived from the long-term identity via
//! [`lifeline_core::identity::Identity::derive_subkey`] — stable across restarts
//! (so our offline mailbox address persists) yet unlinkable from the public
//! Lifeline identity.

use lifeline_bridge::nostr::NostrIdentity;
use lifeline_bridge::ws::{self, ClientChannels, Exit};
use lifeline_transport::Outbound;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::sync::mpsc as tmpsc;

/// Domain tag for [`Identity::derive_subkey`] — keeps the Nostr key independent
/// of any other derived subkey.
const NOSTR_DOMAIN: &[u8] = b"nostr-bearer";

/// Spawn the Nostr bearer. `seed` is the node's derived Nostr secret;
/// `out_rx`/`in_tx`/`peers` are the engine-facing `ChannelInterface`'s far ends.
/// Runs on `handle` (the node's Tokio runtime).
pub fn spawn(
    handle: &Handle,
    relay_urls: Vec<String>,
    seed: [u8; 32],
    out_rx: Receiver<Outbound>,
    in_tx: Sender<(u64, Vec<u8>)>,
    peers: Arc<Mutex<Vec<u64>>>,
) {
    if relay_urls.is_empty() {
        return;
    }
    // Learned PeerId → nostr-pubkey directory, shared across every relay so a
    // peer discovered on one relay is addressable on the others.
    let ws_peers = Arc::new(Mutex::new(HashMap::new()));
    // Aggregated inbound from all relays → the engine's std inbound channel.
    let (agg_in_tx, mut agg_in_rx) = tmpsc::unbounded_channel::<(u64, Vec<u8>)>();

    // One reconnecting client per relay; collect their outbound senders.
    let mut out_senders = Vec::with_capacity(relay_urls.len());
    for url in relay_urls {
        let (o_tx, o_rx) = tmpsc::unbounded_channel::<(Option<u64>, Vec<u8>)>();
        out_senders.push(o_tx);
        let channels = ClientChannels {
            outbound: o_rx,
            inbound: agg_in_tx.clone(),
            peers: ws_peers.clone(),
        };
        handle.spawn(connect_loop(url, seed, channels));
    }
    drop(agg_in_tx); // clients hold their own clones; the loop below drives the last read side

    // Engine outbound → fan out to every relay client (the router already dedups
    // by bundle id, so multi-relay duplicates are harmless and improve delivery).
    std::thread::Builder::new()
        .name("lifeline-nostr-out".into())
        .spawn(move || {
            while let Ok(out) = out_rx.recv() {
                for s in &out_senders {
                    let _ = s.send((out.to, out.frame.clone()));
                }
            }
        })
        .expect("spawn nostr outbound thread");

    // Aggregated inbound → engine, learning peers for `ChannelInterface::scan`.
    handle.spawn(async move {
        while let Some((peer, frame)) = agg_in_rx.recv().await {
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
}

/// Derive the node's stable Nostr secret from its long-term identity.
pub fn seed_from_identity(identity: &lifeline_core::identity::Identity) -> [u8; 32] {
    identity.derive_subkey(NOSTR_DOMAIN)
}

/// One relay: connect, pump, and reconnect with exponential backoff until the
/// engine shuts the bearer down.
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
