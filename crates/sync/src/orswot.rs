//! ORSWOT — Observed-Remove Set Without Tombstones (add-wins OR-Set).
//!
//! An add-wins set that supports concurrent add/remove and converges without
//! keeping tombstones, using a version vector as causal context (the Riak
//! `riak_dt_orswot` design). This is the CRDT the PRD calls for behind "group
//! membership as a CRDT set" and shared blocklists (FR-12, FR-33, FR-48).
//!
//! Semantics: if two replicas concurrently **add** and **remove** the same
//! element, the **add wins** — the safe default for membership (you don't
//! silently drop someone who was concurrently re-added).

pub use crate::vclock::Dot;
use crate::vclock::VersionVector;
use lifeline_proto::Address;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// An observed-remove set of elements `E`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "E: Ord + Clone + Serialize + serde::de::DeserializeOwned")]
pub struct OrSwot<E: Ord + Clone> {
    /// Element → the set of dots currently supporting its presence.
    entries: BTreeMap<E, BTreeSet<Dot>>,
    /// Causal context: every dot this replica has ever observed.
    cc: VersionVector,
}

impl<E: Ord + Clone> Default for OrSwot<E> {
    fn default() -> Self {
        OrSwot {
            entries: BTreeMap::new(),
            cc: VersionVector::new(),
        }
    }
}

impl<E: Ord + Clone> OrSwot<E> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Is `elem` currently in the set?
    pub fn contains(&self, elem: &E) -> bool {
        self.entries.contains_key(elem)
    }

    /// Current members, sorted.
    pub fn elements(&self) -> Vec<E> {
        self.entries.keys().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Add `elem` on behalf of `actor`. A fresh dot supersedes any prior dots
    /// for this element that this replica has already observed (optimized add).
    pub fn add(&mut self, actor: &Address, elem: E) {
        let counter = self.cc.increment(actor);
        let dot: Dot = (actor.clone(), counter);
        let mut dots = BTreeSet::new();
        dots.insert(dot);
        self.entries.insert(elem, dots);
    }

    /// Remove `elem`. Its supporting dots are already covered by `cc`, so no
    /// tombstone is needed; a concurrent add (a dot `cc` hasn't seen) survives.
    pub fn remove(&mut self, elem: &E) {
        self.entries.remove(elem);
    }

    /// Merge `other` into `self` — the CRDT join. Commutative, associative,
    /// idempotent.
    pub fn merge(&mut self, other: &OrSwot<E>) {
        let mut result: BTreeMap<E, BTreeSet<Dot>> = BTreeMap::new();

        let mut elems: BTreeSet<&E> = BTreeSet::new();
        elems.extend(self.entries.keys());
        elems.extend(other.entries.keys());

        for e in elems {
            let a_dots = self.entries.get(e);
            let b_dots = other.entries.get(e);
            let mut kept: BTreeSet<Dot> = BTreeSet::new();

            if let Some(ad) = a_dots {
                for d in ad {
                    // Keep our dot if the other replica also has it, or hasn't
                    // observed it yet (a concurrent add, not a remove).
                    let in_b = b_dots.is_some_and(|bd| bd.contains(d));
                    if in_b || !other.cc.contains(d) {
                        kept.insert(d.clone());
                    }
                }
            }
            if let Some(bd) = b_dots {
                for d in bd {
                    let in_a = a_dots.is_some_and(|ad| ad.contains(d));
                    // Their dot we lack: keep only if we've never observed it.
                    if !in_a && !self.cc.contains(d) {
                        kept.insert(d.clone());
                    }
                }
            }
            if !kept.is_empty() {
                result.insert(e.clone(), kept);
            }
        }

        self.entries = result;
        self.cc.merge(&other.cc);
    }

    /// Expose the causal context (used by [`crate::SharedState`] summaries and
    /// to compute a stability frontier for GC).
    pub fn context(&self) -> &VersionVector {
        &self.cc
    }

    /// Garbage-collect elements that are **causally stable** — every one of an
    /// element's supporting dots is `<= stable` (i.e. observed by all replicas
    /// that contributed to `stable`). Such elements can be dropped from the
    /// materialized set without breaking convergence: the causal context (`cc`)
    /// is retained, so a later merge from a replica that still carries the
    /// element will *not* resurrect it (its dots are already covered by `cc`).
    ///
    /// This bounds an otherwise ever-growing set (Shapiro 2011). Returns how
    /// many elements were collected. Intended for grow-only caches such as the
    /// delivered-bundle set — not for membership sets you still query far in the
    /// future.
    pub fn gc(&mut self, stable: &VersionVector) -> usize {
        let to_drop: Vec<E> = self
            .entries
            .iter()
            .filter(|(_, dots)| dots.iter().all(|d| stable.contains(d)))
            .map(|(e, _)| e.clone())
            .collect();
        let n = to_drop.len();
        for e in to_drop {
            self.entries.remove(&e);
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(b: u8) -> Address {
        Address::from_hash_bytes([b; 16])
    }

    #[test]
    fn add_remove_basic() {
        let mut s: OrSwot<String> = OrSwot::new();
        s.add(&a(1), "asha".into());
        s.add(&a(1), "ravi".into());
        assert!(s.contains(&"asha".into()));
        s.remove(&"asha".into());
        assert!(!s.contains(&"asha".into()));
        assert!(s.contains(&"ravi".into()));
    }

    #[test]
    fn merge_is_commutative_and_idempotent() {
        let mut x: OrSwot<String> = OrSwot::new();
        let mut y: OrSwot<String> = OrSwot::new();
        x.add(&a(1), "x".into());
        y.add(&a(2), "y".into());

        let mut xy = x.clone();
        xy.merge(&y);
        let mut yx = y.clone();
        yx.merge(&x);
        assert_eq!(xy.elements(), yx.elements());

        // Idempotent: merging again changes nothing.
        let before = xy.clone();
        xy.merge(&y);
        xy.merge(&x);
        assert_eq!(xy, before);
    }

    #[test]
    fn concurrent_add_wins_over_remove() {
        // Both start knowing "meera".
        let mut x: OrSwot<String> = OrSwot::new();
        x.add(&a(1), "meera".into());
        let mut y = x.clone();

        // Concurrently: X removes meera; Y re-adds meera (new dot).
        x.remove(&"meera".into());
        y.add(&a(2), "meera".into());

        // Merge both ways — add must win.
        let mut xy = x.clone();
        xy.merge(&y);
        let mut yx = y.clone();
        yx.merge(&x);
        assert!(xy.contains(&"meera".into()), "add-wins violated");
        assert_eq!(xy, yx);
    }

    #[test]
    fn remove_after_observed_add_sticks() {
        let mut x: OrSwot<String> = OrSwot::new();
        x.add(&a(1), "spam".into());
        let mut y = x.clone(); // y has observed the add
        y.remove(&"spam".into()); // y removes what it observed
        x.merge(&y);
        assert!(!x.contains(&"spam".into()), "observed remove should stick");
    }
}
