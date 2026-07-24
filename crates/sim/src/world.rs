//! The simulated world: nodes, mobility, opportunistic contacts, internet
//! fabric, and delivery accounting.

use lifeline_core::message::{seal_bundle, SealOptions};
use lifeline_core::receipt::{make_delivery_receipt, verify_delivery};
use lifeline_core::{message::open_bundle, HashLog, Identity};
use lifeline_proto::codec::{b64url_decode, b64url_encode, from_cbor, to_cbor};
use lifeline_proto::{
    Address, Bundle, Bytes, DeliveryReceipt, GatewayAnnounce, IdentityPublic, LogEvent, Payload,
    PayloadKind, Priority,
};
use lifeline_router::{DtnRouter, IngestOutcome, PeerInfo, RouterConfig};
use lifeline_sync::SharedState;
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use std::collections::{HashMap, HashSet};

/// Seconds of simulated time per tick.
const STEP_SECS: u64 = 10;
/// Freshness window for a gateway announce (PRD Appendix B: 1 h).
const ANNOUNCE_TTL_S: u64 = 3600;

/// A "data mule": a node that ferries between clusters on a fixed route,
/// physically carrying queued bundles (PRD UC3, Appendix A).
#[derive(Debug, Clone)]
pub struct Mule {
    pub node: usize,
    /// Cluster ids visited in order.
    pub route: Vec<usize>,
    /// Ticks spent in each cluster before moving on.
    pub dwell: u64,
}

/// One node in the world: a real identity + router + log, plus sim-only
/// placement.
struct Node {
    identity: Identity,
    public: IdentityPublic,
    router: DtnRouter,
    log: HashLog,
    /// Partition-surviving CRDT state synced on contact (FR-33, §12.3).
    state: SharedState,
    /// Reassembles erasure-coded fragment bundles (FR-28).
    frag_collector: lifeline_core::erasure::FragmentCollector,
    cluster: usize,
    is_gateway: bool,
    caps: Vec<String>,
}

impl Node {
    fn addr(&self) -> Address {
        self.public.id.clone()
    }

    fn seal(&self, to: &IdentityPublic, payload: &Payload, opts: &SealOptions) -> Option<Bundle> {
        seal_bundle(&self.identity, to, payload, opts).ok()
    }

    fn open(&self, bundle: &Bundle) -> Option<lifeline_core::message::Opened> {
        open_bundle(&self.identity, bundle).ok()
    }

    /// Append a signed entry to this node's hash-linked log (FR-30). Uses a
    /// disjoint field borrow of `self.log` + `self.identity`.
    fn log_event(&mut self, event: LogEvent, ref_id: &Bytes, at: u64) {
        let _ = self.log.append(&self.identity, event, ref_id, at);
    }
}

/// Bookkeeping for one message the harness sent, tracking its fate (NFR-3: no
/// silent loss — every message resolves).
struct SentMessage {
    bundle_id: Bytes,
    /// Sender/recipient node indices — reserved for detailed per-message
    /// diagnostics; delivery accounting uses the tick fields below.
    #[allow(dead_code)]
    from: usize,
    #[allow(dead_code)]
    to: usize,
    original: Bundle,
    delivered_tick: Option<u64>,
    verified_tick: Option<u64>,
}

/// Final measurement of a run.
#[derive(Debug, Clone)]
pub struct DeliveryReport {
    pub sent: usize,
    pub delivered: usize,
    pub verified: usize,
    pub ticks: u64,
    /// Total copies forwarded across all routers (spray overhead).
    pub forwarded_copies: u64,
    pub duplicates_suppressed: u64,
    pub dropped_expired: u64,
    /// Bundles refused by relays for missing/invalid PoW postage (FR-46).
    pub dropped_nopostage: u64,
}

impl DeliveryReport {
    pub fn pct_delivered(&self) -> f64 {
        if self.sent == 0 {
            0.0
        } else {
            100.0 * self.delivered as f64 / self.sent as f64
        }
    }
    pub fn pct_verified(&self) -> f64 {
        if self.sent == 0 {
            0.0
        } else {
            100.0 * self.verified as f64 / self.sent as f64
        }
    }
}

