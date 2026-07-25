//! Project Lifeline — node daemon.
//!
//! Runs the mesh [`NodeEngine`] on a dedicated thread, connects to a
//! zero-knowledge relay for internet transport, and serves a web GUI + HTTP/WS
//! API so a person can chat over the mesh from a browser.
//!
//! Config via env:
//! * `LIFELINE_NODE_ADDR`  — GUI/API bind address (default `127.0.0.1:8080`, loopback).
//! * `LIFELINE_RELAY_ADDR` — relay to dial (default `127.0.0.1:7000`).
//! * `LIFELINE_NAME`       — display name (default derived from address).
//! * `LIFELINE_DATA_DIR`   — where the identity is persisted (default `./data`).
//! * `LIFELINE_PASSPHRASE` — passphrase wrapping the stored identity (dev default).
//! * `LIFELINE_NOSTR_RELAY`— comma-separated Nostr relay URLs to bridge onto as
//!   an extra bearer (requires the `nostr` build feature), e.g. `wss://relay.damus.io`.
//! * `LIFELINE_MESHTASTIC_MQTT` — Meshtastic MQTT broker `host[:port]` to bridge
//!   onto (requires the `meshtastic` build feature); see `build_meshtastic`.

mod api;
mod engine_thread;
#[cfg(feature = "nostr")]
mod nostr_bridge;
mod relay_client;
mod views;

use lifeline_core::{Identity, KeyBackup};
use lifeline_transport::{UdpInterface, DEFAULT_GROUP};
use std::net::SocketAddrV4;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Build an optional UDP LAN interface from env:
/// * `LIFELINE_UDP_PORT`   — enable UDP on this port (unset = disabled).
/// * `LIFELINE_UDP_NOMCAST`— if set, skip multicast (use seeds only).
/// * `LIFELINE_UDP_PEERS`  — comma-separated `ip:port` seed peers.
fn build_udp() -> Option<UdpInterface> {
    let port: u16 = std::env::var("LIFELINE_UDP_PORT").ok()?.parse().ok()?;
    let group = if std::env::var("LIFELINE_UDP_NOMCAST").is_ok() {
        None
    } else {
        Some(DEFAULT_GROUP)
    };
    let seeds: Vec<SocketAddrV4> = std::env::var("LIFELINE_UDP_PEERS")
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    match UdpInterface::bind(port, group, seeds) {
        Ok(u) => {
            tracing::info!(
                "UDP LAN interface on :{port} (multicast {})",
                group.is_some()
            );
            Some(u)
        }
        Err(e) => {
            tracing::warn!("UDP interface disabled: {e}");
            None
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Write `bytes` to `path` crash-safely: write a sibling temp file, then rename
/// it over `path`. `rename` is atomic on a POSIX filesystem, so a crash mid-write
/// leaves the previous good file intact instead of a truncated one — which the
/// load path would otherwise discard as "start fresh", losing all history.
pub(crate) fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// Await SIGTERM (unix) or Ctrl-C — the signal to begin graceful shutdown.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = term => {}
    }
}

/// Build the optional Nostr bearer from `LIFELINE_NOSTR_RELAY` (comma-separated
/// relay URLs). Returns the engine-facing `ChannelInterface` and spawns the
/// background client tasks; `None` if the env var is empty. `nostr` feature only.
#[cfg(feature = "nostr")]
fn build_nostr(
    identity: &Identity,
    handle: &tokio::runtime::Handle,
) -> Option<lifeline_transport::ChannelInterface> {
    use lifeline_transport::{ChannelInterface, InterfaceCaps};
    let urls: Vec<String> = std::env::var("LIFELINE_NOSTR_RELAY")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if urls.is_empty() {
        return None;
    }
    let (out_tx, out_rx) = std::sync::mpsc::channel();
    let (in_tx, in_rx) = std::sync::mpsc::channel();
    let peers = Arc::new(Mutex::new(Vec::<u64>::new()));
    let iface = ChannelInterface::new(
        InterfaceCaps::overlay("nostr", 16 * 1024),
        out_tx,
        in_rx,
        peers.clone(),
    );
    let seed = nostr_bridge::seed_from_identity(identity);
    nostr_bridge::spawn(handle, urls, seed, out_rx, in_tx, peers);
    Some(iface)
}

/// Build the optional Meshtastic bearer from `LIFELINE_MESHTASTIC_MQTT`
/// (`host[:port]`). Connects to the MQTT broker and wraps a `MeshtasticNet` as a
/// first-class engine interface. Because the MQTT client is synchronous, this
/// bearer needs no channel bridging — the engine tick drives it directly.
/// `meshtastic` feature only.
///
/// Also honored: `LIFELINE_MESHTASTIC_CHANNEL` (default `lifeline`) and
/// `LIFELINE_MESHTASTIC_ROOT` (topic root, default `msh/lifeline`).
#[cfg(feature = "meshtastic")]
fn build_meshtastic(identity: &Identity) -> Option<Box<dyn lifeline_transport::Interface + Send>> {
    use lifeline_bridge::meshtastic::{MeshtasticNet, MqttBus};
    use lifeline_transport::BridgeInterface;

    let broker = std::env::var("LIFELINE_MESHTASTIC_MQTT").ok()?;
    let (host, port) = match broker.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().unwrap_or(1883)),
        None => (broker, 1883),
    };
    let channel = env_or("LIFELINE_MESHTASTIC_CHANNEL", "lifeline");
    let root = env_or("LIFELINE_MESHTASTIC_ROOT", "msh/lifeline");

    // Node number derived from the identity: stable, nonzero, never the broadcast
    // address (top bit cleared, low bit set).
    let seed = identity.derive_subkey(b"meshtastic-node");
    let node = (u32::from_le_bytes([seed[0], seed[1], seed[2], seed[3]]) & 0x7fff_ffff) | 0x1;
    let client_id = format!("lifeline-{node:08x}");

    match MqttBus::connect(&host, port, &client_id, &root, &channel, node) {
        Ok(bus) => {
            tracing::info!(
                "meshtastic: bearer enabled via {host}:{port} (channel '{channel}', node !{node:08x})"
            );
            let net = MeshtasticNet::new(bus, node, channel);
            Some(Box::new(BridgeInterface::new(net)))
        }
        Err(e) => {
            tracing::warn!("meshtastic: MQTT connect to {host}:{port} failed: {e}");
            None
        }
    }
}

