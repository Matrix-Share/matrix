//! The node runtime: an identity + DTN router + CRDT state driven over any set
//! of [`Interface`]s (PRD §7 L1–L5 composed; FR-22 "transports run
//! concurrently").
//!
//! This is the real, transport-independent node. Give it a BLE interface, an
//! ultrasound interface, and an internet interface, and it will advertise,
//! discover peers, and shuttle the *same* end-to-end-encrypted bundles over
//! whichever links exist — fragmenting each to that interface's MTU. Swap the
//! in-memory interfaces for real radios and nothing else changes.

use lifeline_core::announce::{make_gateway_announce, verify_gateway_announce};
use lifeline_core::content::{chunk, cid_of, BlockStore, Manifest, DEFAULT_BLOCK_SIZE};
use lifeline_core::erasure::{fragment_bundle, FragmentCollector};
use lifeline_core::group::{GroupEnvelope, GroupMessage, ReceiverKeyState, SenderKeyState};
use lifeline_core::message::{open_bundle, seal_bundle, SealOptions};
use lifeline_core::onion::{build_onion, peel_onion, Peeled};
use lifeline_core::prekey::{verify_prekey, PrekeyRing};
use lifeline_core::receipt::{make_delivery_receipt, verify_delivery};
use lifeline_core::Identity;
use lifeline_proto::codec::{b64url_decode, b64url_encode, from_cbor, to_cbor};
use lifeline_proto::{
    Address, AttachChunk, Bundle, Bytes, Coords, DeliveryReceipt, GatewayAnnounce, IdentityPublic,
    Payload, PayloadKind, Priority,
};
use lifeline_router::{DtnRouter, IngestOutcome, PeerInfo, RouterConfig};
use lifeline_sync::SharedState;
use lifeline_transport::arq::{ArqRx, ArqTx};
use lifeline_transport::caps::ThroughputClass;
use lifeline_transport::frame::{Fragmenter, Frame, FrameKind};
use lifeline_transport::interface::{Interface, PeerId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// How this node participates in custody transfer (FR-25).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CustodyRole {
    /// Opportunistic: carries bundles and *offloads* them to committed
    /// custodians (freeing its own storage), but never commits itself. This is
    /// the default so ordinary phones behave exactly as before.
    #[default]
    Carrier,
    /// A committed retainer (a gateway, base station, or well-provisioned data
    /// mule): accepts custody of relayed bundles it stores and signs a receipt
    /// so the previous hop can free its copy. A custodian keeps its copy until
    /// delivery or TTL, so custody only ever moves a bundle to a *safer* holder
    /// — it can never reduce delivery probability.
    Custodian,
}

/// Engine tunables.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub router: RouterConfig,
    /// This node's custody-transfer role (FR-25). Default [`CustodyRole::Carrier`].
    pub custody_role: CustodyRole,
    /// Re-spray an unverified message after this many time units (FR-32).
    pub retry_window: u64,
    /// Max re-sprays per message before giving up.
    pub max_retries: u8,
    /// Copy budget to restore on each re-spray.
    pub respray_copies: u16,
    /// Ticks an unacked frame waits before lossy-link ARQ retransmits it.
    pub arq_rto: u64,
    /// ARQ retransmit rounds per message before giving up (DTN re-spray backs it).
    pub arq_max_rounds: u16,
    /// If non-empty, this node is a **gateway** with these capabilities (e.g.
    /// `["internet"]`, FR-35): it emits signed announces so the mesh builds a
    /// gradient toward it, and it bridges mesh bundles onto its uplink (FR-37).
    pub gateway_caps: Vec<String>,
    /// Ticks between this gateway's own announce emissions.
    pub announce_interval: u64,
    /// Ticks between forward-secret prekey rotations (FR-44). Must be large enough
    /// that `prekey_retain` rotations cover the message TTL, so in-flight messages
    /// still open. Default is large so short-lived sessions don't churn keys.
    pub prekey_rotate_interval: u64,
    /// How many recent prekeys to retain (their secrets kept for opening delayed
    /// messages, then deleted for forward secrecy).
    pub prekey_retain: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            router: RouterConfig::default(),
            custody_role: CustodyRole::default(),
            retry_window: 60,
            max_retries: 4,
            respray_copies: 6,
            arq_rto: lifeline_transport::arq::DEFAULT_RTO,
            arq_max_rounds: lifeline_transport::arq::DEFAULT_MAX_ROUNDS,
            gateway_caps: Vec::new(),
            announce_interval: 10,
            prekey_rotate_interval: 86_400,
            prekey_retain: 8,
        }
    }
}

/// Node presence beacon (FR-7): identity plus enough routing hints for a
/// neighbour to decide whether handing us a bundle moves it *toward* a gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Beacon {
    id: IdentityPublic,
    /// Is the beaconing node itself a gateway?
    gw: bool,
    /// The node's current gradient (hops to nearest gateway), if any.
    grad: Option<u16>,
    /// Differential time beacon, if this node has a fresh time fix. Optional +
    /// skipped-when-none so it stays wire-compatible with older beacons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ts: Option<lifeline_timesync::TimeBeacon>,
}

/// What we've learned about a neighbour from its beacon, used to route downhill.
#[derive(Debug, Clone, Default)]
struct PeerMeta {
    is_gateway: bool,
    gradient: Option<u16>,
}

/// Bandwidth-adaptive routing: the largest NORMAL/BULK bundle a bearer of this
/// throughput class should carry before we make it wait for a fatter link
/// (gateway doc §4 — "the app is asynchronous-first, compressed, and
/// priority-aware, so the straw is never wasted"). `None` = no limit.
fn bearer_soft_cap(t: ThroughputClass) -> Option<u64> {
    match t {
        // Ultrasound/optical: only short texts, receipts, SOS trickle through.
        ThroughputClass::VeryLow => Some(512),
        // BLE / LoRa: modest messages; large attachments wait.
        ThroughputClass::Low => Some(16 * 1024),
        // Wi-Fi Aware: generous, but still not a bulk firehose for chatter.
        ThroughputClass::Medium => Some(256 * 1024),
        // Internet uplink: no limit.
        ThroughputClass::High => None,
    }
}

/// A message delivered up to the application (decrypted, sender authenticated).
#[derive(Debug, Clone)]
pub struct Inbound {
    pub from: Address,
    pub payload: Payload,
    /// The **authenticated** group this message belongs to, set *only* by the
    /// sender-keys group path after verifying the sender-key binding — never from
    /// the (attacker-controllable) `payload.group_id`. `None` for a 1:1/geocast/
    /// onion message. The UI must thread on this, not on `payload.group_id`, or a
    /// direct message could be spoofed into a group thread.
    pub group: Option<String>,
}

struct Port {
    iface: Box<dyn Interface>,
    /// Reassembly + selective-ACK generation for inbound frames.
    rx: ArqRx,
    /// Retransmission bookkeeping for outbound reliable (Bundle) messages.
    tx: ArqTx,
}

struct SentRec {
    bundle_id: Bytes,
    original: Bundle,
    verified: bool,
    submitted_at: u64,
    retries: u8,
}

/// A full node driven over pluggable interfaces.
pub struct NodeEngine {
    identity: Identity,
    public: IdentityPublic,
    router: DtnRouter,
    state: SharedState,
    ports: Vec<Port>,
    /// Discovered peers per (port index, peer handle) → their address.
    peer_addr: HashMap<(usize, PeerId), Address>,
    /// Known public identities (from beacons or added contacts).
    contacts: HashMap<Address, IdentityPublic>,
    sent: Vec<SentRec>,
    inbox: Vec<Inbound>,
    mid_counter: u64,
    /// Reassembles erasure-coded fragment bundles (FR-28).
    frag_collector: FragmentCollector,
    /// Groups this node sends to: our sender key + who we've distributed it to.
    groups: HashMap<String, GroupState>,
    /// Receiving state per (group, sender) for decrypting others' messages.
    receivers: HashMap<(String, Address), ReceiverKeyState>,
    /// Group messages awaiting the sender's key distribution (out-of-order DTN).
    pending_group: Vec<(String, Address, GroupMessage)>,
    /// Peeled onion layers awaiting knowledge of their next hop's key (FR-49):
    /// `(next_hop, remaining_onion, priority)`. Drained once the hop is learned.
    pending_onion: Vec<(Address, Bytes, Priority)>,
    /// Adaptive-retry config (FR-32) + counter.
    retry_window: u64,
    max_retries: u8,
    respray_copies: u16,
    retries_total: u64,
    /// Lossy-link ARQ tunables (FR — Dhwani noise robustness).
    arq_rto: u64,
    arq_max_rounds: u16,
    /// Custody-transfer role (FR-25).
    custody_role: CustodyRole,
    /// Bundle ids this node *originated* (our own messages, receipts, fragments).
    /// We never release these on custody — only relayed copies carried for others.
    origin_ids: HashSet<Bytes>,
    /// Routing hints learned from each neighbour's beacon (gateway flag +
    /// gradient), keyed like `peer_addr`.
    peer_meta: HashMap<(usize, PeerId), PeerMeta>,
    /// This node's gateway capabilities (empty = not a gateway).
    gateway_caps: Vec<String>,
    /// Monotonic announce freshness counter + next-emit schedule (gateways only).
    announce_seq: u64,
    next_announce: u64,
    announce_interval: u64,
    /// Next tick at which we re-gossip known announces (throttled so a cache of
    /// announces isn't re-broadcast on every single tick).
    next_gossip: u64,
    /// Forward-secret prekey ring (FR-44): rotating recipient keys we publish in
    /// our beacon and open incoming messages against.
    prekey_ring: PrekeyRing,
    next_prekey_rotate: u64,
    prekey_rotate_interval: u64,
    /// Content-addressed block store (FR-13): serves blocks by CID to peers.
    blocks: BlockStore,
    /// Manifests we are actively pulling blocks for, keyed by root CID.
    pending_fetches: HashMap<Bytes, PendingFetch>,
    /// Objects fully fetched and verified, awaiting the app: `(root_cid, bytes)`.
    fetched: Vec<(Bytes, Vec<u8>)>,
    /// This node's current position (from GPS), if known — enables receiving
    /// geocasts ("anyone near here") and, later, geographic routing.
    my_pos: Option<lifeline_geo::GeoPoint>,
    /// Bundle ids of geocasts already delivered to our app (dedup).
    geocast_seen: HashSet<Bytes>,
    /// Differential mesh time (FR — offline clock). Disciplined by neighbour
    /// beacons and, if present, a local GPS reference. Advisory only: it is used
    /// for a coarse "mesh time" display and message timestamps, and is **never**
    /// fed into TTL/expiry or any security decision — beacons are unauthenticated,
    /// so gating security on them would be a clock-skew attack surface. (The crate
    /// still bounds each step, limiting drift regardless.)
    time: lifeline_timesync::TimeSync,
    /// Addresses this node *explicitly paired* with (added via invite code / QR —
    /// an out-of-band trust act), as opposed to peers merely auto-discovered from
    /// a beacon (TOFU presence). Only paired peers are allowed to discipline our
    /// clock, so a stranger who simply beacons at us cannot skew our time.
    paired: HashSet<Address>,
    /// Bundle ids each neighbour last told us it holds (from its held-bundle
    /// digest). Feeds `PeerInfo.known` so we don't re-offer a bundle a peer
    /// already has (anti-entropy; §12.3).
    peer_bundles: HashMap<(usize, PeerId), HashSet<Bytes>>,
    /// Fingerprint of our held-bundle set at the last digest we broadcast, so an
    /// unchanged store doesn't re-broadcast the same digest (the differential
    /// idea — send the summary only when it moved).
    last_digest_fp: [u8; 32],
    /// Next tick we may broadcast a held-bundle digest (throttle).
    next_digest: u64,
}

