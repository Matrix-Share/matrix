//! Native BLE radio backend (desktop **central** role) via `btleplug`.
//!
//! This is the hardware-bound half of the BLE bearer: it implements the
//! [`GattPort`](lifeline_transport::ble::GattPort) seam that the platform-
//! independent [`BleDriver`](lifeline_transport::ble::BleDriver) pumps frames
//! across. Everything above the seam (ATT-MTU segmentation, reassembly, the
//! engine bridge) is already written and unit-tested in `lifeline-transport`.
//!
//! Structure: [`spawn`] wires an engine-facing `ChannelInterface` to a background
//! `BleDriver` running on its own thread, whose `GattPort` is a [`BtleplugPort`]
//! backed by shared state. An async task on the tokio runtime drives `btleplug`
//! (scan → connect → subscribe → notify/write) and moves segments in and out of
//! that shared state.
//!
//! Gated behind the `ble-radio` feature (off by default) because it needs a real
//! Bluetooth adapter. It cannot be exercised in CI without hardware; correctness
//! of the framing it feeds is covered by the tests in `lifeline_transport::ble`.

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use lifeline_transport::ble::{
    BleDriver, GattPort, FRAME_CHAR_UUID, MIN_ATT_PAYLOAD, SERVICE_UUID,
};
use lifeline_transport::{ChannelInterface, InterfaceCaps, Outbound, PeerId};