/// The simulated world.
pub struct World {
    nodes: Vec<Node>,
    mules: Vec<Mule>,
    sent: Vec<SentMessage>,
    /// Bundle ids that are delivery-receipt carriers (so we don't ack an ack).
    receipts: HashSet<Bytes>,
    tick: u64,
    now: u64,
    /// Apply PoW-postage admission gating at every router (FR-46).
    require_postage: bool,
    /// Per-handoff bundle loss probability (models lossy links / partial escape).
    loss: f64,
    /// Erasure-coded groups sent, tracked by group id (FR-28).
    erasure_sent: Vec<ErasureSent>,
    rng: StdRng,
}

/// Tracking for an erasure-coded message (delivered = reconstructed).
struct ErasureSent {
    group: Bytes,
    delivered_tick: Option<u64>,
}

impl World {
    /// Create an empty world with a fixed RNG seed (deterministic runs).
    pub fn new(seed: u64) -> Self {
        World {
            nodes: Vec::new(),
            mules: Vec::new(),
            sent: Vec::new(),
            receipts: HashSet::new(),
            tick: 0,
            now: 0,
            require_postage: false,
            loss: 0.0,
            erasure_sent: Vec::new(),
            rng: StdRng::seed_from_u64(seed),
        }
    }

    /// Set a per-handoff bundle loss probability in `[0,1]` (FR-28 / Problem C:
    /// models carriers that fail to escape and links that drop frames).
    pub fn set_loss(&mut self, rate: f64) {
        self.loss = rate.clamp(0.0, 1.0);
    }

    /// Enable PoW-postage admission gating for routers added afterward (FR-46).
    /// Call before [`World::add_node`].
    pub fn set_require_postage(&mut self, require: bool) {
        self.require_postage = require;
    }

    /// Add a node in `cluster`. Returns its index. Set `caps` non-empty and
    /// pass `is_gateway = true` to make it a gateway (FR-35).
    pub fn add_node(&mut self, cluster: usize, is_gateway: bool, caps: Vec<String>) -> usize {
        // Seed identities from the world's RNG so a given seed reproduces the
        // same set of nodes/addresses (message nonces still use the OS CSPRNG).
        let identity = Identity::generate_from_rng(&mut self.rng, self.now);
        let public = identity.public();
        let cfg = RouterConfig {
            require_postage: self.require_postage,
            ..RouterConfig::default()
        };
        let mut router = DtnRouter::new(public.id.clone(), cfg);
        if is_gateway {
            router.set_gateway(caps.clone());
        }
        let log = HashLog::new(public.id.clone());
        let state = SharedState::new(public.id.clone());
        let idx = self.nodes.len();
        self.nodes.push(Node {
            identity,
            public,
            router,
            log,
            state,
            frag_collector: lifeline_core::erasure::FragmentCollector::new(),
            cluster,
            is_gateway,
            caps,
        });
        idx
    }

    /// Register a data mule.
    pub fn add_mule(&mut self, mule: Mule) {
        self.mules.push(mule);
    }

    /// The public identity record of a node (used to hand out "contacts").
    pub fn public(&self, idx: usize) -> IdentityPublic {
        self.nodes[idx].public.clone()
    }

    /// Send a message from `from` to `to` with the given payload and priority.
    /// The sender must already hold `to`'s public identity (as if added by QR,
    /// FR-6). Returns the bundle id.
    pub fn send(&mut self, from: usize, to: usize, payload: Payload, priority: Priority) -> Bytes {
        let to_pub = self.nodes[to].public.clone();
        let opts = SealOptions::normal(self.now).with_priority(priority);
        let bundle = self.nodes[from]
            .seal(&to_pub, &payload, &opts)
            .expect("seal succeeds");
        let id = bundle.bundle_id.clone();
        self.nodes[from].log_event(LogEvent::Sent, &id, self.now);
        self.nodes[from]
            .router
            .submit_local(bundle.clone(), self.now);
        self.sent.push(SentMessage {
            bundle_id: id.clone(),
            from,
            to,
            original: bundle,
            delivered_tick: None,
            verified_tick: None,
        });
        id
    }

