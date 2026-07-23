//! Gateway announce cache + forwarding gradient (FR-29, FR-36, §12.2).
//!
//! Gateways emit **signed announces** that propagate a few hops. Each node
//! caches the best (freshest) announce per gateway and tracks how many hops
//! away it heard it. The minimum such distance is the node's **gradient** —
//! its estimated hops-to-nearest-gateway — which biases forwarding of
//! gateway-bound bundles "downhill".

use lifeline_proto::{Address, GatewayAnnounce};
use std::collections::HashMap;

/// How many hops a gateway announce is allowed to propagate (PRD §12.2:
/// "propagate a few hops"). Also caps the gradient value.
pub const GATEWAY_ANNOUNCE_HOPS: u16 = 6;

#[derive(Debug, Clone)]
struct CachedAnnounce {
    announce: GatewayAnnounce,
    /// Hops this announce traveled to reach us (0 = we are the gateway).
    distance: u16,
}

/// Per-node cache of known gateways.
#[derive(Debug, Default)]
pub struct GatewayCache {
    by_gateway: HashMap<Address, CachedAnnounce>,
}

impl GatewayCache {
    pub fn new() -> Self {
        GatewayCache::default()
    }

    /// Record a heard announce at `distance` hops. Keeps the entry with the
    /// highest `seq` (freshest); on equal seq keeps the shorter distance.
    /// Returns `true` if this updated our knowledge (worth re-gossiping).
    pub fn observe(&mut self, announce: GatewayAnnounce, distance: u16, now: u64) -> bool {
        if !announce.is_fresh(now) || distance > GATEWAY_ANNOUNCE_HOPS {
            return false;
        }
        match self.by_gateway.get(&announce.gateway) {
            Some(existing)
                if existing.announce.seq > announce.seq
                    || (existing.announce.seq == announce.seq && existing.distance <= distance) =>
            {
                false
            }
            _ => {
                self.by_gateway.insert(
                    announce.gateway.clone(),
                    CachedAnnounce { announce, distance },
                );
                true
            }
        }
    }

    /// Drop stale announces (past `expires_at`).
    pub fn expire(&mut self, now: u64) {
        self.by_gateway.retain(|_, c| c.announce.is_fresh(now));
    }

    /// The node's gradient: minimum hop distance to any fresh gateway.
    /// `None` if no gateway is known.
    pub fn gradient(&self, now: u64) -> Option<u16> {
        self.by_gateway
            .values()
            .filter(|c| c.announce.is_fresh(now))
            .map(|c| c.distance)
            .min()
    }

    /// Announces to re-gossip to neighbours, each with an incremented distance
    /// (so downstream nodes compute a correct gradient). Announces already at
    /// the hop cap are not propagated further.
    pub fn to_gossip(&self, now: u64) -> Vec<(GatewayAnnounce, u16)> {
        self.by_gateway
            .values()
            .filter(|c| c.announce.is_fresh(now) && c.distance < GATEWAY_ANNOUNCE_HOPS)
            .map(|c| (c.announce.clone(), c.distance + 1))
            .collect()
    }

    pub fn known_count(&self) -> usize {
        self.by_gateway.len()
    }
}
