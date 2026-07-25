//! The per-node DTN router state machine (PRD §8.5, §12.1, §12.5).

use crate::bounded::BoundedSet;
use crate::gateway::GatewayCache;
use crate::store::{BundleStore, DEFAULT_STORE_CAP_BYTES};
use lifeline_proto::{Address, Bundle, Bytes, GatewayAnnounce, Priority};
use std::collections::HashSet;

/// Capability string for an internet-uplink gateway (PRD §11.6).
pub const CAP_INTERNET: &str = "internet";

/// Cap on the dedup / bridged membership caches — bounds memory against a flood
/// of unique `bundle_id`s (which the store byte-cap does not cover).
const MAX_SEEN: usize = 200_000;
/// Cap on the gateway's pending-uplink queue (holds full bundle clones).
const MAX_BRIDGE_OUT: usize = 4096;
/// Upper clamp on a bundle's spray copy budget, so a malicious peer can't inject
/// `copies_left = u16::MAX` and trigger a replication storm (FR-24).
const MAX_COPIES: u16 = 16;

/// Router tunables.
#[derive(Debug, Clone)]
pub struct RouterConfig {
    /// Carried-bundle store cap in bytes (FR-52).
    pub store_cap_bytes: u64,
    /// Hop limit enforced when relaying (defensive; the bundle also carries its
    /// own `hop_limit`, and we honour the smaller).
    pub max_hops: u16,
    /// If set, gate NORMAL/BULK admission on valid PoW postage (FR-46). SOS and
    /// ALERT-exempt classes always pass. Off by default so radio-less test
    /// topologies aren't forced to pay postage.
    pub require_postage: bool,
}

impl Default for RouterConfig {
    fn default() -> Self {
        RouterConfig {
            store_cap_bytes: DEFAULT_STORE_CAP_BYTES,
            max_hops: 32,
            require_postage: false,
        }
    }
}

/// What happened when a bundle was ingested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestOutcome {
    /// The bundle was addressed to us — hand it to the application to decrypt.
    Delivered,
    /// Stored for later forwarding.
    Stored,
    /// Already seen (dedup by `bundle_id`) — suppressed (FR-26).
    Duplicate,
    /// Dropped; carries a human reason (expired / hop limit / no room).
    Dropped(&'static str),
}

/// Snapshot of a peer we are in contact with, as reported by the transport.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub addr: Address,
    pub is_gateway: bool,
    /// Peer's hops-to-nearest-gateway, if it advertised one.
    pub gradient: Option<u16>,
    /// Bundle ids the peer already holds (anti-entropy — avoids re-sending).
    pub known: HashSet<Bytes>,
    /// Bearer bandwidth hint (FR — "straw, not a firehose"): the max bundle size
    /// this low-bandwidth link should carry for NORMAL/BULK traffic. A heavier
    /// bundle is held back (not marked offered) so it can wait for a fatter
    /// bearer; SOS/ALERT and final-hop delivery always bypass it. `None` = no
    /// limit (a high-throughput link like Wi-Fi/internet).
    pub soft_max_bytes: Option<u64>,
}

/// Aggregate counters for diagnostics (FR-53) and sim assertions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RouterStats {
    pub store_len: usize,
    pub store_bytes: u64,
    pub delivered: u64,
    pub duplicates: u64,
    pub dropped_expired: u64,
    pub dropped_hoplimit: u64,
    pub dropped_norroom: u64,
    pub dropped_nopostage: u64,
    pub forwarded_copies: u64,
    /// Copies dropped after another node signed for custody (FR-25).
    pub custody_transfers: u64,
    pub known_gateways: usize,
}

/// A per-node delay-tolerant router.
pub struct DtnRouter {
    me: Address,
    cfg: RouterConfig,
    is_gateway: bool,
    caps: Vec<String>,

    store: BundleStore,
    /// Ids ever delivered-to-us or dropped — for dedup. Bounded (LRU) so a flood
    /// of unique ids can't exhaust memory; TTL means an evicted id can't usefully
    /// return anyway.
    seen: BoundedSet<Bytes>,
    /// Ids already pushed to the internet fabric by this (gateway) node (bounded).
    bridged: BoundedSet<Bytes>,
    /// Queue of bundles this internet gateway should push over its uplink (capped).
    bridge_out: Vec<Bundle>,

