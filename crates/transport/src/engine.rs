//! The node runtime: an identity + DTN router + CRDT state driven over any set
//! of [`Interface`]s (PRD §7 L1–L5 composed; FR-22 "transports run
//! concurrently").
//!
//! This is the real, transport-independent node. Give it a BLE interface, an
//! ultrasound interface, and an internet interface, and it will advertise,
//! discover peers, and shuttle the *same* end-to-end-encrypted bundles over
//! whichever links exist — fragmenting each to that interface's MTU. Swap the
//! in-memory interfaces for real radios and nothing else changes.

use crate::arq::{ArqRx, ArqTx};
use crate::frame::{Fragmenter, Frame, FrameKind};
use crate::interface::{Interface, PeerId};
use lifeline_core::content::{chunk, cid_of, BlockStore, Manifest, DEFAULT_BLOCK_SIZE};
use lifeline_core::erasure::{fragment_bundle, FragmentCollector};
use lifeline_core::group::{GroupEnvelope, GroupMessage, ReceiverKeyState, SenderKeyState};
use lifeline_core::message::{open_bundle, seal_bundle, SealOptions};
use lifeline_core::onion::{build_onion, peel_onion, Peeled};
use lifeline_core::receipt::{make_delivery_receipt, verify_delivery};
use lifeline_core::Identity;
use lifeline_proto::codec::{b64url_decode, b64url_encode, from_cbor, to_cbor};
use lifeline_proto::{
    Address, AttachChunk, Bundle, Bytes, Coords, DeliveryReceipt, IdentityPublic, Payload,
    PayloadKind, Priority,
};
use lifeline_router::{DtnRouter, IngestOutcome, PeerInfo, RouterConfig};
use lifeline_sync::SharedState;
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
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            router: RouterConfig::default(),
            custody_role: CustodyRole::default(),
            retry_window: 60,
            max_retries: 4,
            respray_copies: 6,
            arq_rto: crate::arq::DEFAULT_RTO,
            arq_max_rounds: crate::arq::DEFAULT_MAX_ROUNDS,
        }
    }
}

/// A message delivered up to the application (decrypted, sender authenticated).
#[derive(Debug, Clone)]
pub struct Inbound {
    pub from: Address,
    pub payload: Payload,
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
    /// Content-addressed block store (FR-13): serves blocks by CID to peers.
    blocks: BlockStore,
    /// Manifests we are actively pulling blocks for, keyed by root CID.
    pending_fetches: HashMap<Bytes, PendingFetch>,
    /// Objects fully fetched and verified, awaiting the app: `(root_cid, bytes)`.
    fetched: Vec<(Bytes, Vec<u8>)>,
}

/// A content object being pulled from a provider over the mesh (FR-13).
struct PendingFetch {
    manifest: Manifest,
    /// Peer we ask for the missing blocks (the content provider).
    from: Address,
    /// Last tick we (re-)sent requests, for retransmission of lost requests.
    last_req: u64,
}