/// How long a time fix stays fresh before a node stops advertising / trusting it.
const TIME_STALE_S: i64 = 3600;
/// Ticks between held-bundle digest broadcasts.
const DIGEST_INTERVAL: u64 = 5;
/// Max bundle ids carried in one digest, to bound its wire size.
const MAX_DIGEST_IDS: usize = 4096;

/// A content object being pulled over the mesh (FR-13), possibly from **many**
/// providers in parallel ("swarm fetch" — BitTorrent-over-DTN). Completeness is
/// monotone (the missing set only shrinks) and idempotent (blocks are
/// content-addressed and deduped), so pulling the same object from several peers
/// over several carriers at once is always safe.
struct PendingFetch {
    manifest: Manifest,
    /// Peers we may ask for missing blocks. More than one enables multi-source
    /// gap-fill and lets us route around a provider that has gone dark.
    providers: Vec<Address>,
    /// Per-CID set of providers that have *advertised* holding that block (via a
    /// [`PayloadKind::HaveReply`]). Empty for a CID until someone answers a HAVE
    /// query; we then fall back to asking all providers.
    holders: HashMap<Bytes, Vec<Address>>,
    /// Rotation counter, bumped each retry, so a block that a chosen provider
    /// failed to deliver is re-asked of a *different* provider next round.
    rotation: usize,
    /// Last tick we (re-)sent requests, for retransmission of lost requests.
    last_req: u64,
}

/// Ticks between re-requesting still-missing blocks of a pending fetch.
const BLOCK_REREQUEST: u64 = 15;

/// Cap on the number of CIDs carried in a single HAVE query/reply, to bound the
/// size of a swarm-discovery message.
const MAX_HAVE_CIDS: usize = 4096;

/// Geohash precision for geocast cells (6 ≈ 1.2 km × 0.6 km). Fixed so a receiver
/// can match its own cell against a geocast's address without a wire field.
const GEOCAST_PRECISION: usize = 6;
/// Spray copy budget for a geocast, so it spreads across the whole region rather
/// than down a single path.
const GEOCAST_COPIES: u16 = 12;
/// Hard cap on a geocast's radius (metres) — bounds the covered-cell count so one
/// call can't flood the mesh.
const MAX_GEOCAST_RADIUS_M: f64 = 5_000.0;
/// Hard cap on the number of covering cells (and thus sealed bundles) per geocast.
const MAX_GEOCAST_CELLS: usize = 64;
/// Cap on the geocast-delivery dedup set; cleared past this so it can't grow
/// unbounded under a flood of unique-id geocasts (worst case: a rare re-delivery).
const MAX_GEOCAST_SEEN: usize = 8_192;

/// How long a gateway announce stays fresh (must exceed `announce_interval` so a
/// gateway's gradient doesn't lapse between emissions).
const ANNOUNCE_TTL_S: u64 = 300;

/// Cap on discovered peers/contacts, to bound memory against a beacon flood of
/// fabricated identities. Already-known peers keep updating past the cap.
const MAX_CONTACTS: usize = 50_000;

/// How long (seconds) we wait for our sealed delivery receipt after a peer takes
/// custody of one of our bundles before counting a miss against that custodian
/// (FR-47 black-hole attribution). Deliberately generous — DTN delivery is slow,
/// and misses only bite after a grace count — so an honest carrier is safe.
const ATTRIBUTION_WINDOW_SECS: u64 = 1800;

/// Per-group sending state (FR-12).
struct GroupState {
    sender_key: SenderKeyState,
    distributed_to: HashSet<Address>,
}

impl NodeEngine {
    pub fn new(identity: Identity, cfg: EngineConfig) -> Self {
        let public = identity.public();
        let (retry_window, max_retries, respray_copies) =
            (cfg.retry_window, cfg.max_retries, cfg.respray_copies);
        let (arq_rto, arq_max_rounds) = (cfg.arq_rto, cfg.arq_max_rounds);
        let custody_role = cfg.custody_role;
        // Start with one prekey so our very first beacon already advertises a
        // forward-secret key.
        let mut prekey_ring = PrekeyRing::new(cfg.prekey_retain);
        prekey_ring.rotate();
        let prekey_rotate_interval = cfg.prekey_rotate_interval.max(1);
        let gateway_caps = cfg.gateway_caps.clone();
        let announce_interval = cfg.announce_interval.max(1);
        let mut router = DtnRouter::new(public.id.clone(), cfg.router);
        if !gateway_caps.is_empty() {
            router.set_gateway(gateway_caps.clone());
        }
        let state = SharedState::new(public.id.clone());
        NodeEngine {
            identity,
            public: public.clone(),
            router,
            state,
            ports: Vec::new(),
            peer_addr: HashMap::new(),
            contacts: HashMap::new(),
            sent: Vec::new(),
            inbox: Vec::new(),
            mid_counter: 0,
            frag_collector: FragmentCollector::new(),
            groups: HashMap::new(),
            receivers: HashMap::new(),
            pending_group: Vec::new(),
            pending_onion: Vec::new(),
            retry_window,
            max_retries,
            respray_copies,
            retries_total: 0,
            arq_rto,
            arq_max_rounds,
            custody_role,
            origin_ids: HashSet::new(),
            peer_meta: HashMap::new(),
            gateway_caps,
            announce_seq: 0,
            next_announce: 0,
            announce_interval,
            next_gossip: 0,
            prekey_ring,
            next_prekey_rotate: prekey_rotate_interval,
            prekey_rotate_interval,
            blocks: BlockStore::new(),
            pending_fetches: HashMap::new(),
            fetched: Vec::new(),
            my_pos: None,
            geocast_seen: HashSet::new(),
            time: lifeline_timesync::TimeSync::new(),
            paired: HashSet::new(),
            peer_bundles: HashMap::new(),
            last_digest_fp: [0u8; 32],
            next_digest: 0,
        }
    }

    /// Attach an interface (BLE, ultrasound, internet, …). Interfaces run
    /// concurrently; the same bundle can travel over any of them (FR-22).
    pub fn add_interface(&mut self, iface: Box<dyn Interface>) {
        self.ports.push(Port {
            iface,
            rx: ArqRx::new(),
            tx: ArqTx::new(),
        });
    }

    pub fn public(&self) -> IdentityPublic {
        self.public.clone()
    }

    pub fn address(&self) -> &Address {
        &self.public.id
    }

    /// Pre-load a contact's public identity (as if scanned by QR, FR-6). Not
    /// strictly required — beacons discover peers automatically — but lets a
    /// node send before it has heard the recipient's beacon.
    pub fn add_contact(&mut self, who: IdentityPublic) {
        self.paired.insert(who.id.clone());
        self.contacts.insert(who.id.clone(), who);
    }

    /// Seal a payload to `to` and inject it into the mesh (§12.1).
    pub fn submit(
        &mut self,
        to: &IdentityPublic,
        payload: Payload,
        priority: Priority,
        now: u64,
    ) -> Bytes {
        let opts = SealOptions::normal(now).with_priority(priority);
        let recipient = self.fs_recipient(to);
        let bundle = seal_bundle(&self.identity, &recipient, &payload, &opts).expect("seal");
        let id = bundle.bundle_id.clone();
        self.sent.push(SentRec {
            bundle_id: id.clone(),
            original: bundle.clone(),
            verified: false,
            submitted_at: now,
            retries: 0,
        });
        self.originate(bundle, now);
        id
    }

    /// Forward secrecy (FR-44): if the recipient advertises a valid current prekey
    /// (learned from a beacon), seal to it instead of their long-term key. A
    /// prekey that doesn't self-verify or whose owner ≠ the recipient is ignored,
    /// falling back to the long-term key — so a forged prekey can only cost
    /// forward secrecy, never redirect the message.
    fn fs_recipient(&self, to: &IdentityPublic) -> IdentityPublic {
        if let Some(pk) = &to.prekey {
            if pk.owner == to.id && verify_prekey(pk).is_ok() {
                let mut r = to.clone();
                r.kex_pub = pk.kex_pub.clone();
                r.prekey = None; // the sealed recipient record needn't carry it
                return r;
            }
        }
        to.clone()
    }