    gateways: GatewayCache,
    reputation: crate::reputation::Reputation,
    /// Source-side black-hole attribution: which peers took custody of our
    /// bundles, and whether those bundles were delivered (FR-47).
    attribution: crate::attribution::ForwardLedger,
    stats: RouterStats,
}

impl DtnRouter {
    pub fn new(me: Address, cfg: RouterConfig) -> Self {
        let store = BundleStore::new(cfg.store_cap_bytes);
        DtnRouter {
            me,
            cfg,
            is_gateway: false,
            caps: Vec::new(),
            store,
            seen: BoundedSet::new(MAX_SEEN),
            bridged: BoundedSet::new(MAX_SEEN),
            bridge_out: Vec::new(),
            gateways: GatewayCache::new(),
            reputation: crate::reputation::Reputation::new(),
            attribution: crate::attribution::ForwardLedger::new(),
            stats: RouterStats::default(),
        }
    }

    pub fn address(&self) -> &Address {
        &self.me
    }

    /// Toggle this node into gateway mode with the given capabilities (FR-35).
    pub fn set_gateway(&mut self, caps: Vec<String>) {
        self.is_gateway = true;
        self.caps = caps;
    }

    pub fn is_gateway(&self) -> bool {
        self.is_gateway
    }

    fn has_internet(&self) -> bool {
        self.caps.iter().any(|c| c == CAP_INTERNET)
    }

    /// The node's forwarding gradient (hops-to-nearest-gateway). A gateway's own
    /// gradient is 0.
    pub fn gradient(&self, now: u64) -> Option<u16> {
        if self.is_gateway {
            return Some(0);
        }
        self.gateways.gradient(now)
    }

    /// Submit a locally-originated bundle for delivery (§12.1 step 3).
    pub fn submit_local(&mut self, bundle: Bundle, now: u64) {
        if bundle.dst == self.me {
            // Talking to yourself — just count it delivered.
            self.seen.insert(bundle.bundle_id.clone());
            self.stats.delivered += 1;
            return;
        }
        self.store.insert(bundle, now);
        self.refresh_store_stats();
    }

    /// Ingest a bundle received from a peer (§12.1 step 4/6).
    pub fn ingest(&mut self, mut bundle: Bundle, now: u64) -> IngestOutcome {
        // Dedup across the bundle's whole lifetime.
        if self.seen.contains(&bundle.bundle_id) || self.store.contains(&bundle.bundle_id) {
            self.stats.duplicates += 1;
            return IngestOutcome::Duplicate;
        }

        // Deliver-to-us takes precedence over every drop rule except dedup.
        if bundle.dst == self.me {
            self.seen.insert(bundle.bundle_id.clone());
            self.stats.delivered += 1;
            return IngestOutcome::Delivered;
        }

        if bundle.is_expired(now) {
            self.seen.insert(bundle.bundle_id.clone());
            self.stats.dropped_expired += 1;
            return IngestOutcome::Dropped("expired");
        }
        let hop_cap = bundle.hop_limit.min(self.cfg.max_hops);
        if bundle.hops > hop_cap {
            self.seen.insert(bundle.bundle_id.clone());
            self.stats.dropped_hoplimit += 1;
            return IngestOutcome::Dropped("hop limit");
        }

        // Anti-abuse admission: gate NORMAL/BULK on valid PoW postage (FR-46,
        // §12.5). SOS/exempt classes bypass, so a flood never delays emergencies.
        if self.cfg.require_postage
            && !lifeline_proto::pow::verify_for_priority(
                bundle.bundle_id.as_slice(),
                bundle.postage.as_ref(),
                bundle.priority,
            )
        {
            self.seen.insert(bundle.bundle_id.clone());
            self.stats.dropped_nopostage += 1;
            return IngestOutcome::Dropped("no postage");
        }

        // Internet gateway: queue this non-local bundle for the uplink (FR-37).
        // The queue is capped (drop-oldest) so a flood with the uplink down can't
        // grow it without bound.
        if self.is_gateway && self.has_internet() && !self.bridged.contains(&bundle.bundle_id) {
            self.bridged.insert(bundle.bundle_id.clone());
            if self.bridge_out.len() >= MAX_BRIDGE_OUT {
                self.bridge_out.remove(0);
            }
            self.bridge_out.push(bundle.clone());
        }

        // Clamp copies to `[1, MAX_COPIES]` so a malicious peer can't inject a
        // huge `copies_left` and trigger a spray/replication storm (FR-24).
        bundle.copies_left = bundle.copies_left.clamp(1, MAX_COPIES);
        if self.store.insert(bundle, now) {
            self.refresh_store_stats();
            IngestOutcome::Stored
        } else {
            self.stats.dropped_norroom += 1;
            IngestOutcome::Dropped("no room")
        }
    }