    /// Convenience: a plain text message.
    pub fn send_text(&mut self, from: usize, to: usize, body: &str) -> Bytes {
        let payload = Payload {
            kind: PayloadKind::Text,
            body: Some(body.to_string()),
            coords: None,
            battery_pct: None,
            attach: None,
            group_id: None,
        };
        self.send(from, to, payload, Priority::Normal)
    }

    /// Send an **erasure-coded** message (`k` of `k+m` fragments reconstruct) so
    /// it survives partial carrier escape / lossy links (FR-28, Problem C).
    /// Returns the group id; "delivered" means reconstructed at the recipient.
    pub fn send_erasure(
        &mut self,
        from: usize,
        to: usize,
        body: &str,
        k: usize,
        m: usize,
    ) -> Bytes {
        let to_pub = self.nodes[to].public.clone();
        let payload = Payload {
            kind: PayloadKind::Text,
            body: Some(body.to_string()),
            coords: None,
            battery_pct: None,
            attach: None,
            group_id: None,
        };
        let opts = SealOptions::normal(self.now);
        let bundle = self.nodes[from]
            .seal(&to_pub, &payload, &opts)
            .expect("seal");
        let group = bundle.bundle_id.clone();
        self.erasure_sent.push(ErasureSent {
            group: group.clone(),
            delivered_tick: None,
        });
        match lifeline_core::erasure::fragment_bundle(&bundle, k, m) {
            Ok(frags) => {
                for f in frags {
                    self.nodes[from].router.submit_local(f, self.now);
                }
            }
            Err(_) => self.nodes[from].router.submit_local(bundle, self.now),
        }
        group
    }

    /// Number of erasure-coded messages sent.
    pub fn erasure_sent_count(&self) -> usize {
        self.erasure_sent.len()
    }

    /// Number of erasure-coded messages successfully reconstructed.
    pub fn erasure_delivered_count(&self) -> usize {
        self.erasure_sent
            .iter()
            .filter(|e| e.delivered_tick.is_some())
            .count()
    }

    // --- Simulation loop ---

    /// Run the world for `ticks` steps and return the delivery report.
    pub fn run(&mut self, ticks: u64) -> DeliveryReport {
        for _ in 0..ticks {
            self.step();
        }
        self.report()
    }

    fn step(&mut self) {
        self.now = self.tick * STEP_SECS;
        self.move_mules();
        self.emit_gateway_announces();
        self.run_contacts();
        self.run_internet_fabric();
        for n in &mut self.nodes {
            n.router.tick(self.now);
        }
        self.tick += 1;
    }

    /// Update mule cluster placement from their routes.
    fn move_mules(&mut self) {
        for m in &self.mules {
            if m.route.is_empty() {
                continue;
            }
            let leg = (self.tick / m.dwell.max(1)) as usize % m.route.len();
            self.nodes[m.node].cluster = m.route[leg];
        }
    }

    /// Each gateway observes its own fresh announce (distance 0) so it can be
    /// gossiped outward and build a gradient (FR-36, §12.2).
    fn emit_gateway_announces(&mut self) {
        let now = self.now;
        let seq = self.tick;
        for n in &mut self.nodes {
            if !n.is_gateway {
                continue;
            }
            let announce = make_announce(&n.identity, &n.caps, seq, now);
            n.router.observe_announce(announce, 0, now);
        }
    }

    /// Run all opportunistic contacts this tick: nodes co-located in the same
    /// cluster can exchange (models BLE/Wi-Fi-Aware in-range links).
    fn run_contacts(&mut self) {
        // Group node indices by current cluster.
        let mut by_cluster: HashMap<usize, Vec<usize>> = HashMap::new();
        for (i, n) in self.nodes.iter().enumerate() {
            by_cluster.entry(n.cluster).or_default().push(i);
        }
        // Deterministic pair list.
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        let mut clusters: Vec<usize> = by_cluster.keys().copied().collect();
        clusters.sort_unstable();
        for c in clusters {
            let members = &by_cluster[&c];
            for a in 0..members.len() {
                for b in (a + 1)..members.len() {
                    pairs.push((members[a], members[b]));
                }
            }
        }
        for (a, b) in pairs {
            self.exchange(a, b);
        }
    }