    /// Inject a bundle this node *originated* into the mesh, remembering its id
    /// so custody transfer never releases it (we only offload relayed copies).
    fn originate(&mut self, bundle: Bundle, now: u64) {
        self.origin_ids.insert(bundle.bundle_id.clone());
        self.router.submit_local(bundle, now);
    }

    /// Seal a payload to a *known* address (from the directory/contacts) and
    /// inject it. Returns the bundle id, or `None` if the address is unknown
    /// (we need the recipient's key to encrypt).
    pub fn submit_to_addr(
        &mut self,
        to: &Address,
        payload: Payload,
        priority: Priority,
        now: u64,
    ) -> Option<Bytes> {
        let recipient = self.contacts.get(to)?.clone();
        Some(self.submit(&recipient, payload, priority, now))
    }

    /// Seal a payload and send it **erasure-coded** into `k + m` fragment
    /// bundles (FR-28): any `k` of them reconstruct it, so the message survives
    /// partial carrier escape. Returns the group id (the original bundle id),
    /// which is what a returned receipt references.
    pub fn submit_erasure(
        &mut self,
        to: &IdentityPublic,
        payload: Payload,
        k: usize,
        m: usize,
        priority: Priority,
        now: u64,
    ) -> Bytes {
        let opts = SealOptions::normal(now).with_priority(priority);
        let bundle = seal_bundle(&self.identity, to, &payload, &opts).expect("seal");
        let group = bundle.bundle_id.clone();
        self.sent.push(SentRec {
            bundle_id: group.clone(),
            original: bundle.clone(),
            verified: false,
            submitted_at: now,
            retries: 0,
        });
        match fragment_bundle(&bundle, k, m) {
            Ok(frags) => {
                for f in frags {
                    self.originate(f, now);
                }
            }
            // Fallback: if coding fails, send the whole bundle.
            Err(_) => self.originate(bundle, now),
        }
        group
    }

    /// Send `payload` to `recipient` via **onion routing** over `relays`
    /// (FR-49): the message is wrapped in one encryption layer per relay, so each
    /// hop learns only the *next* hop — never who is ultimately talking to whom.
    /// The relays must form a reachable chain (each able to reach the next). An
    /// empty `relays` slice is a direct, still-metadata-minimal send.
    ///
    /// Returns the first-hop bundle id.
    pub fn submit_onion(
        &mut self,
        relays: &[IdentityPublic],
        recipient: &IdentityPublic,
        payload: Payload,
        priority: Priority,
        now: u64,
    ) -> Option<Bytes> {
        let inner = to_cbor(&payload).ok()?;
        let (_first_hop, onion) = build_onion(relays, recipient, &inner).ok()?;
        // The first hop's key comes straight from the chosen path (or the
        // recipient for a direct send) — no directory lookup needed.
        let first_pub = relays.first().cloned().unwrap_or_else(|| recipient.clone());
        Some(self.submit_onion_hop(&first_pub, onion, priority, now))
    }

    /// Seal one onion blob into a bundle addressed to `to` and inject it. Used
    /// both to originate an onion and to forward a peeled one to the next hop;
    /// re-sealing at each hop is what makes a relay appear as the sender to the
    /// next, completing the metadata-privacy property.
    fn submit_onion_hop(
        &mut self,
        to: &IdentityPublic,
        onion: Bytes,
        priority: Priority,
        now: u64,
    ) -> Bytes {
        let payload = Payload {
            kind: PayloadKind::Onion,
            body: Some(b64url_encode(&onion.0)),
            coords: None,
            battery_pct: None,
            attach: None,
            group_id: None,
        };
        let opts = SealOptions::normal(now).with_priority(priority);
        let bundle = seal_bundle(&self.identity, to, &payload, &opts).expect("seal onion");
        let id = bundle.bundle_id.clone();
        self.originate(bundle, now);
        id
    }

    // --- Content-addressed blocks / mesh fetch-by-CID (FR-13) ---

    /// Chunk `data` into content-addressed blocks, store them locally, and return
    /// the [`Manifest`] (a small root description). Send the manifest to a peer
    /// in a normal message; they call [`NodeEngine::fetch_content`] to pull the
    /// blocks by CID. Identical content dedups to the same CIDs automatically.
    pub fn store_content(&mut self, data: &[u8]) -> Manifest {
        let (manifest, blocks) = chunk(data, DEFAULT_BLOCK_SIZE);
        let _ = self.blocks.put_all(blocks);
        manifest
    }

    /// Store a single content-addressed block (e.g. one received out-of-band or
    /// cached individually), returning its CID. Complements
    /// [`NodeEngine::store_content`] for callers that hold individual blocks
    /// rather than a whole object — e.g. a swarm peer that carries only part of
    /// an object.
    pub fn store_block(&mut self, block: Vec<u8>) -> Bytes {
        self.blocks.put(block)
    }

    /// Whether a block is already in our local store (diagnostics / tests).
    pub fn has_block(&self, cid: &Bytes) -> bool {
        self.blocks.has(cid)
    }

    /// Begin fetching the object described by `manifest` from a single provider
    /// `from` (FR-13). Thin wrapper over [`NodeEngine::fetch_content_swarm`].
    pub fn fetch_content(&mut self, manifest: Manifest, from: Address, now: u64) {
        self.fetch_content_swarm(manifest, vec![from], now);
    }

    /// Begin **swarm fetching** the object described by `manifest` from any of
    /// `providers` (FR-13) — the "BitTorrent-over-DTN" path. Each missing block
    /// is pulled from whichever provider holds it, spread across providers for
    /// parallelism, and re-asked of a *different* provider on retry so a dark or
    /// black-hole provider can't stall the transfer. A HAVE query discovers which
    /// provider holds which block; content-addressing keeps duplicate arrivals
    /// idempotent. Completed objects surface via
    /// [`NodeEngine::take_fetched_content`]; if we already hold every block the
    /// object completes immediately with no traffic.
    pub fn fetch_content_swarm(&mut self, manifest: Manifest, providers: Vec<Address>, now: u64) {
        let root = manifest.root.clone();
        // De-duplicate providers, preserving order.
        let mut seen = HashSet::new();
        let providers: Vec<Address> = providers
            .into_iter()
            .filter(|p| seen.insert(p.clone()))
            .collect();
        self.pending_fetches.insert(
            root.clone(),
            PendingFetch {
                manifest,
                providers,
                holders: HashMap::new(),
                rotation: 0,
                last_req: 0,
            },
        );
        self.try_complete_fetches();
        // Ask every provider which of the still-missing blocks it holds, then
        // fire the first round of requests (each block to one provider — no
        // duplication; HAVE replies refine targeting for later rounds).
        self.send_have_queries(&root, now);
        self.request_missing_blocks(&root, now);
    }

    /// Objects fully fetched-and-verified since the last call: `(root_cid, bytes)`.
    pub fn take_fetched_content(&mut self) -> Vec<(Bytes, Vec<u8>)> {
        std::mem::take(&mut self.fetched)
    }

    /// Request every still-missing block of a pending fetch (bulk priority —
    /// never competes with SOS), spreading the requests across providers.
    ///
    /// For each missing CID we pick a provider that has *advertised* holding it
    /// (from HAVE replies) if any, else fall back to the whole provider list. The
    /// choice is offset by the fetch's `rotation` counter and the block's index,
    /// so (a) different blocks go to different providers in the same round
    /// (parallel multi-source), and (b) a block a provider failed to deliver is
    /// re-asked of a *different* provider next round (routes around a dead or
    /// black-hole provider). No block is requested from more than one provider
    /// per round, so there is no duplicate traffic; content-addressing makes any
    /// duplicate arrivals harmless anyway.
    fn request_missing_blocks(&mut self, root: &Bytes, now: u64) {
        let Some(pf) = self.pending_fetches.get(root) else {
            return;
        };
        let missing = self.blocks.missing(&pf.manifest);
        if missing.is_empty() {
            return;
        }
        let rotation = pf.rotation;
        let all_providers = pf.providers.clone();
        // Resolve each missing CID to a provider address to ask this round.
        let mut plan: Vec<(Address, Bytes)> = Vec::new();
        for (idx, cid) in missing.into_iter().enumerate() {
            let candidates: &[Address] = match pf.holders.get(&cid) {
                Some(h) if !h.is_empty() => h,
                _ => &all_providers,
            };
            if candidates.is_empty() {
                continue;
            }
            let who = candidates[(rotation + idx) % candidates.len()].clone();
            plan.push((who, cid));
        }
        for (who, cid) in plan {
            let Some(provider) = self.contacts.get(&who).cloned() else {
                continue; // provider's key unknown yet — a retry will pick it up
            };
            let payload = Payload {
                kind: PayloadKind::BlockRequest,
                body: Some(b64url_encode(&cid.0)),
                coords: None,
                battery_pct: None,
                attach: None,
                group_id: None,
            };
            let opts = SealOptions::normal(now).with_priority(Priority::Bulk);
            if let Ok(b) = seal_bundle(&self.identity, &provider, &payload, &opts) {
                self.originate(b, now);
            }
        }
        if let Some(pf) = self.pending_fetches.get_mut(root) {
            pf.last_req = now;
            pf.rotation = pf.rotation.wrapping_add(1);
        }
    }