    /// Ids we hold or have seen — shared with a peer for anti-entropy (§12.3).
    pub fn known_ids(&self) -> HashSet<Bytes> {
        let mut ids: HashSet<Bytes> = self.store.ids().cloned().collect();
        ids.extend(self.seen.iter().cloned());
        ids
    }

    /// Decide which bundles to hand to `peer` on this contact, applying binary
    /// spray-and-wait and the gateway gradient. Mutates local copy budgets.
    pub fn offer_to(&mut self, peer: &PeerInfo, now: u64) -> Vec<Bundle> {
        self.store.evict_expired(now);
        let my_gradient = self.gradient(now);
        let peer_helps_gateway = peer.is_gateway
            || matches!((peer.gradient, my_gradient), (Some(pg), mg) if mg.map_or(true, |g| pg < g));
        // Route around demoted (black-hole) relays: hand them nothing except a
        // final delivery to the recipient itself, or an SOS (never block an
        // emergency on a reputation heuristic). FR-47.
        let peer_demoted = self.reputation.is_demoted(&peer.addr);

        // Collect candidate ids in strict priority order (SOS first), then oldest.
        let mut candidates: Vec<(Bytes, Priority, u64)> = self
            .store
            .iter()
            .filter(|s| {
                !s.bundle.is_expired(now)
                    && !peer.known.contains(&s.bundle.bundle_id)
                    && !s.offered_to.contains(&peer.addr)
            })
            .map(|s| {
                (
                    s.bundle.bundle_id.clone(),
                    s.bundle.priority,
                    s.bundle.created_at,
                )
            })
            .collect();
        candidates.sort_by(|a, b| a.1.as_u8().cmp(&b.1.as_u8()).then(a.2.cmp(&b.2)));

        let mut out = Vec::new();
        for (id, _, _) in candidates {
            let Some(stored) = self.store.get_mut(&id) else {
                continue;
            };
            let peer_is_dst = peer.addr == stored.bundle.dst;
            // Skip demoted relays for normal traffic (but not for direct
            // delivery or SOS).
            if peer_demoted && !peer_is_dst && stored.bundle.priority != Priority::Sos {
                continue;
            }
            // Bearer fit ("straw, not a firehose"): hold a bulky NORMAL/BULK
            // bundle back from a low-bandwidth link so it waits for a fatter
            // bearer. Emergencies (SOS/ALERT) and the final hop always go — and
            // we do NOT mark it offered, so a better link can still carry it.
            if let Some(cap) = peer.soft_max_bytes {
                let prio = stored.bundle.priority;
                let heavy = stored.size > cap;
                let emergency = prio == Priority::Sos || prio == Priority::Alert;
                if heavy && !peer_is_dst && !emergency {
                    continue;
                }
            }
            let copies = stored.bundle.copies_left;

            let handed = if copies > 1 {
                // Spray phase: give half the budget away, keep the rest.
                let give = copies / 2;
                let keep = copies - give;
                stored.bundle.copies_left = keep;
                Some(give)
            } else if peer_is_dst || peer_helps_gateway {
                // Wait phase: pass the last copy only toward the destination or
                // downhill toward a gateway (DTN escape hatch).
                Some(1)
            } else {
                None
            };

            if let Some(give) = handed {
                stored.offered_to.insert(peer.addr.clone());
                let mut copy = stored.bundle.clone();
                copy.copies_left = give;
                copy.hops = stored.bundle.hops.saturating_add(1);
                self.stats.forwarded_copies += 1;
                out.push(copy);
            }
        }
        self.refresh_store_stats();
        out
    }

