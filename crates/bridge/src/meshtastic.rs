//! A **Meshtastic** adapter for Lifeline (real `ServiceEnvelope`/`MeshPacket`
//! protobuf over MQTT).
//!
//! [Meshtastic](https://meshtastic.org) is a widely-deployed LoRa mesh: cheap
//! off-grid radios that also bridge to the internet via MQTT. This adapter lets
//! Lifeline **ride the Meshtastic network** — a Lifeline frame becomes the
//! payload of a genuine Meshtastic `MeshPacket` (on a private application port),
//! wrapped in the `ServiceEnvelope` that every Meshtastic MQTT gateway speaks —
//! so Lifeline traffic flows over real Meshtastic hardware and public MQTT
//! brokers with **no engine change** (it plugs into [`ExternalNet`]).
//!
//! Because MQTT clients are synchronous, [`MeshtasticNet`] implements
//! `ExternalNet` directly and is driven by the engine's own tick — no async
//! runtime, no channel bridging. The network I/O is abstracted behind the small
//! [`MeshBus`] trait: [`MockBroker`]/[`MockBus`] make the whole path testable
//! without a broker, and (behind the `mqtt` feature) [`MqttBus`] is the live
//! `rumqttc` client for real brokers/devices.
//!
//! Addressing mirrors the Nostr adapter: broadcasts go to the Meshtastic
//! broadcast address, directed frames to a peer's learned node number, and the
//! `PeerId ↔ node-number` map is learned from received packets. Lifeline frames
//! are already opaque E2E ciphertext, so we carry them in a plaintext
//! Meshtastic `Data` (the mesh only ever sees ciphertext bytes).

use lifeline_transport::bridge::peer_id_from_identity;
use lifeline_transport::{ExternalNet, InterfaceCaps, PeerId};
use prost::Message as _;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// Meshtastic application port for private/third-party app payloads. Using this
/// keeps Lifeline traffic clearly separated from Meshtastic's own port numbers.
pub const PRIVATE_APP: i32 = 256;
/// Meshtastic broadcast node address (`0xffffffff`).
pub const BROADCAST: u32 = 0xffff_ffff;
/// Conservative MTU: a Meshtastic LoRa `Data.payload` maxes out near 237 bytes,
/// so we advertise 200 and let the engine's fragmenter split larger bundles —
/// keeping frames small enough to also survive a real radio hop, not just MQTT.
const MESH_MTU: usize = 200;

// --- Wire types: a faithful subset of Meshtastic's mesh.proto / mqtt.proto. ---
// Only the fields we read/write are declared; prost skips unknown tags on decode,
// and a oneof field is wire-identical to a plain field, so this is byte-compatible
// with real Meshtastic packets.

/// `Data` — the decoded application payload of a packet (mesh.proto).
#[derive(Clone, PartialEq, prost::Message)]
pub struct Data {
    /// Application port (`PortNum`); we use [`PRIVATE_APP`].
    #[prost(int32, tag = "1")]
    pub portnum: i32,
    /// The opaque application bytes — a Lifeline frame.
    #[prost(bytes = "vec", tag = "2")]
    pub payload: Vec<u8>,
}

/// `MeshPacket` — one packet on the mesh (mesh.proto). `decoded` is the
/// unencrypted `payload_variant`; we never set the `encrypted` variant (tag 5).
#[derive(Clone, PartialEq, prost::Message)]
pub struct MeshPacket {
    #[prost(fixed32, tag = "1")]
    pub from: u32,
    #[prost(fixed32, tag = "2")]
    pub to: u32,
    #[prost(uint32, tag = "3")]
    pub channel: u32,
    #[prost(message, optional, tag = "4")]
    pub decoded: Option<Data>,
    #[prost(fixed32, tag = "6")]
    pub id: u32,
    #[prost(uint32, tag = "9")]
    pub hop_limit: u32,
}

/// `ServiceEnvelope` — the MQTT wrapper every Meshtastic gateway publishes
/// (mqtt.proto): a packet plus which channel and gateway it came through.
#[derive(Clone, PartialEq, prost::Message)]
pub struct ServiceEnvelope {
    #[prost(message, optional, tag = "1")]
    pub packet: Option<MeshPacket>,
    #[prost(string, tag = "2")]
    pub channel_id: String,
    #[prost(string, tag = "3")]
    pub gateway_id: String,
}

/// The transport seam under the adapter: publish an encoded `ServiceEnvelope`
/// to the channel, and drain envelopes received on it. Synchronous, so the
/// engine tick can drive it directly. [`MockBus`] and [`MqttBus`] implement it.
pub trait MeshBus {
    /// Publish an encoded `ServiceEnvelope` to the channel topic.
    fn publish(&mut self, envelope: &[u8]);
    /// Return every `ServiceEnvelope` received since the last poll.
    fn poll(&mut self) -> Vec<Vec<u8>>;
}

/// A Lifeline↔Meshtastic adapter over some [`MeshBus`].
pub struct MeshtasticNet<B: MeshBus> {
    bus: B,
    caps: InterfaceCaps,
    my_node: u32,
    channel: String,
    peers: HashMap<PeerId, u32>,
    /// Dedup by `(from, packet id)` — Meshtastic's own duplicate-suppression key.
    seen: HashSet<u64>,
    next_id: u32,
}