/// Load a persisted identity or generate + save a new one (FR-1, FR-5).
fn load_or_create_identity(data_dir: &str, passphrase: &str, name: Option<String>) -> Identity {
    let _ = std::fs::create_dir_all(data_dir);
    let path = std::path::Path::new(data_dir).join("identity.json");

    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(backup) = serde_json::from_slice::<KeyBackup>(&bytes) {
            match backup.restore(passphrase) {
                Ok(mut id) => {
                    if let Some(n) = name {
                        id.set_display_name(Some(n));
                    }
                    tracing::info!("loaded identity {}", id.address());
                    return id;
                }
                Err(e) => tracing::warn!("could not restore identity ({e}); generating a new one"),
            }
        }
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut id = Identity::generate(now);
    if let Some(n) = name {
        id.set_display_name(Some(n));
    }
    if let Ok(backup) = KeyBackup::create(&id, passphrase) {
        if let Ok(json) = serde_json::to_vec_pretty(&backup) {
            let _ = write_atomic(&path, &json);
        }
    }
    tracing::info!("generated identity {}", id.address());
    id
}

/// Unlock the encrypted state vault and restore contacts + history, or create a
/// fresh one (FR-9, FR-15).
fn load_or_create_state(
    data_dir: &str,
    passphrase: &str,
) -> (lifeline_core::vault::Vault, views::PersistedState) {
    use lifeline_core::vault::{SealedBlob, Vault};
    let path = std::path::Path::new(data_dir).join("state.vault");
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(blob) = serde_json::from_slice::<SealedBlob>(&bytes) {
            match Vault::unlock(passphrase, &blob) {
                Ok((vault, pt)) => {
                    let state =
                        serde_json::from_slice::<views::PersistedState>(&pt).unwrap_or_default();
                    tracing::info!(
                        "restored {} contacts, {} messages",
                        state.contact_codes.len(),
                        state.messages.len()
                    );
                    return (vault, state);
                }
                Err(e) => tracing::warn!("could not unlock vault ({e}); starting fresh"),
            }
        }
    }
    let vault = Vault::create(passphrase).expect("derive vault key");
    (vault, views::PersistedState::default())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    // Bind the local API to loopback by default: every route is unauthenticated,
    // so exposing it on 0.0.0.0 would let any device on the network read the
    // user's history and send messages / trigger SOS as them. Operators who
    // deliberately expose it (e.g. behind their own auth proxy) must opt in.
    let node_addr = env_or("LIFELINE_NODE_ADDR", "127.0.0.1:8080");
    let relay_addr = env_or("LIFELINE_RELAY_ADDR", "127.0.0.1:7000");
    let data_dir = env_or("LIFELINE_DATA_DIR", "./data");
    let passphrase = env_or("LIFELINE_PASSPHRASE", "lifeline-dev");
    if passphrase == "lifeline-dev" {
        tracing::warn!(
            "LIFELINE_PASSPHRASE is unset — using the INSECURE default 'lifeline-dev'. \
             Data at rest (identity + message history) is NOT protected. Set \
             LIFELINE_PASSPHRASE to a strong secret in production."
        );
    }
    let name = std::env::var("LIFELINE_NAME").ok();

    let identity = load_or_create_identity(&data_dir, &passphrase, name.clone());
    let display_name = identity
        .public()
        .display_name
        .unwrap_or_else(|| identity.address().short());

    // Unlock (or create) the encrypted vault and restore contacts + history.
    let (vault, initial) = load_or_create_state(&data_dir, &passphrase);

    // Channels wiring: engine <-> relay client, and API -> engine.
    let (out_tx, out_rx) = std::sync::mpsc::channel();
    let (in_tx, in_rx) = std::sync::mpsc::channel();
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();

    let peers = Arc::new(Mutex::new(Vec::<u64>::new()));
    let connected = Arc::new(AtomicBool::new(false));
    let shared = Arc::new(Mutex::new(views::Snapshot::default()));
    let version = Arc::new(AtomicU64::new(0));

    // Relay client (blocking threads).
    relay_client::spawn(
        relay_addr.clone(),
        out_rx,
        in_tx,
        peers.clone(),
        connected.clone(),
    );

    // Optional infrastructureless LAN transport.
    let udp = build_udp();

    // Optional extra bearers, built here (they need the identity before it moves
    // into the engine thread, and the Tokio runtime handle). Each becomes a
    // first-class engine interface via the `ExternalNet`/`Interface` seam.
    #[allow(unused_mut)]
    let mut extra_ifaces: Vec<Box<dyn lifeline_transport::Interface + Send>> = Vec::new();
    #[cfg(feature = "nostr")]
    if let Some(iface) = build_nostr(&identity, &tokio::runtime::Handle::current()) {
        extra_ifaces.push(Box::new(iface));
    }
    #[cfg(feature = "meshtastic")]
    if let Some(iface) = build_meshtastic(&identity) {
        extra_ifaces.push(iface);
    }

    // A handle to signal the engine to flush + stop at shutdown.
    let shutdown_tx = cmd_tx.clone();

    // Engine thread (kept, so we can join it on shutdown for a final flush).
    let engine_handle = {
        let shared = shared.clone();
        let version = version.clone();
        std::thread::Builder::new()
            .name("lifeline-engine".into())
            .spawn(move || {
                engine_thread::run(
                    identity,
                    display_name,
                    cmd_rx,
                    out_tx,
                    in_rx,
                    peers,
                    connected,
                    shared,
                    version,
                    udp,
                    vault,
                    data_dir,
                    initial,
                    extra_ifaces,
                );
            })?
    };

    // Web GUI + API.
    let app = api::router(api::AppState {
        cmd: cmd_tx,
        shared,
        version,
    });
    let listener = tokio::net::TcpListener::bind(&node_addr).await?;
    tracing::info!("lifeline-node GUI on http://{node_addr}  (relay {relay_addr})");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // Signal received: tell the engine to flush persistent state and exit, then
    // wait for it so a rotated prekey ring / recent messages aren't lost.
    tracing::info!("shutting down — flushing state");
    let _ = shutdown_tx.send(views::Command::Shutdown);
    let _ = engine_handle.join();
    Ok(())
}