    // --- Gateway announces / gradient ---

    /// Record a heard gateway announce at `distance` hops (FR-36, §12.2).
    pub fn observe_announce(&mut self, announce: GatewayAnnounce, distance: u16, now: u64) -> bool {
        let updated = self.gateways.observe(announce, distance, now);
        self.stats.known_gateways = self.gateways.known_count();
        updated
    }

    /// Announces to re-broadcast to neighbours (distance already incremented).
    pub fn announces_to_gossip(&self, now: u64) -> Vec<(GatewayAnnounce, u16)> {
        self.gateways.to_gossip(now)
    }

    /// Drain bundles this internet gateway should push over its uplink (FR-37).
    pub fn drain_bridge(&mut self) -> Vec<Bundle> {
        std::mem::take(&mut self.bridge_out)
    }

    // --- Housekeeping / stats ---

    /// Periodic maintenance: expire bundles and stale gateway entries.
    pub fn tick(&mut self, now: u64) {
        let expired = self.store.evict_expired(now);
        self.stats.dropped_expired += expired as u64;
        self.gateways.expire(now);
        self.stats.known_gateways = self.gateways.known_count();
        self.refresh_store_stats();
    }

    fn refresh_store_stats(&mut self) {
        self.stats.store_len = self.store.len();
        self.stats.store_bytes = self.store.total_bytes();
    }

    pub fn stats(&self) -> &RouterStats {
        &self.stats
    }

    // --- Reputation (FR-47) ---

    /// Reward a relay that contributed to a delivery.
    pub fn credit_relay(&mut self, addr: &Address, w: f32) {
        self.reputation.credit(addr, w);
    }

    /// Penalize a relay observed dropping/stalling traffic (black hole).
    pub fn penalize_relay(&mut self, addr: &Address, w: f32) {
        self.reputation.penalize(addr, w);
    }

    /// This node's reputation view, for gossiping to a peer.
    pub fn reputation(&self) -> crate::reputation::Reputation {
        self.reputation.snapshot()
    }

    /// Merge a peer's gossiped reputation view (pessimistic).
    pub fn merge_reputation(&mut self, other: &crate::reputation::Reputation) {
        self.reputation.gossip_merge(other);
    }

    /// Is `addr` currently demoted at this node?
    pub fn is_demoted(&self, addr: &Address) -> bool {
        self.reputation.is_demoted(addr)
    }

    // --- Black-hole attribution (FR-47) ---
    //
    // The engine feeds three live signals from bundles *we originated*; this layer
    // turns them into reputation credit/penalty. It is passive — it never changes
    // what we store or forward, and penalties are gated so an honest low-contact
    // carrier is not demoted (protecting the ≥95%-delivery target).

    /// A downstream `custodian` signed for our bundle `bundle_id`; watch for its
    /// delivery until `deadline`.
    pub fn note_custody(&mut self, bundle_id: &Bytes, custodian: &Address, deadline: u64) {
        self.attribution
            .record_custody(bundle_id, custodian, deadline);
    }

    /// Our bundle `bundle_id` was delivered (verified receipt): credit every peer
    /// that had taken custody of it.
    pub fn note_delivery(&mut self, bundle_id: &Bytes) {
        for custodian in self.attribution.confirm_delivery(bundle_id) {
            self.reputation
                .credit(&custodian, crate::attribution::CREDIT_W);
        }
    }

    /// Expire overdue custody observations; penalize custodians whose misses have
    /// crossed the grace threshold (likely black holes).
    pub fn attribution_tick(&mut self, now: u64) {
        for custodian in self.attribution.expire(now) {
            self.reputation
                .penalize(&custodian, crate::attribution::PENALIZE_W);
        }
    }

    // --- Adaptive retry (FR-32) ---

