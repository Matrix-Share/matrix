//! Aggregate shared state a node syncs on contact (PRD FR-33).
//!
//! Bundles the mesh's partition-surviving state into one mergeable object:
//! * **group membership** — `group_id → OrSwot<Address>` (FR-12).
//! * **blocklist** — `OrSwot<Address>` of blocked keys (FR-48).
//! * **presence** — `Address → Lww<Presence>`.
//! * **delivery status** — a grow-only `OrSwot<bundle_id>` of ids known
//!   delivered, so delivery state converges across the mesh (FR-32/33).
//!
//! A single Lamport clock drives LWW timestamps and advances on both local
//! mutation and merge, so causal ordering is respected across partitions.

use crate::lww::Lww;
use crate::orswot::OrSwot;
use lifeline_proto::{Address, Bytes};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Coarse presence a node advertises to its groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    Offline,
    Online,
    /// Actively relaying / well-placed (PRD relay role).
    Relay,
    /// SOS active.
    Sos,
}

/// The mergeable shared state owned by one node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedState {
    actor: Address,
    /// Lamport clock for LWW writes.
    clock: u64,
    groups: BTreeMap<String, OrSwot<Address>>,
    blocklist: OrSwot<Address>,
    presence: BTreeMap<Address, Lww<Presence>>,
    delivered: OrSwot<Bytes>,
}

impl SharedState {
    pub fn new(actor: Address) -> Self {
        SharedState {
            actor,
            clock: 0,
            groups: BTreeMap::new(),
            blocklist: OrSwot::new(),
            presence: BTreeMap::new(),
            delivered: OrSwot::new(),
        }
    }

    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    // --- Group membership (FR-12) ---

    /// Add `member` to `group` (creating the group if needed).
    pub fn add_member(&mut self, group: &str, member: Address) {
        let actor = self.actor.clone();
        self.groups
            .entry(group.to_string())
            .or_default()
            .add(&actor, member);
    }

    /// Remove `member` from `group`.
    pub fn remove_member(&mut self, group: &str, member: &Address) {
        if let Some(g) = self.groups.get_mut(group) {
            g.remove(member);
        }
    }

    /// Sorted current membership of `group`.
    pub fn members(&self, group: &str) -> Vec<Address> {
        self.groups
            .get(group)
            .map(|g| g.elements())
            .unwrap_or_default()
    }

    pub fn is_member(&self, group: &str, member: &Address) -> bool {
        self.groups.get(group).is_some_and(|g| g.contains(member))
    }

    // --- Blocklist (FR-48) ---

    pub fn block(&mut self, who: Address) {
        let actor = self.actor.clone();
        self.blocklist.add(&actor, who);
    }

    pub fn unblock(&mut self, who: &Address) {
        self.blocklist.remove(who);
    }

    pub fn is_blocked(&self, who: &Address) -> bool {
        self.blocklist.contains(who)
    }

    // --- Presence ---

    pub fn set_presence(&mut self, who: Address, presence: Presence) {
        let ts = self.tick();
        let actor = self.actor.clone();
        self.presence
            .entry(who)
            .and_modify(|r| r.set(presence, ts, actor.clone()))
            .or_insert_with(|| Lww::new(presence, ts, actor));
    }

    pub fn presence_of(&self, who: &Address) -> Option<Presence> {
        self.presence.get(who).map(|r| *r.value())
    }

    // --- Delivery status (FR-32/33) ---

    pub fn mark_delivered(&mut self, bundle_id: Bytes) {
        let actor = self.actor.clone();
        self.delivered.add(&actor, bundle_id);
    }

    pub fn is_delivered(&self, bundle_id: &Bytes) -> bool {
        self.delivered.contains(bundle_id)
    }

    /// Number of delivery facts currently materialized (metadata size proxy).
    pub fn delivered_len(&self) -> usize {
        self.delivered.elements().len()
    }

    /// Causal context of the delivered-set — fold [`VersionVector::meet`] across
    /// every replica's copy of this to obtain the stability frontier for GC.
    pub fn delivered_context(&self) -> &crate::VersionVector {
        self.delivered.context()
    }

    /// Garbage-collect causally-stable delivery facts (Shapiro 2011). `stable`
    /// must be the pointwise-min of the delivered contexts across all replicas
    /// (see [`SharedState::delivered_context`]). Returns how many were pruned.
    /// The authoritative proof of delivery is still the signed receipt/log; this
    /// only bounds the shared *cache* of who-has-been-delivered.
    pub fn gc_delivered(&mut self, stable: &crate::VersionVector) -> usize {
        self.delivered.gc(stable)
    }

    // --- Anti-entropy (§12.3) ---

