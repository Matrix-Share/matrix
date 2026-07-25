//! Relay reputation (PRD FR-47, §8.9; network-layer "black-hole relays").
//!
//! Replication + missing end-to-end receipts expose non-delivery; this module
//! turns those observations into a per-relay score, gossips it, and lets the
//! router **route around demoted relays**. A node that repeatedly takes custody
//! of bundles that never make progress is demoted; honest relays that
//! contribute to deliveries are credited.
//!
//! Scores are a soft routing *hint*, not consensus. Gossip is **pessimistic**
//! (bad news dominates via a min-blend) so a black hole flagged anywhere is
//! avoided everywhere; the known tradeoff is that a malicious node can also
//! defame an honest one, which a future web-of-trust weighting (FR-47) should
//! bound. This is deliberately not a global ledger (that stays out of the
//! delivery path).

use lifeline_proto::Address;
use std::collections::BTreeMap;

/// Neutral starting score for an unknown relay.
pub const DEFAULT_SCORE: f32 = 0.5;
/// Below this, a relay is avoided when alternatives exist.
pub const DEMOTE_THRESHOLD: f32 = 0.25;
/// Cap on distinct tracked relays — bounds memory against a gossiped snapshot
/// stuffed with millions of fabricated addresses. When full, we only accept new
/// entries that are *demotions* (bad news, which is the point of gossip); neutral
/// or positive scores for unknown relays are dropped.
pub const MAX_TRACKED: usize = 100_000;

/// Per-relay reputation held by one node.
#[derive(Debug, Clone, Default)]
pub struct Reputation {
    scores: BTreeMap<Address, f32>,
}

impl Reputation {
    pub fn new() -> Self {
        Reputation::default()
    }

    /// Current score for `addr` (defaults to neutral).
    pub fn score(&self, addr: &Address) -> f32 {
        self.scores.get(addr).copied().unwrap_or(DEFAULT_SCORE)
    }

    /// Reward good behavior — move the score toward 1.0 by weight `w` (0..1).
    pub fn credit(&mut self, addr: &Address, w: f32) {
        let s = self.score(addr);
        self.scores
            .insert(addr.clone(), (s + w * (1.0 - s)).clamp(0.0, 1.0));
    }

    /// Penalize bad behavior — move the score toward 0.0 by weight `w` (0..1).
    pub fn penalize(&mut self, addr: &Address, w: f32) {
        let s = self.score(addr);
        self.scores
            .insert(addr.clone(), (s - w * s).clamp(0.0, 1.0));
    }

    /// Is this relay demoted (avoid if another path exists)?
    pub fn is_demoted(&self, addr: &Address) -> bool {
        self.score(addr) < DEMOTE_THRESHOLD
    }

    /// Merge a peer's reputation view in (pessimistic: keep the lower score, so
    /// a demotion propagates across the mesh). Bounded: once at capacity, we only
    /// accept gossip about relays we already track or that arrives as a demotion,
    /// so a snapshot stuffed with fabricated addresses can't exhaust memory.
    pub fn gossip_merge(&mut self, other: &Reputation) {
        for (addr, &their) in &other.scores {
            let known = self.scores.contains_key(addr);
            if !known && (self.scores.len() >= MAX_TRACKED || their >= DEFAULT_SCORE) {
                // Don't grow the map for an unknown, non-demoted relay.
                continue;
            }
            let mine = self.score(addr);
            self.scores.insert(addr.clone(), mine.min(their));
        }
    }

    /// Snapshot for gossiping to a peer.
    pub fn snapshot(&self) -> Reputation {
        self.clone()
    }

    /// Relays currently demoted (diagnostics).
    pub fn demoted(&self) -> Vec<Address> {
        self.scores
            .iter()
            .filter(|(_, &s)| s < DEMOTE_THRESHOLD)
            .map(|(a, _)| a.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(b: u8) -> Address {
        Address::from_hash_bytes([b; 16])
    }

    #[test]
    fn credit_and_penalize_move_score() {
        let mut r = Reputation::new();
        assert_eq!(r.score(&a(1)), DEFAULT_SCORE);
        r.credit(&a(1), 0.5);
        assert!(r.score(&a(1)) > DEFAULT_SCORE);
        r.penalize(&a(2), 0.9);
        r.penalize(&a(2), 0.9);
        assert!(r.is_demoted(&a(2)));
    }

    #[test]
    fn gossip_spreads_demotion() {
        // Node X observed relay R as a black hole; node Y hasn't.
        let mut x = Reputation::new();
        x.penalize(&a(9), 0.95);
        x.penalize(&a(9), 0.95);
        assert!(x.is_demoted(&a(9)));

        let mut y = Reputation::new();
        assert!(!y.is_demoted(&a(9)));
        y.gossip_merge(&x);
        assert!(y.is_demoted(&a(9)), "demotion must propagate via gossip");
    }

    #[test]
    fn gossip_is_pessimistic() {
        let mut x = Reputation::new();
        x.credit(&a(5), 0.9); // X thinks well of relay 5
        let mut y = Reputation::new();
        y.penalize(&a(5), 0.9); // Y thinks poorly
        let low = y.score(&a(5));
        x.gossip_merge(&y);
        assert_eq!(x.score(&a(5)), low, "the lower score must win");
    }
}