/// Ticks between re-requesting still-missing blocks of a pending fetch.
const BLOCK_REREQUEST: u64 = 15;

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
        let router = DtnRouter::new(public.id.clone(), cfg.router);
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
            blocks: BlockStore::new(),
            pending_fetches: HashMap::new(),
            fetched: Vec::new(),
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
        let bundle = seal_bundle(&self.identity, to, &payload, &opts).expect("seal");
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

    /// Whether a block is already in our local store (diagnostics / tests).
    pub fn has_block(&self, cid: &Bytes) -> bool {
        self.blocks.has(cid)
    }

    /// Begin fetching the object described by `manifest` from provider `from`
    /// (FR-13): requests each missing block by CID. Completed objects surface via
    /// [`NodeEngine::take_fetched_content`]. If we already hold every block (e.g.
    /// cached), the object completes immediately with no network traffic.
    pub fn fetch_content(&mut self, manifest: Manifest, from: Address, now: u64) {
        let root = manifest.root.clone();
        self.pending_fetches.insert(
            root.clone(),
            PendingFetch {
                manifest,
                from,
                last_req: 0,
            },
        );
        self.try_complete_fetches();
        self.request_missing_blocks(&root, now);
    }

    /// Objects fully fetched-and-verified since the last call: `(root_cid, bytes)`.
    pub fn take_fetched_content(&mut self) -> Vec<(Bytes, Vec<u8>)> {
        std::mem::take(&mut self.fetched)
    }

    /// Send a `BlockRequest` for every still-missing block of a pending fetch to
    /// its provider (bulk priority — never competes with SOS).
    fn request_missing_blocks(&mut self, root: &Bytes, now: u64) {
        let Some(pf) = self.pending_fetches.get(root) else {
            return;
        };
        let from = pf.from.clone();
        let missing = self.blocks.missing(&pf.manifest);
        if missing.is_empty() {
            return;
        }
        let Some(provider) = self.contacts.get(&from).cloned() else {
            return; // provider's key unknown yet — a retry will pick it up
        };
        for cid in missing {
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
        match env {
            GroupEnvelope::Distribution(d) => {
                if let Ok(rx) = ReceiverKeyState::from_distribution(&d) {
                    let group = d.group_id.clone();
                    let owner = d.owner.clone();
                    self.receivers.insert((group.clone(), owner.clone()), rx);
                    self.state.add_member(&group, owner.clone());
                    self.drain_pending(&group, &owner);
                }
            }
            GroupEnvelope::Message(m) => self.handle_group_message(m),
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
                    self.inbox.push(Inbound {
                        from: m.owner.clone(),
                        payload: inner,
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
        self.advertise_and_receive(now);
        self.forward_pending_onions(now);
        self.retry_fetches(now);
        self.offer_round(now);
        self.arq_pump(now);
        self.retry_unverified(now);
        self.router.tick(now);
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
        let beacon = to_cbor(&self.public).expect("cbor beacon");
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
            for peer in peers {
                let Some(addr) = self.peer_addr.get(&(p, peer)).cloned() else {
                    continue; // haven't learned this peer's address yet
                };
                let peer_info = PeerInfo {
                    addr,
                    is_gateway: false,
                    gradient: None,
                    known: HashSet::new(),
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
                if let Ok(pubid) = from_cbor::<IdentityPublic>(&payload) {
                    self.peer_addr.insert((p, peer), pubid.id.clone());
                    self.contacts.entry(pubid.id.clone()).or_insert(pubid);
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

    fn handle_ingest(&mut self, bundle: Bundle, src: Option<Address>, now: u64) {
        match self.router.ingest(bundle.clone(), now) {
            IngestOutcome::Delivered => self.on_delivered(bundle, now),
            // We're now carrying this for someone else. If we're a committed
            // custodian, sign for it so the previous hop can free its copy.
            IngestOutcome::Stored => self.maybe_take_custody(&bundle, src, now),
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
        let Ok(opened) = open_bundle(&self.identity, &bundle) else {
            return;
        };
        if opened.payload.kind == lifeline_proto::PayloadKind::Receipt {
            self.process_receipt(opened);
        } else if opened.payload.kind == PayloadKind::Custody {
            self.process_custody(opened);
        } else if opened.payload.kind == PayloadKind::Onion {
            self.handle_onion(opened, bundle.priority, now);
        } else if opened.payload.kind == PayloadKind::BlockRequest {
            self.handle_block_request(opened, now);
        } else if opened.payload.kind == PayloadKind::BlockResponse {
            self.handle_block_response(opened);
        } else {
            self.deliver_message(&bundle, opened, now);
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
                    // The origin is hidden by construction; the visible "from" is
                    // the immediate previous hop (the last relay, or the sender
                    // on a direct send).
                    self.inbox.push(Inbound {
                        from: opened.sender.id.clone(),
                        payload: real,
                    });
                }
            }
            Err(_) => {}
        }
    }

    /// FR-25 (carrier side): a committed custodian has signed for a bundle we are
    /// relaying, so free our copy — provided the signature verifies and the
    /// bundle is a relayed copy (never one we originated).
    fn process_custody(&mut self, opened: lifeline_core::message::Opened) {
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
            return; // our own message — we retain it until delivery-verified
        }
        self.router.release_custody(&cr.bundle_id);
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
        let sender_pub = self.public.sign_pub.clone();
        let recipient_pub = opened.sender.sign_pub.clone();
        if let Some(rec) = self.sent.iter_mut().find(|s| s.bundle_id == dr.bundle_id) {
            if verify_delivery(
                &rec.original,
                &dr,
                sender_pub.as_slice(),
                recipient_pub.as_slice(),
            )
            .is_ok()
            {
                rec.verified = true;
            }
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

    /// Whether this node currently stores a copy of `bundle_id` (tests/UI).
    pub fn holds_bundle(&self, bundle_id: &Bytes) -> bool {
        self.router.holds(bundle_id)
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
