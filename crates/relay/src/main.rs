//! Project Lifeline — zero-knowledge relay hub.
//!
//! Accepts TCP connections from nodes and forwards opaque (end-to-end-encrypted)
//! frames between them. Everyone connected to the same relay is mutually "in
//! range" over their internet interface — this is the fabric that lets a single
//! gateway light up the mesh across the internet. The relay never decrypts,
//! stores, or forges anything.
//!
//! Config via env: `LIFELINE_RELAY_ADDR` (default `0.0.0.0:7000`).

use lifeline_relay::{read_msg, write_msg, Down, Up};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Mutex};

type Peers = Arc<Mutex<HashMap<u64, mpsc::Sender<Down>>>>;

/// Per-connection outbound queue depth. A consumer that reads slower than this
/// simply drops frames (backpressure) rather than letting the relay buffer
/// without bound — closes the slow-reader memory-DoS.
const QUEUE_BOUND: usize = 256;

/// Max simultaneous connections. Bounds total memory (each connection can pin up
/// to one `MAX_FRAME` read buffer) and refuses a connection-exhaustion flood.
const MAX_CONNS: u64 = 1024;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let addr = std::env::var("LIFELINE_RELAY_ADDR").unwrap_or_else(|_| "0.0.0.0:7000".into());
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("lifeline-relay listening on {addr} (zero-knowledge frame forwarder)");

    let peers: Peers = Arc::new(Mutex::new(HashMap::new()));
    let next_id = Arc::new(AtomicU64::new(1));
    let active = Arc::new(AtomicU64::new(0));

    loop {
        let (socket, remote) = listener.accept().await?;
        // Connection cap: refuse (and immediately drop) beyond the ceiling so a
        // flood of half-open connections can't exhaust memory/fds.
        if active.load(Ordering::Relaxed) >= MAX_CONNS {
            tracing::warn!("connection cap reached; refusing {remote}");
            drop(socket);
            continue;
        }
        active.fetch_add(1, Ordering::Relaxed);
        socket.set_nodelay(true).ok();
        let id = next_id.fetch_add(1, Ordering::Relaxed);
        let peers = peers.clone();
        let active = active.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(id, socket, peers.clone()).await {
                tracing::debug!("conn {id} ({remote}) ended: {e}");
            }
            // Cleanup + notify others of departure.
            peers.lock().await.remove(&id);
            broadcast(&peers, id, Down::Leave { id }).await;
            active.fetch_sub(1, Ordering::Relaxed);
            tracing::info!("peer {id} disconnected");
        });
    }
}

async fn handle_conn(
    id: u64,
    socket: tokio::net::TcpStream,
    peers: Peers,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut rd, mut wr) = socket.into_split();

    // Per-connection outbound queue — bounded, so a slow/idle reader drops frames
    // instead of ballooning the relay's memory.
    let (tx, mut rx) = mpsc::channel::<Down>(QUEUE_BOUND);

    // Register + greet with the current peer list; announce our arrival.
    let existing: Vec<u64> = {
        let mut guard = peers.lock().await;
        let existing = guard.keys().copied().collect();
        guard.insert(id, tx.clone());
        existing
    };
    tx.try_send(Down::Welcome {
        your_id: id,
        peers: existing,
    })
    .ok();
    broadcast(&peers, id, Down::Join { id }).await;
    tracing::info!("peer {id} connected");

    // Writer task: drain the outbound queue to the socket.
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if write_msg(&mut wr, &msg).await.is_err() {
                break;
            }
        }
        let _ = wr.shutdown().await;
    });

    // Reader loop: forward Send messages to their targets.
    while let Some(up) = read_msg::<_, Up>(&mut rd).await? {
        match up {
            Up::Send { to, data } => {
                let frame = Down::Frame { from: id, data };
                match to {
                    Some(target) => {
                        if let Some(peer) = peers.lock().await.get(&target) {
                            peer.try_send(frame).ok();
                        }
                    }
                    None => broadcast(&peers, id, frame).await,
                }
            }
        }
    }
    writer.abort();
    Ok(())
}

/// Send `msg` to every peer except `except`.
async fn broadcast(peers: &Peers, except: u64, msg: Down) {
    let guard = peers.lock().await;
    for (&pid, tx) in guard.iter() {
        if pid != except {
            // Non-blocking: a peer whose queue is full drops this frame rather
            // than stalling the broadcast (and holding the peers lock).
            tx.try_send(msg.clone()).ok();
        }
    }
}