    /// Ask each provider of a pending fetch which of the still-missing blocks it
    /// holds (a HAVE query), so later rounds target confirmed holders.
    fn send_have_queries(&mut self, root: &Bytes, now: u64) {
        let Some(pf) = self.pending_fetches.get(root) else {
            return;
        };
        let missing = self.blocks.missing(&pf.manifest);
        if missing.is_empty() {
            return;
        }
        let cids: Vec<Vec<u8>> = missing
            .iter()
            .take(MAX_HAVE_CIDS)
            .map(|c| c.0.clone())
            .collect();
        let body = match to_cbor(&(root.0.clone(), cids)) {
            Ok(b) => b64url_encode(&b),
            Err(_) => return,
        };
        let providers = pf.providers.clone();
        for who in providers {
            let Some(provider) = self.contacts.get(&who).cloned() else {
                continue;
            };
            let payload = Payload {
                kind: PayloadKind::HaveQuery,
                body: Some(body.clone()),
                coords: None,
                battery_pct: None,
                attach: None,
                group_id: None,
            };
            let opts = SealOptions::normal(now).with_priority(Priority::Bulk);
            if let Ok(b) = seal_bundle(&self.identity, &provider, &payload, &opts) {
                self.originate(b, now);
            }
        }
    }

    /// Serve an inbound HAVE query: of the queried CIDs, reply with the subset we
    /// actually hold (bounded), so the asker can target us for those blocks.
    fn handle_have_query(&mut self, opened: lifeline_core::message::Opened, now: u64) {
        let Some(body) = opened.payload.body.as_ref() else {
            return;
        };
        let Ok(raw) = b64url_decode(body) else {
            return;
        };
        let Ok((root, cids)): Result<(Vec<u8>, Vec<Vec<u8>>), _> = from_cbor(&raw) else {
            return;
        };
        let have: Vec<Vec<u8>> = cids
            .into_iter()
            .take(MAX_HAVE_CIDS)
            .filter(|c| self.blocks.has(&Bytes::new(c.clone())))
            .collect();
        if have.is_empty() {
            return; // nothing to offer — stay silent
        }
        let body = match to_cbor(&(root, have)) {
            Ok(b) => b64url_encode(&b),
            Err(_) => return,
        };
        let payload = Payload {
            kind: PayloadKind::HaveReply,
            body: Some(body),
            coords: None,
            battery_pct: None,
            attach: None,
            group_id: None,
        };
        let opts = SealOptions::normal(now).with_priority(Priority::Bulk);
        if let Ok(b) = seal_bundle(&self.identity, &opened.sender, &payload, &opts) {
            self.originate(b, now);
        }
    }

    /// Record a HAVE reply: the sender holds these CIDs of a pending fetch, so
    /// future rounds can target it. Then fire a request round with the new info.
    fn handle_have_reply(&mut self, opened: lifeline_core::message::Opened, now: u64) {
        let Some(body) = opened.payload.body.as_ref() else {
            return;
        };
        let Ok(raw) = b64url_decode(body) else {
            return;
        };
        let Ok((root, cids)): Result<(Vec<u8>, Vec<Vec<u8>>), _> = from_cbor(&raw) else {
            return;
        };
        let root = Bytes::new(root);
        let sender = opened.sender.id.clone();
        let Some(pf) = self.pending_fetches.get_mut(&root) else {
            return;
        };
        // Only record CIDs that are genuinely part of this object's manifest.
        let manifest_cids: HashSet<&Bytes> = pf.manifest.blocks.iter().collect();
        let mut learned = false;
        for c in cids.into_iter().take(MAX_HAVE_CIDS) {
            let cid = Bytes::new(c);
            if !manifest_cids.contains(&cid) {
                continue;
            }
            let holders = pf.holders.entry(cid).or_default();
            if !holders.contains(&sender) {
                holders.push(sender.clone());
                learned = true;
            }
        }
        if learned {
            self.request_missing_blocks(&root, now);
        }
    }

    /// Re-request the blocks of any pending fetch that has gone quiet, so a lost
    /// request or response doesn't stall the transfer.
    fn retry_fetches(&mut self, now: u64) {
        let roots: Vec<Bytes> = self
            .pending_fetches
            .iter()
            .filter(|(_, pf)| now.saturating_sub(pf.last_req) >= BLOCK_REREQUEST)
            .map(|(r, _)| r.clone())
            .collect();
        for r in roots {
            self.request_missing_blocks(&r, now);
        }
    }

    /// Serve an inbound `BlockRequest`: if we hold the block, reply with it to
    /// the requester (who verifies the hash before storing).
    fn handle_block_request(&mut self, opened: lifeline_core::message::Opened, now: u64) {
        let Some(body) = opened.payload.body.as_ref() else {
            return;
        };
        let Ok(cid_raw) = b64url_decode(body) else {
            return;
        };
        let cid = Bytes::new(cid_raw);
        let Some(block) = self.blocks.get(&cid).cloned() else {
            return;
        };
        // Carry the block bytes *binary* in the attachment field — base64 in the
        // string body would inflate a 64 KiB block by a third and blow past MTUs.
        let payload = Payload {
            kind: PayloadKind::BlockResponse,
            body: None,
            coords: None,
            battery_pct: None,
            attach: Some(AttachChunk {
                id: cid.to_b64url(),
                idx: 0,
                total: 1,
                bytes: Bytes::new(block),
            }),
            group_id: None,
        };
        let opts = SealOptions::normal(now).with_priority(Priority::Bulk);
        if let Ok(b) = seal_bundle(&self.identity, &opened.sender, &payload, &opts) {
            self.originate(b, now);
        }
    }

    /// Store a block that arrived in a `BlockResponse`, after verifying it hashes
    /// to its claimed CID, then complete any fetch it finishes.
    fn handle_block_response(&mut self, opened: lifeline_core::message::Opened) {
        let Some(attach) = opened.payload.attach.as_ref() else {
            return;
        };
        let Ok(cid_raw) = b64url_decode(&attach.id) else {
            return;
        };
        let cid = Bytes::new(cid_raw);
        // Intrinsic integrity: trust the block only if it hashes to the CID.
        if cid_of(&attach.bytes.0) != cid {
            return;
        }
        // Solicited-only: accept a block only if some pending fetch actually needs
        // it. Otherwise an attacker could push unlimited unrequested blocks (each
        // with a valid self-CID) to fill our store.
        let wanted = self
            .pending_fetches
            .values()
            .any(|pf| pf.manifest.blocks.contains(&cid));
        if !wanted {
            return;
        }
        self.blocks.put(attach.bytes.0.clone());
        self.try_complete_fetches();
    }

    /// Reassemble and surface any pending fetch whose blocks are all present.
    fn try_complete_fetches(&mut self) {
        let mut done = Vec::new();
        for (root, pf) in &self.pending_fetches {
            if self.blocks.missing(&pf.manifest).is_empty() {
                if let Ok(obj) = self.blocks.reassemble(&pf.manifest) {
                    self.fetched.push((root.clone(), obj));
                }
                done.push(root.clone());
            }
        }
        for r in done {
            self.pending_fetches.remove(&r);
        }
    }

    /// Broadcast an "I'm safe" message to every known contact (FR-41). Returns
    /// the ids of the bundles submitted.
    pub fn broadcast_safe(&mut self, note: Option<String>, now: u64) -> Vec<Bytes> {
        let recipients: Vec<IdentityPublic> = self
            .contacts
            .values()
            .filter(|p| p.id != self.public.id)
            .cloned()
            .collect();
        let payload = Payload {
            kind: PayloadKind::Safe,
            body: note,
            coords: None,
            battery_pct: None,
            attach: None,
            group_id: None,
        };
        recipients
            .into_iter()
            .map(|r| self.submit(&r, payload.clone(), Priority::Alert, now))
            .collect()
    }

    /// Broadcast a plain text message to every known contact — "ask the mesh to
    /// propagate this" (each copy is independently sprayed/carried/relayed).
    /// Returns the ids of the bundles submitted.
    /// Tell the engine this node's current GPS position, enabling it to receive
    /// geocasts addressed to its area (and, later, geographic routing).
    pub fn set_position(&mut self, lat: f64, lon: f64) {
        self.my_pos = Some(lifeline_geo::GeoPoint::new(lat, lon));
    }

    /// This node's current position, if set.
    pub fn position(&self) -> Option<(f64, f64)> {
        self.my_pos.map(|p| (p.lat, p.lon))
    }

    /// Provide an authoritative time (e.g. from a GPS fix), making this node a
    /// stratum-1 reference that disciplines its neighbours' clocks. `gps_unix` and
    /// `local_now` are both unix seconds.
    pub fn set_gps_time(&mut self, gps_unix: i64, local_now: u64) {
        self.time.set_reference(gps_unix, local_now as i64);
    }

    /// The node's best estimate of network ("mesh") time in unix seconds, or
    /// `None` if it has no fix yet. **Advisory** — coarse display/timestamp use
    /// only; never gate expiry or security on it (beacons are unauthenticated).
    pub fn mesh_time(&self, local_now: u64) -> Option<i64> {
        self.time
            .has_fix()
            .then(|| self.time.corrected(local_now as i64))
    }

    /// **Geocast** a payload to everyone within `radius_m` of (`lat`, `lon`) —
    /// region addressing (FR — "SOS to anyone near here"). The message is sealed
    /// to a key derived from each covered geohash cell, so any node *in* that cell
    /// can open it; the sender need not know the recipients or hold their keys.
    /// Sender identity is authenticated (sealed-sender). Returns the bundle ids.
    pub fn broadcast_geo(
        &mut self,
        lat: f64,
        lon: f64,
        radius_m: f64,
        payload: Payload,
        now: u64,
    ) -> Vec<Bytes> {
        // Clamp the radius and cap the covered-cell count so a single call can't
        // become a mesh-wide amplifier (each cell seals a widely-sprayed bundle).
        let radius_m = radius_m.clamp(0.0, MAX_GEOCAST_RADIUS_M);
        let center = lifeline_geo::GeoPoint::new(lat, lon);
        let mut cells = lifeline_geo::cells_covering(center, radius_m, GEOCAST_PRECISION);
        cells.truncate(MAX_GEOCAST_CELLS);
        let mut ids = Vec::new();
        for cell in cells {
            let recipient = lifeline_core::geocast::region_recipient(&cell);
            let mut opts = SealOptions::normal(now).with_priority(Priority::Alert);
            // Spread widely so it reaches the whole region, not one path.
            opts.copies_left = GEOCAST_COPIES;
            if let Ok(bundle) = seal_bundle(&self.identity, &recipient, &payload, &opts) {
                let id = bundle.bundle_id.clone();
                self.originate(bundle, now);
                ids.push(id);
            }
        }
        ids
    }

