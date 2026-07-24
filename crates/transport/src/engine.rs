//! The node runtime: an identity + DTN router + CRDT state driven over any set
//! of [`Interface`]s (PRD §7 L1–L5 composed; FR-22 "transports run
//! concurrently").
//!
//! This is the real, transport-independent node. Give it a BLE interface, an
//! ultrasound interface, and an internet interface, and it will advertise,
//! discover peers, and shuttle the *same* end-to-end-encrypted bundles over
//! whichever links exist — fragmenting each to that interface's MTU. Swap the
//! in-memory interfaces for real radios and nothing else changes.

use crate::frame::{Fragmenter, Frame, FrameKind, Reassembler};
use crate::interface::{Interface, PeerId};
use lifeline_core::erasure::{fragment_bundle, FragmentCollector};
use lifeline_core::group::{GroupEnvelope, GroupMessage, ReceiverKeyState, SenderKeyState};
use lifeline_core::message::{open_bundle, seal_bundle, SealOptions};
use lifeline_core::receipt::{make_delivery_receipt, verify_delivery};
use lifeline_core::Identity;
use lifeline_proto::codec::{b64url_decode, b64url_encode, from_cbor, to_cbor};
use lifeline_proto::{
    Address, Bundle, Bytes, Coords, DeliveryReceipt, IdentityPublic, Payload, PayloadKind, Priority,
};
use lifeline_router::{DtnRouter, IngestOutcome, PeerInfo, RouterConfig};
use lifeline_sync::SharedState;
use std::collections::{HashMap, HashSet};

/// Engine tunables.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub router: RouterConfig,
    /// Re-spray an unverified message after this many time units (FR-32).
    pub retry_window: u64,
    /// Max re-sprays per message before giving up.
    pub max_retries: u8,
    /// Copy budget to restore on each re-spray.
    pub respray_copies: u16,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            router: RouterConfig::default(),
            retry_window: 60,
            max_retries: 4,
            respray_copies: 6,
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
    reasm: Reassembler,
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
    /// Adaptive-retry config (FR-32) + counter.
    retry_window: u64,
    max_retries: u8,
    respray_copies: u16,
    retries_total: u64,
}

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
            retry_window,
            max_retries,
            respray_copies,
            retries_total: 0,
        }
    }

    /// Attach an interface (BLE, ultrasound, internet, …). Interfaces run
    /// concurrently; the same bundle can travel over any of them (FR-22).
    pub fn add_interface(&mut self, iface: Box<dyn Interface>) {
        self.ports.push(Port {
            iface,
            reasm: Reassembler::new(),
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
        self.router.submit_local(bundle, now);
        id
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
                    self.router.submit_local(f, now);
                }
            }
            // Fallback: if coding fails, send the whole bundle.
            Err(_) => self.router.submit_local(bundle, now),
        }
        group
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
        self.offer_round(now);
        self.retry_unverified(now);
        self.router.tick(now);
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
                if let Some((kind, payload)) = self.ports[p].reasm.accept(frame) {
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
                        self.send_unit(p, peer, FrameKind::Bundle, &bytes, usable);
                    }
                }
                // Anti-entropy: push our CRDT state (§12.3, FR-33).
                if let Ok(bytes) = to_cbor(&self.state) {
                    self.send_unit(p, peer, FrameKind::State, &bytes, usable);
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
    ) {
        let mid = self.next_mid();
        if let Ok(frames) = Fragmenter::fragment(kind, mid, payload, usable) {
            for f in &frames {
                if let Ok(enc) = f.encode() {
                    let _ = self.ports[p].iface.send(peer, &enc);
                }
            }
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
                    self.handle_ingest(b, now);
                }
            }
            FrameKind::State => {
                if let Ok(s) = from_cbor::<SharedState>(&payload) {
                    self.state.merge(&s);
                }
            }
        }
    }

    fn handle_ingest(&mut self, bundle: Bundle, now: u64) {
        if let IngestOutcome::Delivered = self.router.ingest(bundle.clone(), now) {
            self.on_delivered(bundle, now);
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
        } else {
            self.deliver_message(&bundle, opened, now);
        }
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
            self.router.submit_local(rb, now);
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
