//! The engine thread: owns the [`NodeEngine`], applies commands from the API,
//! ticks the mesh, and republishes a [`Snapshot`] for the UI.

use crate::views::{Command, IdentityView, MsgView, PeerView, Snapshot, StatusView};
use lifeline_core::Identity;
use lifeline_proto::codec::{b64url_decode, b64url_encode, from_cbor, to_cbor};
use lifeline_proto::{Address, IdentityPublic, Payload, PayloadKind, Priority};
use lifeline_transport::{
    ChannelInterface, EngineConfig, InterfaceCaps, NodeEngine, Outbound, UdpInterface,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::UnboundedReceiver;

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Encode a public identity as a shareable code.
pub fn encode_code(public: &IdentityPublic) -> String {
    to_cbor(public)
        .map(|b| b64url_encode(&b))
        .unwrap_or_default()
}

fn decode_code(code: &str) -> Option<IdentityPublic> {
    let raw = b64url_decode(code.trim()).ok()?;
    from_cbor::<IdentityPublic>(&raw).ok()
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    identity: Identity,
    name: String,
    mut cmd_rx: UnboundedReceiver<Command>,
    out_tx: std::sync::mpsc::Sender<Outbound>,
    in_rx: Receiver<(u64, Vec<u8>)>,
    peers: Arc<Mutex<Vec<u64>>>,
    connected: Arc<AtomicBool>,
    shared: Arc<Mutex<Snapshot>>,
    version: Arc<AtomicU64>,
    udp: Option<UdpInterface>,
    vault: lifeline_core::vault::Vault,
    data_dir: String,
    initial: crate::views::PersistedState,
) {
    let mut engine = NodeEngine::new(identity, EngineConfig::default());
    engine.add_interface(Box::new(ChannelInterface::new(
        InterfaceCaps::internet(),
        out_tx,
        in_rx,
        peers,
    )));
    // Optional infrastructureless LAN transport (meshes with no relay).
    if let Some(udp) = udp {
        engine.add_interface(Box::new(udp));
    }

    // Restore persisted contacts (FR-9) and message history (FR-15).
    for code in &initial.contact_codes {
        if let Some(p) = decode_code(code) {
            engine.add_contact(p);
        }
    }

    let identity_view = IdentityView {
        address: engine.address().to_text(),
        name: name.clone(),
        code: encode_code(&engine.public()),
    };
    let my_addr = engine.address().to_text();

    let mut messages: Vec<MsgView> = initial.messages;
    // bundle_id (b64) -> index into `messages` for outbound status updates.
    let mut sent_index: HashMap<String, usize> = HashMap::new();
    let state_path = std::path::Path::new(&data_dir).join("state.vault");
    let mut dirty = false;
    let mut last_save_tick = 0u64;
    let mut tick_no = 0u64;

    loop {
        let now = unix_now();
        tick_no += 1;

        // 1. Apply queued commands.
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Command::AddContact { code } => {
                    if let Some(p) = decode_code(&code) {
                        engine.add_contact(p);
                        dirty = true;
                    }
                }
                Command::Sos {
                    lat,
                    lon,
                    acc_m,
                    battery_pct,
                    note,
                } => {
                    let coords = match (lat, lon) {
                        (Some(lat), Some(lon)) => Some(lifeline_proto::Coords {
                            lat,
                            lon,
                            acc_m: acc_m.unwrap_or(0),
                        }),
                        _ => None,
                    };
                    let ids = engine.broadcast_sos(coords, battery_pct, note.clone(), now);
                    for id in ids {
                        let idx = messages.len();
                        messages.push(MsgView {
                            id: id.to_b64url(),
                            dir: "out-sos".into(),
                            peer: String::new(),
                            peer_name: "everyone".into(),
                            body: note.clone().unwrap_or_else(|| "SOS".into()),
                            ts: now,
                            status: "sent".into(),
                        });
                        sent_index.insert(id.to_b64url(), idx);
                    }
                    dirty = true;
                }
                Command::Send { to, body, priority } => {
                    let Ok(addr) = Address::from_text(&to) else {
                        continue;
                    };
                    let (payload, prio) = build_payload(&body, priority);
                    if let Some(id) = engine.submit_to_addr(&addr, payload, prio, now) {
                        let idx = messages.len();
                        let peer_name = name_of(&engine, &addr);
                        messages.push(MsgView {
                            id: id.to_b64url(),
                            dir: "out".into(),
                            peer: to,
                            peer_name,
                            body,
                            ts: now,
                            status: "sent".into(),
                        });
                        sent_index.insert(id.to_b64url(), idx);
                        dirty = true;
                    }
                }
            }
        }

        // 2. Advance the mesh.
        engine.tick(now);

        // 3. Ingest anything delivered to us.
        for inb in engine.take_inbox() {
            let dir = if inb.payload.kind == PayloadKind::Sos {
                "in-sos"
            } else {
                "in"
            };
            let peer_name = name_of(&engine, &inb.from);
            messages.push(MsgView {
                id: String::new(),
                dir: dir.into(),
                peer: inb.from.to_text(),
                peer_name,
                body: inb.payload.body.unwrap_or_default(),
                ts: now,
                status: "received".into(),
            });
            dirty = true;
        }

        // 4. Update outbound delivery-proof status.
        for (bid, verified) in engine.sent_status() {
            if verified {
                if let Some(&idx) = sent_index.get(&bid.to_b64url()) {
                    if messages[idx].status != "verified" {
                        messages[idx].status = "verified".into();
                        dirty = true;
                    }
                }
            }
        }

        // 5. Persist encrypted state periodically when something changed (FR-9/15).
        if dirty && tick_no.saturating_sub(last_save_tick) >= 20 {
            let persisted = crate::views::PersistedState {
                contact_codes: engine.directory().iter().map(encode_code).collect(),
                messages: messages.clone(),
            };
            if let Ok(bytes) = serde_json::to_vec(&persisted) {
                let blob = vault.seal(&bytes);
                if let Ok(json) = serde_json::to_vec(&blob) {
                    let _ = std::fs::write(&state_path, json);
                }
            }
            last_save_tick = tick_no;
            dirty = false;
        }

        // 5. Publish snapshot.
        let snap = build_snapshot(
            &engine,
            &identity_view,
            &my_addr,
            &messages,
            connected.load(Ordering::Relaxed),
        );
        if let Ok(mut g) = shared.lock() {
            *g = snap;
        }
        version.fetch_add(1, Ordering::Relaxed);

        std::thread::sleep(Duration::from_millis(150));
    }
}