    /// If `bundle` is a geocast for the region this node is currently in, open it
    /// with the derived region key and deliver it (once). A regional broadcast has
    /// no single recipient, so it is never terminated by unicast delivery — it
    /// keeps spreading so every node in the region gets it.
    fn try_geocast_deliver(&mut self, bundle: &Bundle) {
        let Some(pos) = self.my_pos else { return };
        if self.origin_ids.contains(&bundle.bundle_id) {
            return; // our own geocast
        }
        let my_cell = lifeline_geo::encode(pos, GEOCAST_PRECISION);
        if bundle.dst != lifeline_core::geocast::region_dst(&my_cell) {
            return; // not addressed to our region cell
        }
        if self.geocast_seen.len() >= MAX_GEOCAST_SEEN {
            self.geocast_seen.clear(); // bound memory under a unique-id flood
        }
        if !self.geocast_seen.insert(bundle.bundle_id.clone()) {
            return; // already delivered
        }
        if let Ok(opened) = lifeline_core::geocast::open_region(&my_cell, bundle) {
            // Moderation applies to geocasts too (FR-48).
            if self.state.is_blocked(&opened.sender.id) {
                return;
            }
            self.inbox.push(Inbound {
                from: opened.sender.id.clone(),
                payload: opened.payload,
                group: None,
            });
        }
    }

    pub fn broadcast_text(&mut self, body: &str, priority: Priority, now: u64) -> Vec<Bytes> {
        let recipients: Vec<IdentityPublic> = self
            .contacts
            .values()
            .filter(|p| p.id != self.public.id)
            .cloned()
            .collect();
        recipients
            .into_iter()
            .map(|r| {
                let payload = Payload {
                    kind: PayloadKind::Text,
                    body: Some(body.to_string()),
                    coords: None,
                    battery_pct: None,
                    attach: None,
                    group_id: None,
                };
                self.submit(&r, payload, priority, now)
            })
            .collect()
    }

    /// One-tap SOS (FR-40): highest-priority distress message with GPS and
    /// battery, fanned out to every known contact so it takes any path out.
    /// Returns the ids of the bundles submitted.
    pub fn broadcast_sos(
        &mut self,
        coords: Option<Coords>,
        battery_pct: Option<u8>,
        note: Option<String>,
        now: u64,
    ) -> Vec<Bytes> {
        let recipients: Vec<IdentityPublic> = self
            .contacts
            .values()
            .filter(|p| p.id != self.public.id)
            .cloned()
            .collect();
        let payload = Payload {
            kind: PayloadKind::Sos,
            body: note,
            coords,
            battery_pct,
            attach: None,
            group_id: None,
        };
        recipients
            .into_iter()
            .map(|r| self.submit(&r, payload.clone(), Priority::Sos, now))
            .collect()
    }

    /// Share the sender's location with a known address (FR-43). Returns the
    /// bundle id, or `None` if the recipient's key is unknown.
    pub fn submit_location(
        &mut self,
        to: &Address,
        lat: f64,
        lon: f64,
        acc_m: u32,
        now: u64,
    ) -> Option<Bytes> {
        let payload = Payload {
            kind: PayloadKind::Location,
            body: None,
            coords: Some(Coords { lat, lon, acc_m }),
            battery_pct: None,
            attach: None,
            group_id: None,
        };
        self.submit_to_addr(to, payload, Priority::Normal, now)
    }

    /// Block a key: its messages are dropped at this endpoint (FR-48). The
    /// blocklist is a CRDT, so the block converges across your own devices.
    pub fn block(&mut self, who: Address) {
        self.state.block(who);
    }

    /// Unblock a previously-blocked key (FR-48).
    pub fn unblock(&mut self, who: &Address) {
        self.state.unblock(who);
    }

    /// Is this key currently blocked at this endpoint?
    pub fn is_blocked(&self, who: &Address) -> bool {
        self.state.is_blocked(who)
    }

    // --- Group messaging (FR-12) ---

    /// Create a group with this node as a member and a fresh sender key.
    pub fn create_group(&mut self, group_id: &str) {
        let me = self.public.id.clone();
        self.groups.entry(group_id.to_string()).or_insert_with(|| {
            let mut d = HashSet::new();
            d.insert(me.clone());
            GroupState {
                sender_key: SenderKeyState::new(group_id.to_string()),
                distributed_to: d,
            }
        });
        self.state.add_member(group_id, me);
    }

    /// Add a member (learning its key) to a group. The sender key is distributed
    /// to them on the next [`NodeEngine::send_group`].
    pub fn add_group_member(&mut self, group_id: &str, member: IdentityPublic) {
        self.state.add_member(group_id, member.id.clone());
        self.contacts.insert(member.id.clone(), member);
    }

    /// Current membership of a group.
    pub fn group_members(&self, group_id: &str) -> Vec<Address> {
        self.state.members(group_id)
    }

    /// Encrypt `payload` once and fan it out to every group member (FR-12).
    /// Distributes our sender key first to any member that lacks it. Returns the
    /// ids of all bundles submitted.
    pub fn send_group(&mut self, group_id: &str, payload: Payload, now: u64) -> Vec<Bytes> {
        self.create_group(group_id); // ensure a sender key + membership exist
        let me = self.public.id.clone();
        let members = self.state.members(group_id);
        let recipients: Vec<IdentityPublic> = members
            .iter()
            .filter(|a| **a != me)
            .filter_map(|a| self.contacts.get(a).cloned())
            .collect();

        let mut ids = Vec::new();

        // 1. Distribute our sender key to members that don't have it yet.
        let dist = {
            let g = self.groups.get(group_id).expect("group exists");
            g.sender_key.distribution(&self.identity)
        };
        let to_distribute: Vec<IdentityPublic> = recipients
            .iter()
            .filter(|r| {
                !self
                    .groups
                    .get(group_id)
                    .unwrap()
                    .distributed_to
                    .contains(&r.id)
            })
            .cloned()
            .collect();
        for r in &to_distribute {
            if let Some(id) = self.send_group_op(r, &GroupEnvelope::Distribution(dist.clone()), now)
            {
                ids.push(id);
            }
            self.groups
                .get_mut(group_id)
                .unwrap()
                .distributed_to
                .insert(r.id.clone());
        }

        // 2. Encrypt the message once and fan it out to every recipient.
        let Ok(inner) = to_cbor(&payload) else {
            return ids;
        };
        let gmsg = {
            let g = self.groups.get_mut(group_id).unwrap();
            g.sender_key.encrypt(&self.identity, &inner)
        };
        for r in &recipients {
            if let Some(id) = self.send_group_op(r, &GroupEnvelope::Message(gmsg.clone()), now) {
                ids.push(id);
            }
        }
        ids
    }

    fn send_group_op(
        &mut self,
        to: &IdentityPublic,
        env: &GroupEnvelope,
        now: u64,
    ) -> Option<Bytes> {
        let gid = match env {
            GroupEnvelope::Distribution(d) => d.group_id.clone(),
            GroupEnvelope::Message(m) => m.group_id.clone(),
        };
        let body = b64url_encode(&to_cbor(env).ok()?);
        let payload = Payload {
            kind: PayloadKind::GroupOp,
            body: Some(body),
            coords: None,
            battery_pct: None,
            attach: None,
            group_id: Some(gid),
        };
        Some(self.submit(to, payload, Priority::Normal, now))
    }

    /// Handle an inbound group-control payload (distribution or message).
    fn handle_group_payload(&mut self, opened: &lifeline_core::message::Opened) {
        let Some(body) = &opened.payload.body else {
            return;
        };
        let Ok(raw) = b64url_decode(body) else { return };
        let Ok(env) = from_cbor::<GroupEnvelope>(&raw) else {
            return;
        };
        // Bind the group op's claimed owner to the E2E-authenticated sender of
        // the bundle it arrived in: a node may only distribute/send under its own
        // identity, never claim to be another member (anti-impersonation).
        match env {
            GroupEnvelope::Distribution(d) => {
                if d.owner != opened.sender.id {
                    return;
                }
                if let Ok(rx) = ReceiverKeyState::from_distribution(&d) {
                    let group = d.group_id.clone();
                    let owner = d.owner.clone();
                    self.receivers.insert((group.clone(), owner.clone()), rx);
                    self.state.add_member(&group, owner.clone());
                    self.drain_pending(&group, &owner);
                }
            }
            GroupEnvelope::Message(m) => {
                if m.owner != opened.sender.id {
                    return;
                }
                self.handle_group_message(m)
            }
        }
    }

    fn handle_group_message(&mut self, m: GroupMessage) {
        let key = (m.group_id.clone(), m.owner.clone());
        let has_key = self.receivers.contains_key(&key);
        let decrypted = self
            .receivers
            .get_mut(&key)
            .and_then(|rx| rx.decrypt(&m).ok());
        match decrypted {
            Some(pt) => {
                if let Ok(inner) = from_cbor::<Payload>(&pt) {
                    // Block enforcement also applies to group messages.
                    if self.state.is_blocked(&m.owner) {
                        return;
                    }
                    // Thread on the *authenticated* sender-key group (`m.group_id`),
                    // not the inner payload's self-asserted `group_id` — a member
                    // must not cross-post a message into a different group thread.
                    self.inbox.push(Inbound {
                        from: m.owner.clone(),
                        payload: inner,
                        group: Some(m.group_id.clone()),
                    });
                }
            }
            // Distribution not arrived yet — buffer (bounded) and retry later.
            None if !has_key && self.pending_group.len() < 4096 => {
                self.pending_group
                    .push((m.group_id.clone(), m.owner.clone(), m));
            }
            None => {} // had the key (or buffer full) — drop
        }
    }