use btleplug::api::{
    Central, CentralEvent, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
use btleplug::platform::{Manager, Peripheral, PeripheralId};

/// Bound on the outbound-segment queue so a stalled radio can't grow memory.
const OUT_QUEUE_CAP: usize = 8192;

/// Shared state bridging the sync [`GattPort`] and the async `btleplug` loop.
#[derive(Default)]
struct BleShared {
    /// Connected peers → usable ATT payload bytes per write.
    peers: HashMap<PeerId, usize>,
    /// Inbound segments (from GATT notifications), tagged with their peer.
    inbound: VecDeque<(PeerId, Vec<u8>)>,
    /// Outbound segments queued by the driver, to be written by the async loop.
    outbound: VecDeque<(PeerId, Vec<u8>)>,
}

/// The `GattPort` the [`BleDriver`] talks to: pure shared-state accessors. All the
/// real radio work happens in the async loop that services the same state.
struct BtleplugPort {
    shared: Arc<Mutex<BleShared>>,
}

impl GattPort for BtleplugPort {
    fn connected_peers(&self) -> Vec<PeerId> {
        self.shared
            .lock()
            .map(|s| s.peers.keys().copied().collect())
            .unwrap_or_default()
    }

    fn att_payload(&self, peer: PeerId) -> usize {
        self.shared
            .lock()
            .ok()
            .and_then(|s| s.peers.get(&peer).copied())
            .unwrap_or(MIN_ATT_PAYLOAD)
    }

    fn write(&mut self, peer: PeerId, bytes: &[u8]) -> lifeline_transport::Result<()> {
        if let Ok(mut s) = self.shared.lock() {
            if s.outbound.len() < OUT_QUEUE_CAP {
                s.outbound.push_back((peer, bytes.to_vec()));
            }
        }
        Ok(())
    }

    fn drain(&mut self) -> Vec<(PeerId, Vec<u8>)> {
        self.shared
            .lock()
            .map(|mut s| s.inbound.drain(..).collect())
            .unwrap_or_default()
    }
}

/// A stable, non-zero [`PeerId`] for a discovered peripheral (hash of its id).
fn peer_id_of(id: &PeripheralId) -> PeerId {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    id.to_string().hash(&mut h);
    h.finish() | 1
}

/// Build the BLE bearer if a Bluetooth adapter is available. Returns the engine-
/// facing `ChannelInterface` and spawns the radio driver; `None` if no adapter.
pub fn spawn(handle: &tokio::runtime::Handle) -> Option<ChannelInterface> {
    // We don't probe the adapter here (this runs on a tokio worker, where a
    // blocking `block_on` would panic). The async central loop below discovers the
    // adapter and simply retries if none is present, so a machine with no radio
    // just yields a bearer with no peers rather than a hard failure.
    let (out_tx, out_rx) = std::sync::mpsc::channel::<Outbound>();
    let (in_tx, in_rx) = std::sync::mpsc::channel::<(PeerId, Vec<u8>)>();
    let peers = Arc::new(Mutex::new(Vec::new()));
    let iface = ChannelInterface::new(InterfaceCaps::ble(), out_tx, in_rx, peers.clone());

    let shared = Arc::new(Mutex::new(BleShared::default()));

    // Async central: scan, connect, subscribe, notify/write. Reconnect on error.
    let central_shared = shared.clone();
    handle.spawn(async move {
        loop {
            if let Err(e) = run_central(central_shared.clone()).await {
                tracing::warn!("ble-radio: central loop error: {e}; restarting in 2s");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    });

    // Sync driver thread: pump segments between the engine channels and the port.
    std::thread::Builder::new()
        .name("ble-driver".into())
        .spawn(move || {
            let port = BtleplugPort { shared };
            let mut driver = BleDriver::new(port, out_rx, in_tx, peers);
            loop {
                if !driver.pump() {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        })
        .ok()?;

    tracing::info!("ble-radio: BLE bearer enabled (desktop central)");
    Some(iface)
}

/// One run of the central: set up the adapter, scan for the Lifeline service, and
/// service discovery + connections until an error bubbles up (then the caller
/// restarts us). Per connected peripheral, a task forwards its notifications and
/// the shared outbound queue is drained to its frame characteristic.
async fn run_central(
    shared: Arc<Mutex<BleShared>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let service = uuid::Uuid::parse_str(SERVICE_UUID)?;
    let frame_uuid = uuid::Uuid::parse_str(FRAME_CHAR_UUID)?;

    let manager = Manager::new().await?;
    let adapter = manager
        .adapters()
        .await?
        .into_iter()
        .next()
        .ok_or("no adapter")?;

    adapter
        .start_scan(ScanFilter {
            services: vec![service],
        })
        .await?;

    // Peripherals we've already wired up (so we don't double-connect).
    let handles: Arc<tokio::sync::Mutex<HashMap<PeerId, (Peripheral, Characteristic)>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    // Writer: drain the outbound queue and push each segment to its peer.
    let writer_handles = handles.clone();
    let writer_shared = shared.clone();
    let writer = tokio::spawn(async move {
        loop {
            let batch: Vec<(PeerId, Vec<u8>)> = {
                match writer_shared.lock() {
                    Ok(mut s) => s.outbound.drain(..).collect(),
                    Err(_) => Vec::new(),
                }
            };
            if batch.is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            }
            let map = writer_handles.lock().await;
            for (peer, seg) in batch {
                if let Some((periph, ch)) = map.get(&peer) {
                    let _ = periph.write(ch, &seg, WriteType::WithoutResponse).await;
                }
            }
        }
    });

    let mut events = adapter.events().await?;
    while let Some(ev) = events.next().await {
        match ev {
            CentralEvent::DeviceDiscovered(id) | CentralEvent::DeviceUpdated(id) => {
                let peer = peer_id_of(&id);
                if handles.lock().await.contains_key(&peer) {
                    continue;
                }
                let Ok(periph) = adapter.peripheral(&id).await else {
                    continue;
                };
                if connect_peer(&periph, frame_uuid, peer, &shared, &handles)
                    .await
                    .is_err()
                {
                    let _ = periph.disconnect().await;
                }
            }
            CentralEvent::DeviceDisconnected(id) => {
                let peer = peer_id_of(&id);
                handles.lock().await.remove(&peer);
                if let Ok(mut s) = shared.lock() {
                    s.peers.remove(&peer);
                }
            }
            _ => {}
        }
    }

    writer.abort();
    Ok(())
}

/// Connect to one peripheral, find the frame characteristic, subscribe, and start
/// forwarding its notifications into the shared inbound queue.
async fn connect_peer(
    periph: &Peripheral,
    frame_uuid: uuid::Uuid,
    peer: PeerId,
    shared: &Arc<Mutex<BleShared>>,
    handles: &Arc<tokio::sync::Mutex<HashMap<PeerId, (Peripheral, Characteristic)>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !periph.is_connected().await.unwrap_or(false) {
        periph.connect().await?;
    }
    periph.discover_services().await?;
    let ch = periph
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == frame_uuid)
        .ok_or("frame characteristic not found")?;
    periph.subscribe(&ch).await?;

    // Register as connected. We can't portably read the negotiated ATT MTU from
    // btleplug, so we advertise the always-safe minimum; segmentation stays
    // correct, just chattier. (MTU negotiation is a follow-up.)
    if let Ok(mut s) = shared.lock() {
        s.peers.insert(peer, MIN_ATT_PAYLOAD);
    }
    handles.lock().await.insert(peer, (periph.clone(), ch));

    // Forward this peripheral's notifications into the shared inbound queue.
    let notif_shared = shared.clone();
    let mut notifs = periph.notifications().await?;
    tokio::spawn(async move {
        while let Some(n) = notifs.next().await {
            if let Ok(mut s) = notif_shared.lock() {
                s.inbound.push_back((peer, n.value));
            }
        }
    });
    Ok(())
}