    /// Re-spray a bundle we still hold: reset its copy budget and forget which
    /// peers we already offered it to, so it spreads again on new contacts. Used
    /// by the sender when a message stays unverified past its retry window.
    /// Returns `true` if we held the bundle.
    pub fn respray(&mut self, bundle_id: &Bytes, copies: u16) -> bool {
        if let Some(s) = self.store.get_mut(bundle_id) {
            s.bundle.copies_left = copies.max(1);
            s.offered_to.clear();
            true
        } else {
            false
        }
    }

    // --- Custody transfer (FR-25) ---

    /// Whether this node currently holds a copy of `bundle_id` in its store.
    pub fn holds(&self, bundle_id: &Bytes) -> bool {
        self.store.contains(bundle_id)
    }

    /// Release our copy of a bundle because another node has signed for custody
    /// (the caller has verified the [`lifeline_proto::CustodyReceipt`]). Frees
    /// storage without silent loss — the custodian is now responsible. Returns
    /// `true` if a copy was held and dropped.
    pub fn release_custody(&mut self, bundle_id: &Bytes) -> bool {
        let removed = self.store.remove(bundle_id).is_some();
        if removed {
            // Remember it so we don't re-accept our own released copy.
            self.seen.insert(bundle_id.clone());
            self.stats.custody_transfers += 1;
            self.refresh_store_stats();
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lifeline_proto::WIRE_VERSION;

    fn addr(b: u8) -> Address {
        Address::from_hash_bytes([b; 16])
    }

    fn bundle(id: u8, dst: Address, prio: Priority, now: u64) -> Bundle {
        Bundle {
            v: WIRE_VERSION,
            bundle_id: Bytes::new(vec![id; 16]),
            dst,
            src_sealed: Bytes::new(vec![0; 48]),
            priority: prio,
            created_at: now,
            ttl_s: 1000,
            hop_limit: 8,
            hops: 0,
            copies_left: 6,
            ciphertext: Bytes::new(vec![0; 64]),
            postage: None,
            frag: None,
        }
    }

    fn peer(a: Address) -> PeerInfo {
        PeerInfo {
            addr: a,
            is_gateway: false,
            gradient: None,
            known: HashSet::new(),
            soft_max_bytes: None,
        }
    }

    #[test]
    fn delivers_to_self() {
        let mut r = DtnRouter::new(addr(1), RouterConfig::default());
        let b = bundle(1, addr(1), Priority::Normal, 0);
        assert_eq!(r.ingest(b, 10), IngestOutcome::Delivered);
        assert_eq!(r.stats().delivered, 1);
    }

    #[test]
    fn attribution_demotes_a_black_hole_but_not_an_honest_custodian() {
        let mut r = DtnRouter::new(addr(1), RouterConfig::default());
        let black_hole = addr(0xBB);
        let honest = addr(0xAA);

        // The black hole takes custody of many of our bundles that never deliver.
        for i in 0..8u8 {
            let id = Bytes::new(vec![i; 16]);
            r.note_custody(&id, &black_hole, 100);
            r.attribution_tick(200); // past deadline → miss (penalized after grace)
        }
        assert!(
            r.is_demoted(&black_hole),
            "a peer that swallows every bundle should be demoted"
        );

        // The honest peer takes custody of just as many, but they all deliver.
        for i in 0..8u8 {
            let id = Bytes::new(vec![100 + i; 16]);
            r.note_custody(&id, &honest, 100);
            r.note_delivery(&id); // verified receipt came back → credit
        }
        assert!(
            !r.is_demoted(&honest),
            "a custodian that delivers must never be demoted"
        );
    }

    #[test]
    fn attribution_does_not_demote_on_a_few_misses() {
        // An honest low-contact carrier misses a couple of deliveries; the grace
        // window must keep it routable (protecting the delivery target).
        let mut r = DtnRouter::new(addr(1), RouterConfig::default());
        let carrier = addr(0xCC);
        for i in 0..crate::attribution::GRACE_MISSES as u8 {
            let id = Bytes::new(vec![i; 16]);
            r.note_custody(&id, &carrier, 100);
            r.attribution_tick(200);
        }
        assert!(!r.is_demoted(&carrier), "a few misses stay within grace");
    }

    #[test]
    fn inflated_copy_budget_is_clamped_on_ingest() {
        // A malicious peer injects copies_left = u16::MAX to trigger a spray storm.
        let mut r = DtnRouter::new(addr(1), RouterConfig::default());
        let mut b = bundle(2, addr(9), Priority::Normal, 0); // relayed (not for us)
        b.copies_left = u16::MAX;
        assert_eq!(r.ingest(b, 10), IngestOutcome::Stored);
        // Offer it to a peer and confirm the handed budget is bounded, not 32k+.
        let handed = r.offer_to(&peer(addr(2)), 11);
        assert_eq!(handed.len(), 1);
        assert!(
            handed[0].copies_left <= MAX_COPIES,
            "spray budget must be clamped to MAX_COPIES, got {}",
            handed[0].copies_left
        );
    }

    #[test]
    fn demoted_relay_is_skipped_but_sos_and_dst_still_flow() {
        let mut r = DtnRouter::new(addr(1), RouterConfig::default());
        // Two bundles for dst addr(9): one NORMAL, one SOS.
        r.ingest(bundle(40, addr(9), Priority::Normal, 0), 1);
        r.ingest(bundle(41, addr(9), Priority::Sos, 0), 1);

        // Demote a relay peer addr(2) (not the destination).
        r.penalize_relay(&addr(2), 0.99);
        r.penalize_relay(&addr(2), 0.99);
        assert!(r.is_demoted(&addr(2)));

        // Offering to the demoted relay: only the SOS is handed over.
        let offered = r.offer_to(&peer(addr(2)), 2);
        assert_eq!(offered.len(), 1);
        assert_eq!(offered[0].priority, Priority::Sos);

        // A healthy relay addr(3) still receives the NORMAL bundle.
        let offered3 = r.offer_to(&peer(addr(3)), 2);
        assert!(offered3.iter().any(|b| b.priority == Priority::Normal));
    }

    #[test]
    fn direct_delivery_to_demoted_recipient_still_allowed() {
        let mut r = DtnRouter::new(addr(1), RouterConfig::default());
        r.ingest(bundle(42, addr(9), Priority::Normal, 0), 1);
        // Even if the *recipient* somehow got demoted, deliver directly to them.
        r.penalize_relay(&addr(9), 0.99);
        r.penalize_relay(&addr(9), 0.99);
        let offered = r.offer_to(&peer(addr(9)), 2);
        assert_eq!(offered.len(), 1);
    }

    #[test]
    fn dedup_suppresses_repeats() {
        let mut r = DtnRouter::new(addr(1), RouterConfig::default());
        let b = bundle(2, addr(9), Priority::Normal, 0);
        assert_eq!(r.ingest(b.clone(), 10), IngestOutcome::Stored);
        assert_eq!(r.ingest(b, 10), IngestOutcome::Duplicate);
    }

    #[test]
    fn drops_expired_and_hoplimit() {
        let mut r = DtnRouter::new(addr(1), RouterConfig::default());
        let mut b = bundle(3, addr(9), Priority::Normal, 0);
        b.ttl_s = 5;
        assert!(matches!(
            r.ingest(b, 100),
            IngestOutcome::Dropped("expired")
        ));

        let mut b2 = bundle(4, addr(9), Priority::Normal, 0);
        b2.hops = 9; // > hop_limit 8
        assert!(matches!(
            r.ingest(b2, 1),
            IngestOutcome::Dropped("hop limit")
        ));
    }

    #[test]
    fn binary_spray_splits_budget() {
        let mut r = DtnRouter::new(addr(1), RouterConfig::default());
        r.ingest(bundle(5, addr(9), Priority::Normal, 0), 1);
        let offered = r.offer_to(&peer(addr(2)), 2);
        assert_eq!(offered.len(), 1);
        assert_eq!(offered[0].copies_left, 3); // gave floor(6/2)
        assert_eq!(offered[0].hops, 1);
        // Local keeps the other half.
        let kept = r.known_ids();
        assert!(kept.contains(&Bytes::new(vec![5; 16])));
    }

    #[test]
    fn wait_phase_only_forwards_to_dst_or_gateway() {
        let mut r = DtnRouter::new(addr(1), RouterConfig::default());
        let mut b = bundle(6, addr(9), Priority::Normal, 0);
        b.copies_left = 1; // wait phase
        r.ingest(b, 1);
        // Random peer that is neither dst nor a gateway: no offer.
        assert!(r.offer_to(&peer(addr(2)), 2).is_empty());
        // The destination itself: offered.
        let offered = r.offer_to(&peer(addr(9)), 2);
        assert_eq!(offered.len(), 1);
    }

    #[test]
    fn sos_offered_before_normal() {
        let mut r = DtnRouter::new(addr(1), RouterConfig::default());
        r.ingest(bundle(10, addr(9), Priority::Normal, 0), 1);
        r.ingest(bundle(11, addr(9), Priority::Sos, 0), 1);
        let offered = r.offer_to(&peer(addr(2)), 2);
        // SOS bundle (id 11) must be first.
        assert_eq!(offered[0].bundle_id, Bytes::new(vec![11; 16]));
        assert_eq!(offered[0].priority, Priority::Sos);
    }

    #[test]
    fn postage_gates_bulk_but_not_sos() {
        let cfg = RouterConfig {
            require_postage: true,
            ..RouterConfig::default()
        };
        let mut r = DtnRouter::new(addr(1), cfg);

        // BULK with no postage → dropped.
        let mut spam = bundle(30, addr(9), Priority::Bulk, 0);
        spam.postage = None;
        assert!(matches!(
            r.ingest(spam, 1),
            IngestOutcome::Dropped("no postage")
        ));

        // BULK with valid postage → stored.
        let mut ok = bundle(31, addr(9), Priority::Bulk, 0);
        ok.postage =
            lifeline_proto::pow::mint_for_priority(ok.bundle_id.as_slice(), Priority::Bulk);
        assert_eq!(r.ingest(ok, 1), IngestOutcome::Stored);

        // SOS with no postage → still stored (exempt).
        let mut sos = bundle(32, addr(9), Priority::Sos, 0);
        sos.postage = None;
        assert_eq!(r.ingest(sos, 1), IngestOutcome::Stored);
    }

    #[test]
    fn respray_resets_budget_and_offered_set() {
        let mut r = DtnRouter::new(addr(1), RouterConfig::default());
        r.ingest(bundle(60, addr(9), Priority::Normal, 0), 1);
        // Offer to a peer: consumes budget and records the peer.
        let first = r.offer_to(&peer(addr(2)), 2);
        assert_eq!(first.len(), 1);
        // Same peer, no re-offer.
        assert!(r.offer_to(&peer(addr(2)), 3).is_empty());
        // Respray → budget restored + offered set cleared → offers again.
        assert!(r.respray(&Bytes::new(vec![60; 16]), 6));
        let again = r.offer_to(&peer(addr(2)), 4);
        assert_eq!(again.len(), 1, "respray must let it re-offer");
    }

    #[test]
    fn release_custody_drops_held_copy() {
        let mut r = DtnRouter::new(addr(1), RouterConfig::default());
        let b = bundle(50, addr(9), Priority::Normal, 0);
        let id = b.bundle_id.clone();
        r.ingest(b, 1);
        assert_eq!(r.stats().store_len, 1);

        // Another node took custody → release our copy.
        assert!(r.release_custody(&id));
        assert_eq!(r.stats().store_len, 0);
        assert_eq!(r.stats().custody_transfers, 1);
        // We won't re-accept the released copy (dedup via `seen`).
        let dup = bundle(50, addr(9), Priority::Normal, 0);
        assert_eq!(r.ingest(dup, 1), IngestOutcome::Duplicate);
    }

    #[test]
    fn internet_gateway_bridges_nonlocal() {
        let mut r = DtnRouter::new(addr(1), RouterConfig::default());
        r.set_gateway(vec![CAP_INTERNET.into()]);
        r.ingest(bundle(20, addr(9), Priority::Normal, 0), 1);
        let bridged = r.drain_bridge();
        assert_eq!(bridged.len(), 1);
        assert_eq!(bridged[0].bundle_id, Bytes::new(vec![20; 16]));
        // Draining twice yields nothing.
        assert!(r.drain_bridge().is_empty());
    }
}