    fn drain_pending(&mut self, group: &str, owner: &Address) {
        let mut still = Vec::new();
        let drained: Vec<GroupMessage> = std::mem::take(&mut self.pending_group)
            .into_iter()
            .filter_map(|(g, o, m)| {
                if g == group && &o == owner {
                    Some(m)
                } else {
                    still.push((g, o, m));
                    None
                }
            })
            .collect();
        self.pending_group = still;
        for m in drained {
            self.handle_group_message(m);
        }
    }

    fn next_mid(&mut self) -> Bytes {
        self.mid_counter += 1;
        let mut m = vec![0u8; 16];
        m[..8].copy_from_slice(&self.mid_counter.to_le_bytes());
        Bytes::new(m)
    }

    /// One scheduler step: advertise, receive, and offer over every interface.
    pub fn tick(&mut self, now: u64) {
        if now >= self.next_prekey_rotate {
            self.prekey_ring.rotate();
            self.next_prekey_rotate = now + self.prekey_rotate_interval;
        }
        self.gateway_round(now);
        self.advertise_and_receive(now);
        self.forward_pending_onions(now);
        self.retry_fetches(now);
        self.offer_round(now);
        self.arq_pump(now);
        self.retry_unverified(now);
        self.router.attribution_tick(now);
        self.router.tick(now);
    }

    /// FR-36/FR-37: if we're a gateway, periodically emit our own signed announce
    /// (so the mesh forms a gradient toward us) and push mesh-originated bundles
    /// onto every off-mesh uplink so they escape to the wider network.
    fn gateway_round(&mut self, now: u64) {
        if !self.router.is_gateway() {
            return;
        }
        if now >= self.next_announce {
            self.announce_seq += 1;
            let ann = make_gateway_announce(
                &self.identity,
                &self.gateway_caps,
                0.3,
                self.announce_seq,
                now + ANNOUNCE_TTL_S,
            );
            self.router.observe_announce(ann, 0, now);
            self.next_announce = now + self.announce_interval;
        }
        // Bridge bundles onto uplinks (internet/LoRa). Best-effort broadcast — the
        // DTN store keeps a copy, so a lost frame is retried by the normal offer.
        let bridged = self.router.drain_bridge();
        if bridged.is_empty() {
            return;
        }
        for p in 0..self.ports.len() {
            if !self.ports[p].iface.caps().bridges_offmesh {
                continue;
            }
            let usable = self.ports[p].iface.caps().usable_mtu();
            for b in &bridged {
                let Ok(bytes) = to_cbor(b) else { continue };
                let mid = self.next_mid();
                if let Ok(frames) = Fragmenter::fragment(FrameKind::Bundle, mid, &bytes, usable) {
                    for f in &frames {
                        if let Ok(enc) = f.encode() {
                            let _ = self.ports[p].iface.broadcast(&enc);
                        }
                    }
                }
            }
        }
    }

