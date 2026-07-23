//! Version vector — per-actor causal context (PRD §12.3 "CRDT version vectors").

use lifeline_proto::Address;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A dot is a single, globally-unique event: `(actor, counter)`.
pub type Dot = (Address, u64);

/// Maps each actor to the highest contiguous counter observed from it. Because
/// per-actor counters are dense and delivered in order under full-state merge,
/// "have I seen dot `(a, n)`?" reduces to `n <= self.get(a)`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionVector(pub BTreeMap<Address, u64>);

impl VersionVector {
    pub fn new() -> Self {
        VersionVector(BTreeMap::new())
    }

    /// Highest counter seen for `actor` (0 if none).
    pub fn get(&self, actor: &Address) -> u64 {
        self.0.get(actor).copied().unwrap_or(0)
    }

    /// Allocate the next counter for `actor`, advancing the vector. Returns the
    /// new counter, forming a fresh dot `(actor, counter)`.
    pub fn increment(&mut self, actor: &Address) -> u64 {
        let c = self.0.entry(actor.clone()).or_insert(0);
        *c += 1;
        *c
    }

    /// Has this vector observed `dot`?
    pub fn contains(&self, dot: &Dot) -> bool {
        dot.1 <= self.get(&dot.0)
    }

    /// Merge another vector in (pointwise max) — idempotent and commutative.
    pub fn merge(&mut self, other: &VersionVector) {
        for (actor, &c) in &other.0 {
            let e = self.0.entry(actor.clone()).or_insert(0);
            if c > *e {
                *e = c;
            }
        }
    }

    /// Pointwise **minimum** with another vector (the "meet"). An actor absent
    /// from either side contributes 0, so it is dropped. Folding `meet` across
    /// every replica's context yields the **causal-stability frontier**: dots at
    /// or below it have been observed everywhere and are safe to garbage-collect
    /// (Shapiro 2011 — "budget for CRDT metadata GC").
    pub fn meet(&self, other: &VersionVector) -> VersionVector {
        let mut out = std::collections::BTreeMap::new();
        for (actor, &c) in &self.0 {
            let m = c.min(other.get(actor));
            if m > 0 {
                out.insert(actor.clone(), m);
            }
        }
        VersionVector(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(b: u8) -> Address {
        Address::from_hash_bytes([b; 16])
    }

    #[test]
    fn increment_and_contains() {
        let mut vv = VersionVector::new();
        assert_eq!(vv.increment(&a(1)), 1);
        assert_eq!(vv.increment(&a(1)), 2);
        assert!(vv.contains(&(a(1), 2)));
        assert!(!vv.contains(&(a(1), 3)));
        assert!(!vv.contains(&(a(2), 1)));
    }

    #[test]
    fn merge_is_pointwise_max() {
        let mut x = VersionVector::new();
        x.increment(&a(1));
        x.increment(&a(1)); // a1 -> 2
        let mut y = VersionVector::new();
        y.increment(&a(1)); // a1 -> 1
        y.increment(&a(2)); // a2 -> 1
        x.merge(&y);
        assert_eq!(x.get(&a(1)), 2);
        assert_eq!(x.get(&a(2)), 1);
    }
}
