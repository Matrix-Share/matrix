//! Blocking relay client: bridges the engine's [`ChannelInterface`] to a TCP
//! connection to `lifeline-relay`. Runs on plain OS threads (no async), so it
//! composes cleanly with the engine thread.

use lifeline_relay::{read_msg_sync, write_msg_sync, Down, Up};
use lifeline_transport::Outbound;
use std::io::Write;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Spawn the relay client. `out_rx` carries frames the engine wants to send;
/// `in_tx` receives frames from the relay; `peers` mirrors the current peer set;
/// `connected` reflects link state for the UI.
pub fn spawn(
    relay_addr: String,
    out_rx: Receiver<Outbound>,
    in_tx: Sender<(u64, Vec<u8>)>,
    peers: Arc<Mutex<Vec<u64>>>,
    connected: Arc<AtomicBool>,
) {
    // The current writable socket, swapped in on each (re)connect.
    let writer: Arc<Mutex<Option<TcpStream>>> = Arc::new(Mutex::new(None));

    // Writer thread: drain engine outbound → relay.
    {
        let writer = writer.clone();
        std::thread::spawn(move || {
            while let Ok(out) = out_rx.recv() {
                let msg = Up::Send {
                    to: out.to,
                    data: out.frame,
                };
                let mut guard = writer.lock().unwrap();
                if let Some(sock) = guard.as_mut() {
                    if write_msg_sync(sock, &msg).is_err() {
                        // Manager will reconnect and replace the socket.
                        *guard = None;
                    }
                }
            }
        });
    }

    // Manager/reader thread: (re)connect and pump inbound frames.
    std::thread::spawn(move || loop {
        match TcpStream::connect(&relay_addr) {
            Ok(rd) => {
                rd.set_nodelay(true).ok();
                if let Ok(wr) = rd.try_clone() {
                    *writer.lock().unwrap() = Some(wr);
                }
                connected.store(true, Ordering::Relaxed);
                tracing::info!("connected to relay at {relay_addr}");
                let mut rd = rd;
                while let Ok(Some(down)) = read_msg_sync::<_, Down>(&mut rd) {
                    handle_down(down, &in_tx, &peers);
                }
                connected.store(false, Ordering::Relaxed);
                *writer.lock().unwrap() = None;
                peers.lock().unwrap().clear();
                tracing::warn!("relay connection lost; reconnecting…");
            }
            Err(e) => {
                tracing::debug!("relay connect failed: {e}");
            }
        }
        let _ = std::io::stdout().flush();
        std::thread::sleep(Duration::from_secs(1));
    });
}

fn handle_down(down: Down, in_tx: &Sender<(u64, Vec<u8>)>, peers: &Arc<Mutex<Vec<u64>>>) {
    match down {
        Down::Welcome { peers: ps, .. } => {
            *peers.lock().unwrap() = ps;
        }
        Down::Join { id } => {
            let mut p = peers.lock().unwrap();
            if !p.contains(&id) {
                p.push(id);
            }
        }
        Down::Leave { id } => {
            peers.lock().unwrap().retain(|&x| x != id);
        }
        Down::Frame { from, data } => {
            in_tx.send((from, data)).ok();
        }
    }
}