fn build_payload(body: &str, priority: u8) -> (Payload, Priority) {
    if priority == 0 {
        (
            Payload {
                kind: PayloadKind::Sos,
                body: Some(body.to_string()),
                coords: None,
                battery_pct: None,
                attach: None,
                group_id: None,
            },
            Priority::Sos,
        )
    } else {
        (
            Payload {
                kind: PayloadKind::Text,
                body: Some(body.to_string()),
                coords: None,
                battery_pct: None,
                attach: None,
                group_id: None,
            },
            Priority::from_u8(priority).unwrap_or(Priority::Normal),
        )
    }
}

fn name_of(engine: &NodeEngine, addr: &Address) -> String {
    engine
        .directory()
        .into_iter()
        .find(|p| &p.id == addr)
        .and_then(|p| p.display_name)
        .unwrap_or_else(|| addr.short())
}

fn build_snapshot(
    engine: &NodeEngine,
    identity: &IdentityView,
    my_addr: &str,
    messages: &[MsgView],
    relay_connected: bool,
) -> Snapshot {
    let directory: Vec<PeerView> = engine
        .directory()
        .into_iter()
        .filter(|p| p.id.to_text() != my_addr)
        .map(|p| PeerView {
            address: p.id.to_text(),
            name: p.display_name.unwrap_or_else(|| p.id.short()),
            verified: false,
        })
        .collect();

    let stats = engine.router_stats();
    let sent = engine.sent_status();
    let verified = sent.iter().filter(|(_, v)| *v).count();
    let received = messages.iter().filter(|m| m.dir.starts_with("in")).count();

    Snapshot {
        identity: identity.clone(),
        directory,
        messages: messages.to_vec(),
        status: StatusView {
            relay_connected,
            peer_count: engine.peer_count(),
            interfaces: engine.interface_names(),
            forwarded_copies: stats.forwarded_copies,
            store_len: stats.store_len,
            sent: sent.len(),
            verified,
            received,
            store_bytes: stats.store_bytes,
            duplicates: stats.duplicates,
            dropped_expired: stats.dropped_expired,
            dropped_nopostage: stats.dropped_nopostage,
            custody_transfers: stats.custody_transfers,
            known_gateways: stats.known_gateways,
            retries: engine.retry_count(),
        },
    }
}