    /// FR-49: forward any buffered onion layers whose next hop we now know (its
    /// key arrived on a beacon after we peeled the layer).
    fn forward_pending_onions(&mut self, now: u64) {
        if self.pending_onion.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending_onion);
        for (next, inner, priority) in pending {
            match self.contacts.get(&next).cloned() {
                Some(next_pub) => {
                    self.submit_onion_hop(&next_pub, inner, priority, now);
                }
                None => self.pending_onion.push((next, inner, priority)),
            }
        }
    }

    /// Lossy-link ARQ (Dhwani): retransmit fragments of unicast bundles that
    /// haven't been acknowledged within the RTO, and drop any message whose
    /// retransmit budget is spent (the DTN re-spray is the coarse backstop).
    fn arq_pump(&mut self, now: u64) {
        let (rto, max_rounds) = (self.arq_rto, self.arq_max_rounds);
        for p in 0..self.ports.len() {
            let due = self.ports[p].tx.due(now, rto, max_rounds);
            for (peer, enc) in due {
                let _ = self.ports[p].iface.send(peer, &enc);
            }
        }
    }

    /// Adaptive retry (FR-32): re-spray any of our messages that remain
    /// unverified past the retry window (up to `max_retries`), so delivery is
    /// retried on new paths rather than silently stalling.
    fn retry_unverified(&mut self, now: u64) {
        let (window, max, copies) = (self.retry_window, self.max_retries, self.respray_copies);
        let mut to_respray = Vec::new();
        for rec in &mut self.sent {
            if !rec.verified && rec.retries < max && now.saturating_sub(rec.submitted_at) >= window
            {
                rec.retries += 1;
                rec.submitted_at = now;
                to_respray.push(rec.bundle_id.clone());
            }
        }
        for id in to_respray {
            if self.router.respray(&id, copies) {
                self.retries_total += 1;
            }
        }
    }

    /// Total re-sprays performed (diagnostics / tests).
    pub fn retry_count(&self) -> u64 {
        self.retries_total
    }

    /// Total lossy-link ARQ frame retransmissions across all interfaces (FR —
    /// Dhwani noise robustness; surfaced as a diagnostic).
    pub fn arq_retransmits(&self) -> u64 {
        self.ports.iter().map(|p| p.tx.retransmits()).sum()
    }

    /// Reliable messages still awaiting full acknowledgement across interfaces.
    pub fn arq_pending(&self) -> usize {
        self.ports.iter().map(|p| p.tx.pending()).sum()
    }

    fn advertise_and_receive(&mut self, now: u64) {
        // Beacon carries our gateway status + current gradient so neighbours can
        // route bundles *downhill* toward the nearest gateway (FR-29/FR-36), plus
        // our current forward-secret prekey so senders seal to it (FR-44).
        let mut beacon_id = self.public.clone();
        beacon_id.prekey = self.prekey_ring.publish(&self.identity);
        let beacon = to_cbor(&Beacon {
            id: beacon_id,
            gw: self.router.is_gateway(),
            grad: self.router.gradient(now),
            // Share our time only if we have a fresh fix (else `None`).
            ts: self.time.advertise(now as i64, TIME_STALE_S),
        })
        .expect("cbor beacon");
        // Announces to gossip onward — throttled to at most once per
        // `announce_interval` so a cache of announces isn't re-broadcast every
        // tick (bandwidth + amplification bound).
        let announces = if now >= self.next_gossip {
            self.next_gossip = now + self.announce_interval;
            self.router.announces_to_gossip(now)
        } else {
            Vec::new()
        };
        // Held-bundle digest (anti-entropy): broadcast our stored bundle ids so
        // neighbours can suppress re-offering bundles we already hold. Throttled,
        // and — the differential idea — sent only when our set's fingerprint has
        // actually moved since the last digest.
        let digest_bytes = if now >= self.next_digest {
            self.next_digest = now + DIGEST_INTERVAL;
            let mut ids: Vec<Bytes> = self.router.known_ids().into_iter().collect();
            ids.sort_unstable();
            ids.truncate(MAX_DIGEST_IDS);
            let fp = lifeline_reconcile::fingerprint(&ids);
            if !ids.is_empty() && fp != self.last_digest_fp {
                self.last_digest_fp = fp;
                to_cbor(&ids).ok()
            } else {
                None
            }
        } else {
            None
        };
        for p in 0..self.ports.len() {
            // Advertise our identity beacon.
            let mid = self.next_mid();
            let usable = self.ports[p].iface.caps().usable_mtu();
            if let Ok(frames) = Fragmenter::fragment(FrameKind::Beacon, mid, &beacon, usable) {
                for f in &frames {
                    if let Ok(enc) = f.encode() {
                        let _ = self.ports[p].iface.broadcast(&enc);
                    }
                }
            }
            // Gossip each known gateway announce a few hops outward (FR-36).
            for (ann, dist) in &announces {
                if let Ok(bytes) = to_cbor(&(ann, dist)) {
                    let mid = self.next_mid();
                    if let Ok(frames) =
                        Fragmenter::fragment(FrameKind::Announce, mid, &bytes, usable)
                    {
                        for f in &frames {
                            if let Ok(enc) = f.encode() {
                                let _ = self.ports[p].iface.broadcast(&enc);
                            }
                        }
                    }
                }
            }
            // Broadcast our held-bundle digest, if we have a fresh one.
            if let Some(ref dbytes) = digest_bytes {
                let mid = self.next_mid();
                if let Ok(frames) = Fragmenter::fragment(FrameKind::Digest, mid, dbytes, usable) {
                    for f in &frames {
                        if let Ok(enc) = f.encode() {
                            let _ = self.ports[p].iface.broadcast(&enc);
                        }
                    }
                }
            }
            // Receive whatever arrived.
            let inbound = self.ports[p].iface.poll();
            for (peer, raw) in inbound {
                let Ok(frame) = Frame::decode(&raw) else {
                    continue;
                };
                // A selective ACK prunes our retransmit queue; it is never
                // reassembled as a payload.
                if frame.kind == FrameKind::Ack {
                    self.ports[p].tx.on_ack(peer, &frame);
                    continue;
                }
                let out = self.ports[p].rx.accept(frame);
                // Send any selective ACK straight back to the source peer so it
                // stops retransmitting the fragments we already hold.
                if let Some(ack) = out.ack {
                    let _ = self.ports[p].iface.send(peer, &ack);
                }
                if let Some((kind, payload)) = out.payload {
                    self.handle_payload(p, peer, kind, payload, now);
                }
            }
        }
    }

    fn offer_round(&mut self, now: u64) {
        for p in 0..self.ports.len() {
            let peers = self.ports[p].iface.scan();
            let usable = self.ports[p].iface.caps().usable_mtu();
            // Bandwidth-adaptive routing: over a low-throughput bearer, hold back
            // bulky NORMAL/BULK bundles (they wait for a fatter link); SOS/ALERT
            // and final-hop delivery always go (see `offer_to`).
            let soft_max_bytes = bearer_soft_cap(self.ports[p].iface.caps().throughput);
            for peer in peers {
                let Some(addr) = self.peer_addr.get(&(p, peer)).cloned() else {
                    continue; // haven't learned this peer's address yet
                };
                // Route with what the neighbour's beacon told us: is it a gateway,
                // and is its gradient better than ours? (offer_to sends the last
                // copy downhill toward a gateway.)
                let meta = self.peer_meta.get(&(p, peer)).cloned().unwrap_or_default();
                let peer_info = PeerInfo {
                    addr,
                    is_gateway: meta.is_gateway,
                    gradient: meta.gradient,
                    // Anti-entropy: don't re-offer bundles this peer's digest says
                    // it already holds (retires the previously-inert `known`).
                    known: self
                        .peer_bundles
                        .get(&(p, peer))
                        .cloned()
                        .unwrap_or_default(),
                    soft_max_bytes,
                };
                // Offer DTN bundles (spray-and-wait decisions inside).
                let offers = self.router.offer_to(&peer_info, now);
                for b in offers {
                    if let Ok(bytes) = to_cbor(&b) {
                        self.send_unit(p, peer, FrameKind::Bundle, &bytes, usable, now);
                    }
                }
                // Anti-entropy: push our CRDT state (§12.3, FR-33). Best-effort —
                // it is periodic and self-healing, so it skips ARQ.
                if let Ok(bytes) = to_cbor(&self.state) {
                    self.send_unit(p, peer, FrameKind::State, &bytes, usable, now);
                }
            }
        }
    }

    fn send_unit(
        &mut self,
        p: usize,
        peer: PeerId,
        kind: FrameKind,
        payload: &[u8],
        usable: usize,
        now: u64,
    ) {
        let mid = self.next_mid();
        let Ok(frames) = Fragmenter::fragment(kind, mid.clone(), payload, usable) else {
            return;
        };
        // Encode once; send now, and for reliable (Bundle) traffic hand the
        // encoded frames to ARQ so lost fragments are retransmitted until acked.
        let mut encoded = Vec::with_capacity(frames.len());
        for f in &frames {
            if let Ok(enc) = f.encode() {
                let _ = self.ports[p].iface.send(peer, &enc);
                encoded.push(enc);
            }
        }
        if kind == FrameKind::Bundle {
            self.ports[p].tx.track(peer, mid, encoded, now);
        }
    }

    fn handle_payload(
        &mut self,
        p: usize,
        peer: PeerId,
        kind: FrameKind,
        payload: Vec<u8>,
        now: u64,
    ) {
        match kind {
            FrameKind::Beacon => {
                if let Ok(b) = from_cbor::<Beacon>(&payload) {
                    // Bound discovered-peer growth against a beacon flood of
                    // fabricated identities: only learn a *new* peer if we have
                    // room; already-known peers keep updating.
                    let known = self.contacts.contains_key(&b.id.id);
                    // Discipline our clock only to *explicitly paired* contacts
                    // (added out-of-band via invite/QR) — never a peer merely
                    // auto-discovered from a beacon, so a stranger can't skew our
                    // time. The crate's bounded slew limits damage even so; this
                    // stays advisory (display/timestamps), not a TTL/security input.
                    if self.paired.contains(&b.id.id) {
                        if let Some(tb) = b.ts {
                            self.time.observe(tb, now as i64, TIME_STALE_S);
                        }
                    }
                    if known || self.contacts.len() < MAX_CONTACTS {
                        self.peer_addr.insert((p, peer), b.id.id.clone());
                        self.peer_meta.insert(
                            (p, peer),
                            PeerMeta {
                                is_gateway: b.gw,
                                gradient: b.grad,
                            },
                        );
                        match self.contacts.entry(b.id.id.clone()) {
                            std::collections::hash_map::Entry::Occupied(mut e) => {
                                // Learn the peer's *current* prekey on each beacon
                                // (its long-term keys stay TOFU-fixed).
                                e.get_mut().prekey = b.id.prekey;
                            }
                            std::collections::hash_map::Entry::Vacant(e) => {
                                e.insert(b.id);
                            }
                        }
                    }
                }
            }
            FrameKind::Announce => {
                if let Ok((ann, dist)) = from_cbor::<(GatewayAnnounce, u16)>(&payload) {
                    self.observe_announce(ann, dist, now);
                }
            }
            FrameKind::Digest => {
                // Record which bundle ids this peer holds, to suppress re-offering
                // them (anti-entropy). Bounded so a peer can't inflate our state.
                if let Ok(ids) = from_cbor::<Vec<Bytes>>(&payload) {
                    let set: HashSet<Bytes> = ids.into_iter().take(MAX_DIGEST_IDS).collect();
                    self.peer_bundles.insert((p, peer), set);
                }
            }
            FrameKind::Bundle => {
                if let Ok(b) = from_cbor::<Bundle>(&payload) {
                    let src = self.peer_addr.get(&(p, peer)).cloned();
                    self.handle_ingest(b, src, now);
                }
            }
            FrameKind::State => {
                if let Ok(s) = from_cbor::<SharedState>(&payload) {
                    self.state.merge(&s);
                }
            }
            // ACKs are intercepted before reassembly; they never reach here.
            FrameKind::Ack => {}
        }
    }

    /// Record a gossiped gateway announce (FR-36), only if it is authentic.
    /// The announce is **self-verifying** (carries the gateway's key bound to its
    /// address), so we reject *every* forged announce — closing the gradient-
    /// poisoning / cache-flood vector — without needing the gateway as a contact.
    /// We also refuse an implausibly far-future expiry so a forged announce can't
    /// pin a cache entry indefinitely (the signature covers `expires_at`, so we
    /// reject rather than clamp).
    fn observe_announce(&mut self, ann: GatewayAnnounce, dist: u16, now: u64) {
        if verify_gateway_announce(&ann).is_err() {
            return;
        }
        if ann.expires_at > now + 4 * ANNOUNCE_TTL_S {
            return;
        }
        self.router.observe_announce(ann, dist, now);
    }

    fn handle_ingest(&mut self, bundle: Bundle, src: Option<Address>, now: u64) {
        match self.router.ingest(bundle.clone(), now) {
            IngestOutcome::Delivered => self.on_delivered(bundle, now),
            // We're now carrying this for someone else. It may also be a geocast
            // for our region (delivered locally *and* kept spreading). If we're a
            // committed custodian, sign for it so the previous hop frees its copy.
            IngestOutcome::Stored => {
                self.try_geocast_deliver(&bundle);
                self.maybe_take_custody(&bundle, src, now);
            }
            _ => {}
        }
    }

    /// FR-25 (custodian side): a committed custodian that just stored a *relayed*
    /// bundle signs a custody receipt back to the previous hop.
    fn maybe_take_custody(&mut self, bundle: &Bundle, src: Option<Address>, now: u64) {
        if self.custody_role != CustodyRole::Custodian {
            return;
        }
        // Never take custody of a bundle we originated (we track its delivery).
        if self.origin_ids.contains(&bundle.bundle_id) {
            return;
        }
        let Some(src) = src else { return };
        let Some(prev_hop) = self.contacts.get(&src).cloned() else {
            return;
        };
        let receipt =
            lifeline_core::receipt::make_custody_receipt(&self.identity, &bundle.bundle_id, now);
        let Ok(rbytes) = to_cbor(&receipt) else {
            return;
        };
        let payload = Payload {
            kind: PayloadKind::Custody,
            body: Some(b64url_encode(&rbytes)),
            coords: None,
            battery_pct: None,
            attach: None,
            group_id: None,
        };
        let opts = SealOptions::normal(now).with_priority(Priority::Alert);
        if let Ok(cb) = seal_bundle(&self.identity, &prev_hop, &payload, &opts) {
            self.originate(cb, now);
        }
    }

    /// Handle a bundle addressed to us: reassemble erasure fragments if needed,
    /// then dispatch on payload type. Recognising a receipt by its *content*
    /// (not a pre-shared id set) is what lets a node verify its own messages and
    /// never acknowledge an acknowledgement.
    fn on_delivered(&mut self, bundle: Bundle, now: u64) {
        // Erasure fragment (FR-28): buffer until `k` shards reconstruct, then
        // process the reconstructed original.
        if bundle.frag.is_some() {
            if let Some(recon) = self.frag_collector.accept(&bundle) {
                self.on_delivered(recon, now);
            }
            return;
        }
        // Open with our long-term key; if that fails, the sender may have sealed
        // to one of our rotating forward-secret prekeys — try the ring (FR-44).
        let opened = match open_bundle(&self.identity, &bundle) {
            Ok(o) => o,
            Err(_) => match self.prekey_ring.open_bundle(&self.public.id, &bundle) {
                Some(o) => o,
                None => return,
            },
        };
        // Exhaustive dispatch: no wildcard, so adding a `PayloadKind` variant is a
        // compile error until it is routed here — closing the silent-misroute gap
        // where a new control type would have fallen through to `deliver_message`.
        match opened.payload.kind {
            // Control payloads handled internally (never surfaced to the app).
            PayloadKind::Receipt => self.process_receipt(opened),
            PayloadKind::Custody => self.process_custody(opened, now),
            PayloadKind::Onion => self.handle_onion(opened, bundle.priority, now),
            PayloadKind::BlockRequest => self.handle_block_request(opened, now),
            PayloadKind::BlockResponse => self.handle_block_response(opened),
            PayloadKind::HaveQuery => self.handle_have_query(opened, now),
            PayloadKind::HaveReply => self.handle_have_reply(opened, now),
            // Application payloads delivered to the inbox (+ delivery receipt).
            PayloadKind::Text
            | PayloadKind::Sos
            | PayloadKind::Safe
            | PayloadKind::Location
            | PayloadKind::Alert
            | PayloadKind::GroupOp
            | PayloadKind::AttachChunk => self.deliver_message(&bundle, opened, now),
            // Forward-compat: a payload type a newer peer defined. Ignore it
            // (we already relayed the encrypted bundle for endpoints that do
            // understand it) rather than mis-handling it.
            PayloadKind::Unknown => {}
        }
    }

    /// FR-49: peel one onion layer. If we are an intermediate hop, re-seal the
    /// remaining onion to the revealed next hop and forward it; if we are the
    /// final recipient, deliver the inner payload. A relay learns only the next
    /// hop, and (for a multi-hop route) the recipient sees only the last relay
    /// as sender — the origin stays hidden.
    fn handle_onion(
        &mut self,
        opened: lifeline_core::message::Opened,
        priority: Priority,
        now: u64,
    ) {
        let Some(body) = opened.payload.body.as_ref() else {
            return;
        };
        let Ok(onion) = b64url_decode(body) else {
            return;
        };
        match peel_onion(&self.identity, &onion) {
            Ok(Peeled::Forward { next, inner }) => {
                // Forward if we already know the next hop's key; otherwise buffer
                // it (bounded) and forward once we learn the hop via a beacon —
                // frames from another interface may arrive later this same tick.
                if let Some(next_pub) = self.contacts.get(&next).cloned() {
                    self.submit_onion_hop(&next_pub, inner, priority, now);
                } else if self.pending_onion.len() < 4096 {
                    self.pending_onion.push((next, inner, priority));
                }
            }
            Ok(Peeled::Deliver { payload }) => {
                if let Ok(real) = from_cbor::<Payload>(&payload) {
                    // Moderation applies to onion-delivered messages too (FR-48).
                    if self.state.is_blocked(&opened.sender.id) {
                        return;
                    }
                    // The origin is hidden by construction; the visible "from" is
                    // the immediate previous hop (the last relay, or the sender
                    // on a direct send).
                    self.inbox.push(Inbound {
                        from: opened.sender.id.clone(),
                        payload: real,
                        group: None,
                    });
                }
            }
            Err(_) => {}
        }
    }

    /// FR-25 (carrier side): a committed custodian has signed for a bundle we are
    /// relaying, so free our copy — provided the signature verifies and the
    /// bundle is a relayed copy (never one we originated).
    fn process_custody(&mut self, opened: lifeline_core::message::Opened, now: u64) {
        let Some(body) = opened.payload.body else {
            return;
        };
        let Ok(raw) = b64url_decode(&body) else {
            return;
        };
        let Ok(cr) = from_cbor::<lifeline_proto::CustodyReceipt>(&raw) else {
            return;
        };
        // The custodian is the sender; verify the receipt under their signing key.
        if lifeline_core::receipt::verify_custody_receipt(&cr, opened.sender.sign_pub.as_slice())
            .is_err()
        {
            return;
        }
        if self.origin_ids.contains(&cr.bundle_id) {
            // Our own message: we keep our copy until delivery is verified, but we
            // *record* that this peer committed to carrying it. If our sealed
            // delivery receipt never comes back (only we can read it), that is a
            // black-hole signal against this custodian (FR-47).
            self.router.note_custody(
                &cr.bundle_id,
                &opened.sender.id,
                now + ATTRIBUTION_WINDOW_SECS,
            );
            return;
        }
        self.router
            .release_custody(&cr.bundle_id, &opened.sender.id);
    }

    fn deliver_message(
        &mut self,
        bundle: &Bundle,
        opened: lifeline_core::message::Opened,
        now: u64,
    ) {
        // Endpoint moderation (FR-48): silently drop messages from blocked keys —
        // no inbox entry and no receipt (so a blocked sender learns nothing).
        if self.state.is_blocked(&opened.sender.id) {
            return;
        }
        // Group control payload (FR-12): decode + decrypt the inner message
        // rather than surfacing the raw envelope.
        if opened.payload.kind == PayloadKind::GroupOp {
            self.handle_group_payload(&opened);
        } else {
            self.inbox.push(Inbound {
                from: opened.sender.id.clone(),
                payload: opened.payload.clone(),
                group: None, // a direct 1:1 message is never a group thread
            });
        }
        self.state.mark_delivered(bundle.bundle_id.clone());

        // Emit a signed delivery receipt back to the (now-known) sender.
        let receipt = make_delivery_receipt(&self.identity, &bundle.bundle_id, now);
        let Ok(rbytes) = to_cbor(&receipt) else {
            return;
        };
        let payload = Payload {
            kind: lifeline_proto::PayloadKind::Receipt,
            body: Some(b64url_encode(&rbytes)),
            coords: None,
            battery_pct: None,
            attach: None,
            group_id: None,
        };
        let opts = SealOptions::normal(now).with_priority(Priority::Alert);
        if let Ok(rb) = seal_bundle(&self.identity, &opened.sender, &payload, &opts) {
            self.originate(rb, now);
        }
    }

    fn process_receipt(&mut self, opened: lifeline_core::message::Opened) {
        let Some(body) = opened.payload.body else {
            return;
        };
        let Ok(raw) = b64url_decode(&body) else {
            return;
        };
        let Ok(dr) = from_cbor::<DeliveryReceipt>(&raw) else {
            return;
        };
        let recipient_pub = opened.sender.sign_pub.clone();
        let mut verified = false;
        if let Some(rec) = self.sent.iter_mut().find(|s| s.bundle_id == dr.bundle_id) {
            if verify_delivery(&rec.original, &dr, recipient_pub.as_slice()).is_ok() {
                rec.verified = true;
                verified = true;
            }
        }
        if verified {
            // Delivery confirmed → credit every peer that took custody of this
            // bundle for us (they demonstrably contributed) (FR-47).
            self.router.note_delivery(&dr.bundle_id);
        }
    }

    // --- App-facing accessors ---

    /// Drain messages delivered to the application since the last call.
    pub fn take_inbox(&mut self) -> Vec<Inbound> {
        std::mem::take(&mut self.inbox)
    }

    /// How many of *our* sent messages have a verified delivery receipt.
    pub fn verified_count(&self) -> usize {
        self.sent.iter().filter(|s| s.verified).count()
    }

    /// Number of interfaces attached (concurrent transports).
    pub fn interface_count(&self) -> usize {
        self.ports.len()
    }

    /// Names of the attached interfaces (for diagnostics/UI).
    pub fn interface_names(&self) -> Vec<String> {
        self.ports
            .iter()
            .map(|p| p.iface.caps().name.clone())
            .collect()
    }

    /// Public identities this node knows (discovered via beacons or added as
    /// contacts) — the address book / online directory for a UI (FR-7/FR-8).
    pub fn directory(&self) -> Vec<IdentityPublic> {
        self.contacts.values().cloned().collect()
    }

    /// The public identity of a known contact, if any (needed to seal to them).
    pub fn contact(&self, addr: &Address) -> Option<IdentityPublic> {
        self.contacts.get(addr).cloned()
    }

    /// Per-sent-message delivery-proof status: `(bundle_id, verified)`.
    pub fn sent_status(&self) -> Vec<(Bytes, bool)> {
        self.sent
            .iter()
            .map(|s| (s.bundle_id.clone(), s.verified))
            .collect()
    }

    /// Count of distinct peers currently discovered across all interfaces.
    pub fn peer_count(&self) -> usize {
        self.peer_addr
            .values()
            .collect::<std::collections::HashSet<_>>()
            .len()
    }

    /// Total bundle ids learned from neighbours' held-bundle digests (anti-
    /// entropy diagnostics): how many `(peer, bundle)` "already-held" facts we
    /// currently know and use to suppress redundant offers.
    pub fn peer_digest_ids(&self) -> usize {
        self.peer_bundles.values().map(|s| s.len()).sum()
    }

    /// Whether this node currently stores a copy of `bundle_id` (tests/UI).
    pub fn holds_bundle(&self, bundle_id: &Bytes) -> bool {
        self.router.holds(bundle_id)
    }

    /// Snapshot the forward-secret prekey ring for encrypted-at-rest persistence
    /// (FR-44), so a node restart doesn't lose in-flight prekey-sealed messages.
    pub fn export_prekeys(&self) -> lifeline_core::prekey::PrekeyRingState {
        self.prekey_ring.export()
    }

    /// Restore a persisted prekey ring on startup (call right after `new`).
    pub fn restore_prekeys(&mut self, state: &lifeline_core::prekey::PrekeyRingState) {
        self.prekey_ring = PrekeyRing::import(state);
    }

    /// Whether this node is operating as a gateway (FR-35).
    pub fn is_gateway(&self) -> bool {
        self.router.is_gateway()
    }

    /// This node's current gradient — hops to the nearest known gateway, or
    /// `None` if it knows of no gateway (FR-36). A gateway's own gradient is 0.
    pub fn gradient(&self, now: u64) -> Option<u16> {
        self.router.gradient(now)
    }

    /// Number of distinct gateways this node currently knows about.
    pub fn known_gateways(&self) -> usize {
        self.router.stats().known_gateways
    }

    /// Router diagnostics (queue depth, forwarded copies, drops…) — FR-53.
    pub fn router_stats(&self) -> lifeline_router::RouterStats {
        self.router.stats().clone()
    }

    /// Read-only view of the CRDT shared state (group membership, etc.).
    pub fn state(&self) -> &SharedState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut SharedState {
        &mut self.state
    }
}