    /// One bidirectional exchange between two in-contact nodes.
    fn exchange(&mut self, i: usize, j: usize) {
        let now = self.now;
        let pj = self.peer_info(j);
        let pi = self.peer_info(i);

        // Offer bundles both ways (spray-and-wait / gradient decisions inside).
        let offers_i_to_j = self.nodes[i].router.offer_to(&pj, now);
        let offers_j_to_i = self.nodes[j].router.offer_to(&pi, now);

        // Gossip gateway announces both ways (a few hops).
        let gi = self.nodes[i].router.announces_to_gossip(now);
        let gj = self.nodes[j].router.announces_to_gossip(now);
        for (a, d) in gi {
            self.nodes[j].router.observe_announce(a, d, now);
        }
        for (a, d) in gj {
            self.nodes[i].router.observe_announce(a, d, now);
        }

        // Anti-entropy: merge CRDT shared state both ways (§12.3, FR-33).
        let sj = self.nodes[j].state.clone();
        self.nodes[i].state.merge(&sj);
        let si = self.nodes[i].state.clone();
        self.nodes[j].state.merge(&si);

        // Gossip relay reputation both ways so demotions propagate (FR-47).
        let rj = self.nodes[j].router.reputation();
        self.nodes[i].router.merge_reputation(&rj);
        let ri = self.nodes[i].router.reputation();
        self.nodes[j].router.merge_reputation(&ri);

        for b in offers_i_to_j {
            self.deliver_to(j, b);
        }
        for b in offers_j_to_i {
            self.deliver_to(i, b);
        }
    }

    fn peer_info(&self, idx: usize) -> PeerInfo {
        let n = &self.nodes[idx];
        PeerInfo {
            addr: n.addr(),
            is_gateway: n.is_gateway,
            gradient: n.router.gradient(self.now),
            known: n.router.known_ids(),
        }
    }