impl<B: MeshBus> MeshtasticNet<B> {
    /// Create an adapter. `my_node` is our Meshtastic node number (derive it
    /// from the Lifeline identity for stability); `channel` is the Meshtastic
    /// channel name every bridged node shares.
    pub fn new(bus: B, my_node: u32, channel: impl Into<String>) -> Self {
        Self {
            bus,
            caps: InterfaceCaps::overlay("meshtastic", MESH_MTU),
            my_node,
            channel: channel.into(),
            peers: HashMap::new(),
            seen: HashSet::new(),
            next_id: 1,
        }
    }

    /// Our Meshtastic node number.
    pub fn node_num(&self) -> u32 {
        self.my_node
    }

    fn build_envelope(&mut self, to_node: u32, frame: &[u8]) -> Vec<u8> {
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let packet = MeshPacket {
            from: self.my_node,
            to: to_node,
            channel: 0,
            decoded: Some(Data {
                portnum: PRIVATE_APP,
                payload: frame.to_vec(),
            }),
            id: self.next_id,
            hop_limit: 3,
        };
        ServiceEnvelope {
            packet: Some(packet),
            channel_id: self.channel.clone(),
            gateway_id: format!("!{:08x}", self.my_node),
        }
        .encode_to_vec()
    }
}

impl<B: MeshBus> ExternalNet for MeshtasticNet<B> {
    fn caps(&self) -> &InterfaceCaps {
        &self.caps
    }

    fn publish(&mut self, to: Option<PeerId>, frame: &[u8]) -> lifeline_transport::Result<()> {
        let to_node = match to {
            None => BROADCAST,
            Some(peer) => match self.peers.get(&peer) {
                Some(node) => *node,
                None => return Ok(()), // peer's node number not learned yet — drop
            },
        };
        let env = self.build_envelope(to_node, frame);
        self.bus.publish(&env);
        Ok(())
    }

    fn receive(&mut self) -> Vec<(PeerId, Vec<u8>)> {
        let mut out = Vec::new();
        for raw in self.bus.poll() {
            let Ok(env) = ServiceEnvelope::decode(raw.as_slice()) else {
                continue;
            };
            if env.channel_id != self.channel {
                continue; // a different Meshtastic channel
            }
            let Some(pkt) = env.packet else { continue };
            if pkt.from == self.my_node || pkt.from == 0 {
                continue; // our own packet, or an unset sender
            }
            let key = ((pkt.from as u64) << 32) | pkt.id as u64;
            if !self.seen.insert(key) {
                continue; // duplicate (mesh re-broadcast / multi-gateway overlap)
            }
            if pkt.to != self.my_node && pkt.to != BROADCAST {
                continue; // addressed to someone else
            }
            let Some(data) = pkt.decoded else { continue }; // encrypted/foreign → can't read
            if data.portnum != PRIVATE_APP {
                continue; // not a Lifeline payload
            }
            let peer = peer_id_from_identity(&pkt.from.to_le_bytes());
            self.peers.insert(peer, pkt.from);
            out.push((peer, data.payload));
        }
        out
    }

    fn peers(&mut self) -> Vec<PeerId> {
        self.peers.keys().copied().collect()
    }
}

// --- In-memory bus for tests: a shared broker every client publishes to and
// polls from (each client keeps its own read cursor). ---

/// A shared in-memory MQTT stand-in. `connect()` hands out a [`MockBus`] per
/// client; a publish on any client is visible to every client's next poll.
#[derive(Clone, Default)]
pub struct MockBroker(Rc<RefCell<Vec<Vec<u8>>>>);

impl MockBroker {
    pub fn new() -> Self {
        Self::default()
    }
    /// A fresh client handle onto this broker.
    pub fn connect(&self) -> MockBus {
        MockBus {
            shared: self.0.clone(),
            cursor: 0,
        }
    }
}

/// One client's view of a [`MockBroker`].
pub struct MockBus {
    shared: Rc<RefCell<Vec<Vec<u8>>>>,
    cursor: usize,
}

impl MeshBus for MockBus {
    fn publish(&mut self, envelope: &[u8]) {
        self.shared.borrow_mut().push(envelope.to_vec());
    }
    fn poll(&mut self) -> Vec<Vec<u8>> {
        let log = self.shared.borrow();
        let out = log[self.cursor..].to_vec();
        self.cursor = log.len();
        out
    }
}

#[cfg(feature = "mqtt")]
pub use mqtt::MqttBus;

/// Live MQTT transport (`mqtt` feature) — a synchronous `rumqttc` client that
/// speaks the Meshtastic MQTT topic convention.
#[cfg(feature = "mqtt")]
mod mqtt {
    use super::MeshBus;
    use rumqttc::{Client, Event, MqttOptions, Packet, QoS};
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// A live Meshtastic MQTT client. Subscribes to `{root}/2/e/{channel}/#`,
    /// publishes to `{root}/2/e/{channel}/!{node}`, and buffers inbound
    /// publishes for [`MeshBus::poll`]. A background thread drives the MQTT
    /// event loop, so `publish`/`poll` never block the engine tick.
    pub struct MqttBus {
        client: Client,
        pub_topic: String,
        queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
    }

