//! The engine thread: owns the [`NodeEngine`], applies commands from the API,
//! ticks the mesh, and republishes a [`Snapshot`] for the UI.

use crate::views::{
    Command, IdentityView, MsgView, NearbyView, PeerView, PoiView, PosView, Snapshot, StatusView,
    StrobeView,
};
use lifeline_core::Identity;
use lifeline_engine::{EngineConfig, NodeEngine};
use lifeline_proto::codec::{b64url_decode, b64url_encode, from_cbor, to_cbor};
use lifeline_proto::{Address, IdentityPublic, Payload, PayloadKind, Priority};
use lifeline_transport::{ChannelInterface, InterfaceCaps, Outbound, UdpInterface};
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

/// Seal the node's persistent state (contacts + history + prekey ring) and write
/// it crash-safely (FR-9/15/44). Called both on the periodic timer and on a final
/// flush at shutdown, so the two paths can never diverge.
fn persist_state(
    engine: &NodeEngine,
    messages: &[MsgView],
    groups: &[String],
    vault: &lifeline_core::vault::Vault,
    state_path: &std::path::Path,
) {
    let persisted = crate::views::PersistedState {
        contact_codes: engine.directory().iter().map(encode_code).collect(),
        messages: messages.to_vec(),
        // Persist the current prekey ring so a restart keeps forward-secret
        // in-flight messages openable (FR-44).
        prekeys: to_cbor(&engine.export_prekeys())
            .ok()
            .map(lifeline_proto::Bytes::new),
        groups: groups.to_vec(),
    };
    if let Ok(bytes) = serde_json::to_vec(&persisted) {
        let blob = vault.seal(&bytes);
        if let Ok(json) = serde_json::to_vec(&blob) {
            // Atomic replace: a crash mid-write keeps the previous good vault
            // rather than truncating it (which the loader discards as "fresh").
            let _ = crate::write_atomic(state_path, &json);
        }
    }
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
    // Optional extra bearers (the Nostr client's `ChannelInterface`, a Meshtastic
    // `BridgeInterface`, …). Built in `main` behind their features; empty if none.
    extra_ifaces: Vec<Box<dyn lifeline_transport::Interface + Send>>,
) {
    // A node self-hosted as infrastructure (a gateway / always-on relay) can run
    // as a committed custodian so battery-limited carriers offload to it (FR-25).
    let mut cfg = EngineConfig::default();
    if std::env::var("LIFELINE_CUSTODIAN").is_ok() {
        cfg.custody_role = lifeline_engine::CustodyRole::Custodian;
    }
    // Operate as a gateway (FR-35): emit announces so the mesh forms a gradient
    // toward us and bridge mesh bundles onto the uplink. `LIFELINE_GATEWAY` may
    // list capabilities (e.g. `internet,lora`); any truthy value implies internet.
    if let Ok(v) = std::env::var("LIFELINE_GATEWAY") {
        let caps: Vec<String> = v
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && *s != "1")
            .map(|s| s.to_string())
            .collect();
        cfg.gateway_caps = if caps.is_empty() {
            vec!["internet".to_string()]
        } else {
            caps
        };
    }
    let mut engine = NodeEngine::new(identity, cfg);
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
    // Optional extra bearers (Nostr relay network, Meshtastic mesh, …).
    for iface in extra_ifaces {
        engine.add_interface(iface);
    }

    // Restore persisted contacts (FR-9) and message history (FR-15).
    for code in &initial.contact_codes {
        if let Some(p) = decode_code(code) {
            engine.add_contact(p);
        }
    }
    // Restore the forward-secret prekey ring (FR-44) so messages sealed to our
    // pre-restart prekeys still open, and we don't churn to a fresh key.
    if let Some(pk) = &initial.prekeys {
        if let Ok(state) = from_cbor::<lifeline_core::prekey::PrekeyRingState>(pk.as_slice()) {
            engine.restore_prekeys(&state);
        }
    }

    let identity_view = IdentityView {
        address: engine.address().to_text(),
        name: name.clone(),
        code: encode_code(&engine.public()),
    };
    let my_addr = engine.address().to_text();

    let mut messages: Vec<MsgView> = initial.messages;
    // Group ids this node participates in (FR-12); threads keyed `group:<id>`.
    let mut groups: Vec<String> = initial.groups;
    // bundle_id (b64) -> index into `messages` for outbound status updates.
    let mut sent_index: HashMap<String, usize> = HashMap::new();
    // Last-known position per contact (lat, lon, unix_secs), from received
    // Location payloads — the data behind the "Nearby / find each other" view.
    let mut peer_pos: HashMap<Address, (f64, f64, u64)> = HashMap::new();
    // Shared points of interest (wayfinding), keyed by a stable id so a re-share
    // updates in place. Holds both POIs we added and ones contacts sent.
    let mut pois: HashMap<String, PoiRecord> = HashMap::new();
    // The currently-armed strobe beacon `(start, bpm, seconds, from)`, if any;
    // cleared once it elapses.
    let mut active_strobe: Option<(u64, u16, u16, String)> = None;
    let state_path = std::path::Path::new(&data_dir).join("state.vault");
    let mut dirty = false;
    // Unix seconds our own position was last set (for the Nearby view's "you").
    let mut my_pos_at = 0u64;
    let mut last_save_tick = 0u64;
    let mut tick_no = 0u64;
    let mut shutdown = false;
    let mut panic_wipe = false;

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
                    record_broadcast(
                        &mut messages,
                        &mut sent_index,
                        &ids,
                        "out-sos",
                        &format!("SOS · {} contact{}", ids.len(), plural(ids.len())),
                        &note.clone().unwrap_or_else(|| "SOS".into()),
                        now,
                    );
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
                Command::SendPrivate { to, body } => {
                    let Ok(addr) = Address::from_text(&to) else {
                        continue;
                    };
                    let me = engine.address().clone();
                    if let Some(recipient) = engine.contact(&addr) {
                        // Auto-pick up to two intermediate relays from the
                        // directory (never the recipient or ourselves).
                        let relays: Vec<IdentityPublic> = engine
                            .directory()
                            .into_iter()
                            .filter(|p| p.id != addr && p.id != me)
                            .take(2)
                            .collect();
                        let (payload, _) = build_payload(&body, 2);
                        if let Some(id) =
                            engine.submit_onion(&relays, &recipient, payload, Priority::Normal, now)
                        {
                            let idx = messages.len();
                            let peer_name = name_of(&engine, &addr);
                            messages.push(MsgView {
                                id: id.to_b64url(),
                                dir: "out-private".into(),
                                peer: to,
                                peer_name,
                                body,
                                ts: now,
                                status: "private".into(),
                            });
                            sent_index.insert(id.to_b64url(), idx);
                            dirty = true;
                        }
                    }
                }
                Command::Broadcast { body } => {
                    let ids = engine.broadcast_text(&body, Priority::Alert, now);
                    record_broadcast(
                        &mut messages,
                        &mut sent_index,
                        &ids,
                        "out-broadcast",
                        &format!("Broadcast · {} node{}", ids.len(), plural(ids.len())),
                        &body,
                        now,
                    );
                    dirty = true;
                }
                Command::Safe { note } => {
                    let body = note.unwrap_or_else(|| "I'm safe".into());
                    let ids = engine.broadcast_safe(Some(body.clone()), now);
                    record_broadcast(
                        &mut messages,
                        &mut sent_index,
                        &ids,
                        "out",
                        &format!("I'm safe · {} contact{}", ids.len(), plural(ids.len())),
                        &body,
                        now,
                    );
                    dirty = true;
                }
                Command::Location {
                    to,
                    lat,
                    lon,
                    acc_m,
                } => {
                    if let Ok(addr) = Address::from_text(&to) {
                        if let Some(id) =
                            engine.submit_location(&addr, lat, lon, acc_m.unwrap_or(0), now)
                        {
                            let idx = messages.len();
                            let peer_name = name_of(&engine, &addr);
                            messages.push(MsgView {
                                id: id.to_b64url(),
                                dir: "out".into(),
                                peer: to,
                                peer_name,
                                body: format!("📍 shared location ({lat:.4}, {lon:.4})"),
                                ts: now,
                                status: "sent".into(),
                            });
                            sent_index.insert(id.to_b64url(), idx);
                            dirty = true;
                        }
                    }
                }
                Command::LocationAll { lat, lon, acc_m } => {
                    // "Find each other": share our position with every contact at
                    // once, and set our own fix so we can measure distances to
                    // theirs. Recorded as a single mesh-thread bubble.
                    engine.set_position(lat, lon);
                    my_pos_at = now;
                    let mut ids = Vec::new();
                    for peer in engine.directory() {
                        if let Some(id) =
                            engine.submit_location(&peer.id, lat, lon, acc_m.unwrap_or(0), now)
                        {
                            ids.push(id);
                        }
                    }
                    record_broadcast(
                        &mut messages,
                        &mut sent_index,
                        &ids,
                        "out",
                        &name,
                        &format!("📍 shared location with everyone ({lat:.4}, {lon:.4})"),
                        now,
                    );
                    dirty = true;
                }
                Command::AddPoi {
                    name,
                    category,
                    lat,
                    lon,
                    share,
                } => {
                    let name = name.trim().to_string();
                    let category = poi_category(&category);
                    if !name.is_empty() {
                        // Store it locally so it shows on our own wayfinding view.
                        let id = poi_id("me", &category, &name);
                        pois.insert(
                            id,
                            PoiRecord {
                                name: name.clone(),
                                category: category.clone(),
                                lat,
                                lon,
                                at: now,
                                from: "me".into(),
                            },
                        );
                        // And, if asked, broadcast it to the whole crew.
                        if share {
                            engine.broadcast_poi(poi_label(&category, &name), lat, lon, now);
                        }
                        dirty = true;
                    }
                }
                Command::Strobe { bpm, seconds } => {
                    // Clamp seizure-safe (≤ 3 Hz) and to a sane duration, then
                    // arm it locally and broadcast the same phase to the crew.
                    let bpm = bpm.clamp(STROBE_MIN_BPM, STROBE_MAX_BPM);
                    let seconds = seconds.clamp(1, STROBE_MAX_SECONDS);
                    active_strobe = Some((now, bpm, seconds, "you".into()));
                    engine.broadcast_strobe(strobe_label(now, bpm, seconds), now);
                    dirty = true;
                }
                Command::CreateGroup { id } => {
                    let id = id.trim().to_string();
                    if !id.is_empty() {
                        engine.create_group(&id);
                        if !groups.contains(&id) {
                            groups.push(id);
                        }
                        dirty = true;
                    }
                }
                Command::AddGroupMember { group, addr } => {
                    if let Ok(a) = Address::from_text(&addr) {
                        if let Some(member) = engine.contact(&a) {
                            engine.create_group(&group);
                            engine.add_group_member(&group, member);
                            if !groups.contains(&group) {
                                groups.push(group);
                            }
                            dirty = true;
                        }
                    }
                }
                Command::SendGroup { group, body } => {
                    let mut payload = Payload {
                        kind: PayloadKind::Text,
                        body: Some(body.clone()),
                        coords: None,
                        battery_pct: None,
                        attach: None,
                        group_id: Some(group.clone()),
                    };
                    payload.group_id = Some(group.clone());
                    let ids = engine.send_group(&group, payload, now);
                    if !groups.contains(&group) {
                        groups.push(group.clone());
                    }
                    messages.push(MsgView {
                        id: ids.first().map(|b| b.to_b64url()).unwrap_or_default(),
                        dir: "out".into(),
                        peer: format!("group:{group}"),
                        peer_name: name.clone(),
                        body,
                        ts: now,
                        status: "sent".into(),
                    });
                    dirty = true;
                }
                Command::Block { addr } => {
                    if let Ok(a) = Address::from_text(&addr) {
                        engine.block(a);
                        dirty = true;
                    }
                }
                Command::Unblock { addr } => {
                    if let Ok(a) = Address::from_text(&addr) {
                        engine.unblock(&a);
                        dirty = true;
                    }
                }
                Command::SetPosition { lat, lon } => {
                    // Required for geocast *receive*: a node only accepts a
                    // geocast whose region cell matches its own position. Also
                    // anchors distances/bearings in the Nearby view.
                    engine.set_position(lat, lon);
                    my_pos_at = now;
                }
                Command::Geocast {
                    lat,
                    lon,
                    radius_m,
                    body,
                } => {
                    let payload = Payload {
                        kind: PayloadKind::Text,
                        body: Some(body.clone()),
                        coords: None,
                        battery_pct: None,
                        attach: None,
                        group_id: None,
                    };
                    let ids = engine.broadcast_geo(lat, lon, radius_m, payload, now);
                    messages.push(MsgView {
                        id: ids.first().map(|b| b.to_b64url()).unwrap_or_default(),
                        dir: "out".into(),
                        peer: format!("geocast:{radius_m:.0}m"),
                        peer_name: name.clone(),
                        body,
                        ts: now,
                        status: "sent".into(),
                    });
                    dirty = true;
                }
                Command::Shutdown => {
                    shutdown = true;
                }
                Command::Panic => {
                    panic_wipe = true;
                }
            }
        }

        // Panic / duress wipe (G3): destroy on-disk secrets and stop WITHOUT
        // flushing. Checked before the shutdown flush so a Panic can never be
        // overtaken by a state save. Returning drops `engine` (and the identity,
        // prekey ring, and group sender keys it owns), whose `zeroize`-on-drop
        // scrubs every in-memory secret — so this one action clears both disk and
        // memory. Irreversible by design; there is no final persist.
        if panic_wipe {
            let data_dir = state_path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."));
            let report = lifeline_core::wipe::wipe_node_data(data_dir);
            if report.is_complete() {
                tracing::warn!(
                    "engine: PANIC WIPE complete — {} artifact(s), {} bytes destroyed; stopping",
                    report.erased_count(),
                    report.bytes_erased
                );
            } else {
                tracing::error!(
                    "engine: PANIC WIPE partial — {} destroyed, {} FAILED: {:?}",
                    report.erased_count(),
                    report.failed.len(),
                    report.failed
                );
            }
            return;
        }

        // Graceful shutdown: force a final flush of persistent state and exit the
        // loop so `main` can join this thread. Runs before the tick so a rotated
        // prekey ring / recent messages survive SIGTERM (FR-9/15/44).
        if shutdown {
            persist_state(&engine, &messages, &groups, &vault, &state_path);
            tracing::info!("engine: state flushed, stopping");
            return;
        }

        // 2. Advance the mesh.
        engine.tick(now);

        // 3. Ingest anything delivered to us.
        for inb in engine.take_inbox() {
            // Group messages thread under `group:<id>`; the bubble shows the
            // actual sender's name. 1:1 messages thread under the sender address.
            // Thread on the engine's **authenticated** `group` (set only by the
            // verified sender-keys path), NOT the payload's self-asserted
            // `group_id` — otherwise a direct/geocast message could be spoofed
            // into a group thread.
            let (peer, dir) = match &inb.group {
                Some(g) => {
                    if !groups.contains(g) {
                        groups.push(g.clone());
                    }
                    (format!("group:{g}"), "in")
                }
                None if inb.payload.kind == PayloadKind::Sos => (inb.from.to_text(), "in-sos"),
                None => (inb.from.to_text(), "in"),
            };
            // Record any location (shared position or an SOS with coordinates) so
            // the Nearby view can show where this contact is (FR-43).
            if matches!(inb.payload.kind, PayloadKind::Location | PayloadKind::Sos) {
                if let Some(c) = &inb.payload.coords {
                    peer_pos.insert(inb.from.clone(), (c.lat, c.lon, now));
                }
            }
            // A shared point of interest goes to the wayfinding view, not the chat
            // thread: store it and move on without an inbox message.
            if inb.payload.kind == PayloadKind::Poi {
                if let (Some(label), Some(c)) = (&inb.payload.body, &inb.payload.coords) {
                    let (category, name) = parse_poi_label(label);
                    if !name.is_empty() {
                        let from_key = inb.from.to_text();
                        let id = poi_id(&from_key, &category, &name);
                        pois.insert(
                            id,
                            PoiRecord {
                                name,
                                category,
                                lat: c.lat,
                                lon: c.lon,
                                at: now,
                                from: name_of(&engine, &inb.from),
                            },
                        );
                        dirty = true;
                    }
                }
                continue;
            }
            // A strobe beacon arms the synchronized glow, not a chat message.
            if inb.payload.kind == PayloadKind::Strobe {
                if let Some(label) = &inb.payload.body {
                    if let Some((start, bpm, seconds)) = parse_strobe_label(label) {
                        // Ignore a stale one, or an older one than we already show.
                        let live = start.saturating_add(seconds as u64) > now;
                        let newer = active_strobe.as_ref().map_or(true, |(s, ..)| start >= *s);
                        if live && newer {
                            active_strobe =
                                Some((start, bpm, seconds, name_of(&engine, &inb.from)));
                            dirty = true;
                        }
                    }
                }
                continue;
            }
            messages.push(MsgView {
                id: String::new(),
                dir: dir.into(),
                peer,
                peer_name: name_of(&engine, &inb.from),
                body: display_body(&inb.payload),
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
            persist_state(&engine, &messages, &groups, &vault, &state_path);
            last_save_tick = tick_no;
            dirty = false;
        }

        // 5. Publish snapshot.
        let (nearby, my_pos) = build_nearby(&engine, &peer_pos, my_pos_at);
        let poi_views = build_pois(&engine, &pois);
        // Drop a strobe once it has run its course; otherwise surface it.
        if let Some((start, _, seconds, _)) = &active_strobe {
            if start.saturating_add(*seconds as u64) <= now {
                active_strobe = None;
            }
        }
        let strobe_view = active_strobe
            .as_ref()
            .map(|(start, bpm, seconds, from)| StrobeView {
                start: *start,
                bpm: *bpm,
                seconds: *seconds,
                from: from.clone(),
            });
        let snap = build_snapshot(
            &engine,
            &identity_view,
            &my_addr,
            &messages,
            &groups,
            connected.load(Ordering::Relaxed),
            nearby,
            my_pos,
            poi_views,
            strobe_view,
        );
        if let Ok(mut g) = shared.lock() {
            *g = snap;
        }
        version.fetch_add(1, Ordering::Relaxed);

        std::thread::sleep(Duration::from_millis(150));
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Record a mesh-wide broadcast as a single message in the "mesh" thread. All
/// recipient ids map to the one message, so it flips to "verified" as soon as
/// any recipient's receipt returns.
fn record_broadcast(
    messages: &mut Vec<MsgView>,
    sent_index: &mut HashMap<String, usize>,
    ids: &[lifeline_proto::Bytes],
    dir: &str,
    peer_name: &str,
    body: &str,
    now: u64,
) {
    if ids.is_empty() {
        return;
    }
    let idx = messages.len();
    messages.push(MsgView {
        id: ids[0].to_b64url(),
        dir: dir.into(),
        peer: "mesh".into(),
        peer_name: peer_name.into(),
        body: body.into(),
        ts: now,
        status: "sent".into(),
    });
    for id in ids {
        sent_index.insert(id.to_b64url(), idx);
    }
}

/// Human-readable body for an inbound payload (Location/Safe/SOS carry no text
/// body, so synthesize one for the UI).
fn display_body(p: &Payload) -> String {
    use PayloadKind::*;
    match p.kind {
        Location => match &p.coords {
            Some(c) => format!("📍 shared their location ({:.4}, {:.4})", c.lat, c.lon),
            None => "📍 shared their location".into(),
        },
        Safe => p.body.clone().unwrap_or_else(|| "I'm safe".into()),
        Sos => {
            let mut s = p.body.clone().unwrap_or_else(|| "SOS".into());
            if let Some(c) = &p.coords {
                s.push_str(&format!("  ·  📍 {:.4}, {:.4}", c.lat, c.lon));
            }
            if let Some(b) = p.battery_pct {
                s.push_str(&format!("  ·  🔋 {b}%"));
            }
            s
        }
        _ => p.body.clone().unwrap_or_default(),
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

/// Seizure-safe tempo bounds for a strobe. The ceiling is 3 Hz (180/min) per
/// photosensitive-epilepsy guidance; the floor keeps it a visible pulse.
const STROBE_MIN_BPM: u16 = 20;
const STROBE_MAX_BPM: u16 = 180;
const STROBE_MAX_SECONDS: u16 = 120;

/// Encode a strobe's shared parameters into a payload body.
fn strobe_label(start: u64, bpm: u16, seconds: u16) -> String {
    format!("{start}\u{1f}{bpm}\u{1f}{seconds}")
}

/// Parse a strobe body back to `(start, bpm, seconds)`, re-clamping the tempo so
/// a peer can never push us past the seizure-safe ceiling.
fn parse_strobe_label(label: &str) -> Option<(u64, u16, u16)> {
    let mut it = label.split('\u{1f}');
    let start: u64 = it.next()?.parse().ok()?;
    let bpm: u16 = it.next()?.parse().ok()?;
    let seconds: u16 = it.next()?.parse().ok()?;
    Some((
        start,
        bpm.clamp(STROBE_MIN_BPM, STROBE_MAX_BPM),
        seconds.clamp(1, STROBE_MAX_SECONDS),
    ))
}

/// A shared point of interest held in the node loop (wayfinding).
struct PoiRecord {
    name: String,
    category: String,
    lat: f64,
    lon: f64,
    at: u64,
    from: String,
}

/// Canonical POI category slug (defends the wire against arbitrary strings).
fn poi_category(raw: &str) -> String {
    match raw.trim().to_lowercase().as_str() {
        "water" => "water",
        "food" => "food",
        "medical" | "med" | "aid" => "medical",
        "stage" | "music" => "stage",
        "toilet" | "bathroom" | "restroom" | "wc" => "toilet",
        "tent" | "camp" => "tent",
        "car" | "parking" => "car",
        _ => "other",
    }
    .to_string()
}

/// The opaque `category\u{1f}name` label carried in a POI payload body.
fn poi_label(category: &str, name: &str) -> String {
    format!("{category}\u{1f}{name}")
}

/// Parse a POI label back into `(category, name)`. Tolerant of a missing
/// separator (treats the whole thing as the name, category `other`).
fn parse_poi_label(label: &str) -> (String, String) {
    match label.split_once('\u{1f}') {
        Some((cat, name)) => (poi_category(cat), name.trim().to_string()),
        None => ("other".to_string(), label.trim().to_string()),
    }
}

/// Stable id for a POI so a re-share updates in place rather than duplicating.
fn poi_id(from_key: &str, category: &str, name: &str) -> String {
    format!("{from_key}\u{1f}{category}\u{1f}{name}")
}

/// Distance (metres) + compass bearing from our position `me` to a point, or all
/// `None` when we have no fix of our own. Shared by the Nearby and POI views.
fn dist_bearing(
    me: Option<lifeline_geo::GeoPoint>,
    lat: f64,
    lon: f64,
) -> (Option<f64>, Option<f64>, Option<String>) {
    use lifeline_geo::{bearing_deg, compass_8, haversine_m, GeoPoint};
    match me {
        Some(here) => {
            let there = GeoPoint::new(lat, lon);
            let b = bearing_deg(here, there);
            (
                Some(haversine_m(here, there)),
                Some(b),
                Some(compass_8(b).into()),
            )
        }
        None => (None, None, None),
    }
}

/// Build the "Nearby / find each other" data: each contact's last-shared
/// position, annotated with distance + compass direction from *this* node when
/// we have our own fix, sorted nearest-first. Returns the list plus our own
/// position (present iff we have a fix).
fn build_nearby(
    engine: &NodeEngine,
    peer_pos: &HashMap<Address, (f64, f64, u64)>,
    my_pos_at: u64,
) -> (Vec<NearbyView>, Option<PosView>) {
    let me = engine
        .position()
        .map(|(lat, lon)| lifeline_geo::GeoPoint::new(lat, lon));
    let my_pos = engine.position().map(|(lat, lon)| PosView {
        lat,
        lon,
        at: my_pos_at,
    });

    let mut nearby: Vec<NearbyView> = peer_pos
        .iter()
        .map(|(addr, &(lat, lon, at))| {
            let (distance_m, bearing_deg, compass) = dist_bearing(me, lat, lon);
            NearbyView {
                address: addr.to_text(),
                name: name_of(engine, addr),
                lat,
                lon,
                at,
                distance_m,
                bearing_deg,
                compass,
            }
        })
        .collect();

    // Nearest first; without our own fix (no distance), fall back to most-recent.
    nearby.sort_by(|a, b| nearest_first(a.distance_m, a.at, b.distance_m, b.at));

    (nearby, my_pos)
}

/// Build the wayfinding POI list, annotated with distance + direction and sorted
/// nearest-first (unknown distance falls back to most-recent).
fn build_pois(engine: &NodeEngine, pois: &HashMap<String, PoiRecord>) -> Vec<PoiView> {
    let me = engine
        .position()
        .map(|(lat, lon)| lifeline_geo::GeoPoint::new(lat, lon));
    let mut out: Vec<PoiView> = pois
        .iter()
        .map(|(id, p)| {
            let (distance_m, bearing_deg, compass) = dist_bearing(me, p.lat, p.lon);
            PoiView {
                id: id.clone(),
                name: p.name.clone(),
                category: p.category.clone(),
                lat: p.lat,
                lon: p.lon,
                at: p.at,
                from: p.from.clone(),
                distance_m,
                bearing_deg,
                compass,
            }
        })
        .collect();
    out.sort_by(|a, b| nearest_first(a.distance_m, a.at, b.distance_m, b.at));
    out
}

/// Ordering: closest first; entries with no distance (no own fix) sort after
/// ones with a distance, and among themselves by most-recent.
fn nearest_first(da: Option<f64>, ta: u64, db: Option<f64>, tb: u64) -> std::cmp::Ordering {
    match (da, db) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => tb.cmp(&ta),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_snapshot(
    engine: &NodeEngine,
    identity: &IdentityView,
    my_addr: &str,
    messages: &[MsgView],
    groups: &[String],
    relay_connected: bool,
    nearby: Vec<NearbyView>,
    my_pos: Option<PosView>,
    pois: Vec<PoiView>,
    strobe: Option<StrobeView>,
) -> Snapshot {
    let directory: Vec<PeerView> = engine
        .directory()
        .into_iter()
        .filter(|p| p.id.to_text() != my_addr)
        .map(|p| PeerView {
            address: p.id.to_text(),
            blocked: engine.is_blocked(&p.id),
            name: p.display_name.unwrap_or_else(|| p.id.short()),
            verified: false,
        })
        .collect();

    // Group threads: members resolved to display names via the directory.
    let group_views: Vec<crate::views::GroupView> = groups
        .iter()
        .map(|id| crate::views::GroupView {
            id: id.clone(),
            members: engine
                .group_members(id)
                .into_iter()
                .map(|a| crate::views::GroupMemberView {
                    address: a.to_text(),
                    name: name_of(engine, &a),
                })
                .collect(),
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
        groups: group_views,
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
            arq_retransmits: engine.arq_retransmits(),
            is_gateway: engine.is_gateway(),
            gradient: engine.gradient(unix_now()),
        },
        nearby,
        my_pos,
        pois,
        strobe,
    }
}