    /// The internet fabric: every internet gateway pushes its non-local bundles
    /// onto the uplink; the fabric delivers each (once) into every internet
    /// gateway, which then meshes it into its own cluster (FR-37, UC4).
    fn run_internet_fabric(&mut self) {
        let gw_idxs: Vec<usize> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.is_gateway && n.caps.iter().any(|c| c == "internet"))
            .map(|(i, _)| i)
            .collect();
        if gw_idxs.is_empty() {
            return;
        }
        // Collect + globally dedup bridged bundles.
        let mut fabric: Vec<Bundle> = Vec::new();
        let mut seen: HashSet<Bytes> = HashSet::new();
        for &g in &gw_idxs {
            for b in self.nodes[g].router.drain_bridge() {
                if seen.insert(b.bundle_id.clone()) {
                    fabric.push(b);
                }
            }
        }
        // Deliver into every internet gateway (they mesh it onward).
        for b in fabric {
            for &g in &gw_idxs {
                self.deliver_to(g, b.clone());
            }
        }
    }

    /// Ingest a bundle at node `idx` and react to delivery.
    fn deliver_to(&mut self, idx: usize, bundle: Bundle) {
        let now = self.now;
        // Lossy link: the bundle is dropped in transit (FR-28 / Problem C).
        if self.loss > 0.0 && self.rng.gen::<f64>() < self.loss {
            return;
        }
        if let IngestOutcome::Delivered = self.nodes[idx].router.ingest(bundle.clone(), now) {
            // Erasure fragment: buffer until `k` shards reconstruct (FR-28).
            if bundle.frag.is_some() {
                if let Some(recon) = self.nodes[idx].frag_collector.accept(&bundle) {
                    if let Some(rec) = self
                        .erasure_sent
                        .iter_mut()
                        .find(|e| e.group == recon.bundle_id)
                    {
                        if rec.delivered_tick.is_none() {
                            rec.delivered_tick = Some(self.tick);
                        }
                    }
                }
                return;
            }
            if self.receipts.contains(&bundle.bundle_id) {
                self.process_receipt(idx, &bundle);
            } else {
                self.deliver_message(idx, &bundle);
            }
        }
    }

    /// A payload message reached its recipient: record it, log it, and emit a
    /// signed delivery receipt back to the (now-known) sender (FR-31).
    fn deliver_message(&mut self, idx: usize, bundle: &Bundle) {
        let now = self.now;
        let tick = self.tick;
        let Some(opened) = self.nodes[idx].open(bundle) else {
            return; // not decryptable by us (shouldn't happen if dst==us)
        };

        // Mark the original message delivered.
        if let Some(sm) = self
            .sent
            .iter_mut()
            .find(|s| s.bundle_id == bundle.bundle_id)
        {
            if sm.delivered_tick.is_none() {
                sm.delivered_tick = Some(tick);
            }
        }
        self.nodes[idx].log_event(LogEvent::Received, &bundle.bundle_id, now);

        // Build a receipt payload sealed back to the sender.
        let receipt = make_delivery_receipt(&self.nodes[idx].identity, &bundle.bundle_id, now);
        let receipt_bytes = to_cbor(&receipt).expect("cbor receipt");
        let payload = Payload {
            kind: PayloadKind::Receipt,
            body: Some(b64url_encode(&receipt_bytes)),
            coords: None,
            battery_pct: None,
            attach: None,
            group_id: None,
        };
        let opts = SealOptions::normal(now).with_priority(Priority::Alert);
        if let Some(rb) = self.nodes[idx].seal(&opened.sender, &payload, &opts) {
            let rid = rb.bundle_id.clone();
            self.receipts.insert(rid.clone());
            self.nodes[idx].log_event(LogEvent::Sent, &rid, now);
            self.nodes[idx].router.submit_local(rb, now);
        }
    }

    /// A delivery receipt reached the original sender: decode, then verify the
    /// full delivery proof **offline** (§12.4) and mark it verified (FR-32).
    fn process_receipt(&mut self, idx: usize, receipt_bundle: &Bundle) {
        let tick = self.tick;
        let Some(opened) = self.nodes[idx].open(receipt_bundle) else {
            return;
        };
        let Some(body) = opened.payload.body else {
            return;
        };
        let Ok(raw) = b64url_decode(&body) else {
            return;
        };
        let Ok(dr) = from_cbor::<DeliveryReceipt>(&raw) else {
            return;
        };

        let Some(pos) = self.sent.iter().position(|s| s.bundle_id == dr.bundle_id) else {
            return;
        };
        // Sender of the original message is this node (receipt was addressed here).
        let sender_pub = self.nodes[idx].public.sign_pub.clone();
        let recipient_pub = opened.sender.sign_pub.clone();
        let original = self.sent[pos].original.clone();
        if verify_delivery(
            &original,
            &dr,
            sender_pub.as_slice(),
            recipient_pub.as_slice(),
        )
        .is_ok()
            && self.sent[pos].verified_tick.is_none()
        {
            self.sent[pos].verified_tick = Some(tick);
        }
    }

    /// Aggregate the run into a report.
    pub fn report(&self) -> DeliveryReport {
        let delivered = self
            .sent
            .iter()
            .filter(|s| s.delivered_tick.is_some())
            .count();
        let verified = self
            .sent
            .iter()
            .filter(|s| s.verified_tick.is_some())
            .count();
        let mut forwarded = 0;
        let mut dups = 0;
        let mut expired = 0;
        let mut nopostage = 0;
        for n in &self.nodes {
            let s = n.router.stats();
            forwarded += s.forwarded_copies;
            dups += s.duplicates;
            expired += s.dropped_expired;
            nopostage += s.dropped_nopostage;
        }
        DeliveryReport {
            sent: self.sent.len(),
            delivered,
            verified,
            ticks: self.tick,
            forwarded_copies: forwarded,
            duplicates_suppressed: dups,
            dropped_expired: expired,
            dropped_nopostage: nopostage,
        }
    }

    /// Inject `count` unpaid BULK bundles from a spammer (no PoW postage). With
    /// postage gating on, relays drop these at the first hop — demonstrating the
    /// flood is throttled without ever touching legitimate/SOS traffic (FR-46
    /// AC). These are *not* counted as legitimate sent messages.
    pub fn inject_unpaid_bulk(&mut self, from: usize, to: usize, count: usize) {
        let to_pub = self.nodes[to].public.clone();
        for _ in 0..count {
            let payload = Payload {
                kind: PayloadKind::Text,
                body: Some("BUY NOW".into()),
                coords: None,
                battery_pct: None,
                attach: None,
                group_id: None,
            };
            let opts = SealOptions::normal(self.now).with_priority(Priority::Bulk);
            if let Some(mut b) = self.nodes[from].seal(&to_pub, &payload, &opts) {
                b.postage = None; // spammer refuses to pay
                self.nodes[from].router.submit_local(b, self.now);
            }
        }
    }

    /// Verify every node's hash-linked log is internally consistent (FR-30) —
    /// a whole-world integrity check used by tests.
    pub fn all_logs_valid(&self) -> bool {
        self.nodes.iter().all(|n| {
            n.log
                .verify_chain(n.identity.verifying_key().as_bytes())
                .is_ok()
        })
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Address of a node (for group membership operations).
    pub fn address(&self, idx: usize) -> Address {
        self.nodes[idx].public.id.clone()
    }

    // --- Reputation (FR-47) ---

    /// Node `observer` penalizes relay node `target` (as if it detected a
    /// black-hole). The demotion then gossips across the mesh on contact.
    pub fn penalize_relay(&mut self, observer: usize, target: usize, w: f32) {
        let t = self.address(target);
        self.nodes[observer].router.penalize_relay(&t, w);
    }

    /// How many nodes currently consider `target` a demoted relay?
    pub fn demoting_count(&self, target: usize) -> usize {
        let t = self.address(target);
        self.nodes
            .iter()
            .filter(|n| n.router.is_demoted(&t))
            .count()
    }

    // --- CRDT shared-state operations (FR-12, FR-33, FR-48) ---

    /// Node `idx` adds `member` (a node index) to `group` in its local CRDT.
    pub fn group_add(&mut self, idx: usize, group: &str, member: usize) {
        let m = self.address(member);
        self.nodes[idx].state.add_member(group, m);
    }

    /// Node `idx` removes `member` from `group` in its local CRDT.
    pub fn group_remove(&mut self, idx: usize, group: &str, member: usize) {
        let m = self.address(member);
        self.nodes[idx].state.remove_member(group, &m);
    }

    /// Sorted membership of `group` as seen by node `idx`.
    pub fn group_members(&self, idx: usize, group: &str) -> Vec<Address> {
        self.nodes[idx].state.members(group)
    }

    /// Do all nodes agree on the exact membership of `group`? (FR-33 AC in a
    /// realistic mesh: every replica converges to identical state.)
    pub fn all_agree_on_group(&self, group: &str) -> bool {
        let mut iter = self.nodes.iter();
        let Some(first) = iter.next() else {
            return true;
        };
        let baseline = first.state.members(group);
        self.nodes
            .iter()
            .all(|n| n.state.members(group) == baseline)
    }
}

/// Build a signed gateway announce for the sim (router trusts announces in
/// Phase 1; signature verification is L6/Phase 3 anti-abuse).
fn make_announce(identity: &Identity, caps: &[String], seq: u64, now: u64) -> GatewayAnnounce {
    let mut a = GatewayAnnounce {
        gateway: identity.address().clone(),
        caps: caps.to_vec(),
        load_q: GatewayAnnounce::from_load(0.3),
        seq,
        expires_at: now + ANNOUNCE_TTL_S,
        sig: Bytes::default(),
    };
    // Sign over the canonical announce fields (excluding sig).
    let signing =
        to_cbor(&(&a.gateway, &a.caps, a.load_q, a.seq, a.expires_at)).expect("cbor announce");
    a.sig = identity.sign(&signing);
    a
}