    impl MqttBus {
        /// Connect to `host:port` and subscribe to the Meshtastic `channel`
        /// under topic `root` (e.g. `"msh/lifeline"`). `node` is our node
        /// number (used in the publish topic). `client_id` must be unique per
        /// connection to the broker.
        pub fn connect(
            host: &str,
            port: u16,
            client_id: &str,
            root: &str,
            channel: &str,
            node: u32,
        ) -> Result<Self, rumqttc::ClientError> {
            let mut opts = MqttOptions::new(client_id, host, port);
            opts.set_keep_alive(Duration::from_secs(30));
            let (client, mut connection) = Client::new(opts, 64);
            let sub_topic = format!("{root}/2/e/{channel}/#");
            client.subscribe(sub_topic, QoS::AtMostOnce)?;

            let queue: Arc<Mutex<VecDeque<Vec<u8>>>> = Arc::new(Mutex::new(VecDeque::new()));
            let q = queue.clone();
            std::thread::Builder::new()
                .name("lifeline-meshtastic-mqtt".into())
                .spawn(move || {
                    for notification in connection.iter() {
                        match notification {
                            Ok(Event::Incoming(Packet::Publish(p))) => {
                                q.lock().unwrap().push_back(p.payload.to_vec());
                            }
                            Err(e) => {
                                tracing::debug!("meshtastic mqtt: {e}");
                                std::thread::sleep(Duration::from_secs(1));
                            }
                            _ => {}
                        }
                    }
                })
                .expect("spawn meshtastic mqtt thread");

            Ok(Self {
                client,
                pub_topic: format!("{root}/2/e/{channel}/!{node:08x}"),
                queue,
            })
        }
    }

    impl MeshBus for MqttBus {
        fn publish(&mut self, envelope: &[u8]) {
            let _ = self
                .client
                .publish(&self.pub_topic, QoS::AtMostOnce, false, envelope.to_vec());
        }
        fn poll(&mut self) -> Vec<Vec<u8>> {
            let mut q = self.queue.lock().unwrap();
            q.drain(..).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_roundtrips_as_real_meshtastic_protobuf() {
        let env = ServiceEnvelope {
            packet: Some(MeshPacket {
                from: 0x1234_5678,
                to: BROADCAST,
                channel: 0,
                decoded: Some(Data {
                    portnum: PRIVATE_APP,
                    payload: b"hello mesh".to_vec(),
                }),
                id: 42,
                hop_limit: 3,
            }),
            channel_id: "lifeline".into(),
            gateway_id: "!12345678".into(),
        };
        let bytes = env.encode_to_vec();
        let back = ServiceEnvelope::decode(bytes.as_slice()).unwrap();
        assert_eq!(back, env);
        let data = back.packet.unwrap().decoded.unwrap();
        assert_eq!(data.portnum, PRIVATE_APP);
        assert_eq!(data.payload, b"hello mesh");
    }

    #[test]
    fn two_adapters_exchange_frames_over_a_mock_bus() {
        let broker = MockBroker::new();
        let mut alice = MeshtasticNet::new(broker.connect(), 0x0000_00a1, "lifeline");
        let mut bob = MeshtasticNet::new(broker.connect(), 0x0000_00b2, "lifeline");

        // Alice broadcasts; Bob receives it and learns Alice as a peer.
        alice.publish(None, b"alice-beacon").unwrap();
        let got = bob.receive();
        assert_eq!(got.len(), 1);
        let alice_peer = got[0].0;
        assert_eq!(got[0].1, b"alice-beacon");
        assert!(bob.peers().contains(&alice_peer));

        // Bob replies directly; the packet is addressed to Alice's node number.
        bob.publish(Some(alice_peer), b"private-reply").unwrap();
        let got = alice.receive();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, b"private-reply");

        // Alice never receives her own beacon back (sender filtered).
        assert!(!alice.receive().iter().any(|(_, f)| f == b"alice-beacon"));
    }

    #[test]
    fn directed_frame_to_unknown_peer_is_dropped() {
        let broker = MockBroker::new();
        let mut alice = MeshtasticNet::new(broker.connect(), 1, "lifeline");
        // Peer 999 was never learned → publish is a no-op, nothing hits the bus.
        alice.publish(Some(999), b"nowhere").unwrap();
        let mut sniff = MeshtasticNet::new(broker.connect(), 2, "lifeline");
        assert!(sniff.receive().is_empty());
    }

    #[test]
    fn other_channel_is_ignored() {
        let broker = MockBroker::new();
        let mut alice = MeshtasticNet::new(broker.connect(), 1, "lifeline");
        let mut other = MeshtasticNet::new(broker.connect(), 2, "different");
        alice.publish(None, b"beacon").unwrap();
        assert!(other.receive().is_empty(), "channel isolation");
    }
}