    /// Merge another node's state into ours. On contact both peers call this in
    /// each direction; the result is identical on both sides (convergence).
    pub fn merge(&mut self, other: &SharedState) {
        for (gid, og) in &other.groups {
            self.groups.entry(gid.clone()).or_default().merge(og);
        }
        self.blocklist.merge(&other.blocklist);
        self.delivered.merge(&other.delivered);
        for (who, or) in &other.presence {
            self.presence
                .entry(who.clone())
                .and_modify(|r| r.merge(or))
                .or_insert_with(|| or.clone());
        }
        // Advance our clock past everything we just absorbed.
        if other.clock > self.clock {
            self.clock = other.clock;
        }
        self.clock += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(b: u8) -> Address {
        Address::from_hash_bytes([b; 16])
    }

    /// FR-33 AC: "Two partitions with divergent group edits merge with no
    /// conflicts and identical final state on reconnect."
    #[test]
    fn divergent_partitions_converge_identically() {
        let g = "relief-team";
        // Both partitions start from the same shared history.
        let mut left = SharedState::new(a(1));
        left.add_member(g, a(10));
        left.add_member(g, a(11));
        let mut right = left.clone();
        right = SharedState {
            actor: a(2),
            ..right
        }; // right is a different actor

        // Partition A (left): add 12, remove 10, block a spammer.
        left.add_member(g, a(12));
        left.remove_member(g, &a(10));
        left.block(a(99));
        left.set_presence(a(1), Presence::Relay);

        // Partition B (right): concurrently add 13, re-add 10, set presence.
        right.add_member(g, a(13));
        right.add_member(g, a(10)); // concurrent re-add — add wins
        right.set_presence(a(2), Presence::Online);

        // Reconnect: exchange and merge both directions.
        let mut merged_l = left.clone();
        merged_l.merge(&right);
        let mut merged_r = right.clone();
        merged_r.merge(&left);

        // Identical membership on both sides.
        assert_eq!(merged_l.members(g), merged_r.members(g));
        // 10 was concurrently removed (A) and re-added (B) → add wins.
        assert!(merged_l.is_member(g, &a(10)));
        // Both unique adds survive.
        assert!(merged_l.is_member(g, &a(12)));
        assert!(merged_l.is_member(g, &a(13)));
        // Blocklist propagated.
        assert!(merged_r.is_blocked(&a(99)));
        // Presence from both partitions preserved.
        assert_eq!(merged_l.presence_of(&a(1)), Some(Presence::Relay));
        assert_eq!(merged_l.presence_of(&a(2)), Some(Presence::Online));
    }

    #[test]
    fn merge_order_independent() {
        let g = "grp";
        let mut s0 = SharedState::new(a(1));
        s0.add_member(g, a(5));
        let mut b = SharedState {
            actor: a(2),
            ..s0.clone()
        };
        let mut c = SharedState {
            actor: a(3),
            ..s0.clone()
        };
        b.add_member(g, a(6));
        c.add_member(g, a(7));

        // (s0 merge b) merge c  vs  (s0 merge c) merge b
        let mut left = s0.clone();
        left.merge(&b);
        left.merge(&c);
        let mut right = s0.clone();
        right.merge(&c);
        right.merge(&b);
        assert_eq!(left.members(g), right.members(g));
    }

    /// Shapiro "budget for GC": causally-stable delivery facts can be pruned to
    /// bound metadata, without breaking convergence or resurrecting on merge.
    #[test]
    fn delivered_gc_prunes_stable_without_resurrection() {
        let mut x = SharedState::new(a(1));
        let mut y = SharedState::new(a(2));
        let stable_id = Bytes::new(vec![1; 16]);
        let fresh_id = Bytes::new(vec![2; 16]);

        // Both observe stable_id (it will be causally stable everywhere).
        x.mark_delivered(stable_id.clone());
        x.merge(&y);
        y.merge(&x);
        // y additionally delivers a fresh id that x has NOT yet seen.
        y.mark_delivered(fresh_id.clone());

        // Stability frontier = pointwise-min of both delivered contexts.
        let stable = x.delivered_context().meet(y.delivered_context());

        // GC on x: stable_id is pruned; nothing else to prune.
        let pruned = x.gc_delivered(&stable);
        assert_eq!(pruned, 1);
        assert!(!x.is_delivered(&stable_id), "stable fact pruned from cache");

        // Merging y (which still carries stable_id) must NOT resurrect it —
        // x's causal context still covers that dot.
        x.merge(&y);
        assert!(
            !x.is_delivered(&stable_id),
            "GC'd stable fact must not resurrect"
        );
        // The fresh, not-yet-stable fact still propagates normally.
        assert!(x.is_delivered(&fresh_id));
    }

    #[test]
    fn delivery_status_converges() {
        let mut x = SharedState::new(a(1));
        let mut y = SharedState::new(a(2));
        x.mark_delivered(Bytes::new(vec![1; 16]));
        y.mark_delivered(Bytes::new(vec![2; 16]));
        x.merge(&y);
        assert!(x.is_delivered(&Bytes::new(vec![1; 16])));
        assert!(x.is_delivered(&Bytes::new(vec![2; 16])));
    }
}
