//! Project Lifeline — node daemon.
//!
//! Runs the mesh [`NodeEngine`] on a dedicated thread, connects to a
//! zero-knowledge relay for internet transport, and serves a web GUI + HTTP/WS
//! API so a person can chat over the mesh from a browser.
//!
//! Config via env:
//! * `LIFELINE_NODE_ADDR`  — GUI/API bind address (default `0.0.0.0:8080`).
//! * `LIFELINE_RELAY_ADDR` — relay to dial (default `127.0.0.1:7000`).
//! * `LIFELINE_NAME`       — display name (default derived from address).
//! * `LIFELINE_DATA_DIR`   — where the identity is persisted (default `./data`).
//! * `LIFELINE_PASSPHRASE` — passphrase wrapping the stored identity (dev default).

mod api;
mod engine_thread;
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
            let _ = std::fs::write(&path, json);
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

    let node_addr = env_or("LIFELINE_NODE_ADDR", "0.0.0.0:8080");
    let relay_addr = env_or("LIFELINE_RELAY_ADDR", "127.0.0.1:7000");
    let data_dir = env_or("LIFELINE_DATA_DIR", "./data");
    let passphrase = env_or("LIFELINE_PASSPHRASE", "lifeline-dev");
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

    // Engine thread.
    {
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
                );
            })?;
    }

    // Web GUI + API.
    let app = api::router(api::AppState {
        cmd: cmd_tx,
        shared,
        version,
    });
    let listener = tokio::net::TcpListener::bind(&node_addr).await?;
    tracing::info!("lifeline-node GUI on http://{node_addr}  (relay {relay_addr})");
    axum::serve(listener, app).await?;
    Ok(())
}
