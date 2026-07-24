//! A fixed-capacity insertion-ordered set — an LRU-ish membership cache.
//!
//! Several mesh-control collections (dedup ids, bridged ids) would otherwise
//! grow forever as an attacker feeds unique `bundle_id`s, giving an unbounded
//! memory-exhaustion DoS that the store's byte cap does not cover. This bounds
//! them: once at capacity, inserting a new id evicts the oldest. TTL-bounded
//! bundles can never usefully return after eviction, so the only cost of an
//! evicted id is a possible re-process of a very old bundle — never unbounded
//! memory.

use std::collections::{HashSet, VecDeque};
use std::hash::Hash;

/// An insertion-ordered set capped at `cap` entries (oldest evicted first).
#[derive(Debug, Clone)]
pub struct BoundedSet<T: Eq + Hash + Clone> {
    set: HashSet<T>,
    order: VecDeque<T>,
    cap: usize,
}

impl<T: Eq + Hash + Clone> BoundedSet<T> {
    pub fn new(cap: usize) -> Self {
        BoundedSet {
            set: HashSet::new(),
            order: VecDeque::new(),
            cap: cap.max(1),
        }
    }

    pub fn contains(&self, item: &T) -> bool {
        self.set.contains(item)
    }

    /// Insert `item`. Returns true if it was newly added. Evicts the oldest entry
    /// if that pushes the set over capacity.
    pub fn insert(&mut self, item: T) -> bool {
        if !self.set.insert(item.clone()) {
            return false;
        }
        self.order.push_back(item);
        while self.order.len() > self.cap {
            if let Some(old) = self.order.pop_front() {
                self.set.remove(&old);
            }
        }
        true
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.set.len()
    }

    /// Iterate the current members (order unspecified).
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.set.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_oldest_over_capacity() {
        let mut s = BoundedSet::new(3);
        assert!(s.insert(1));
        assert!(s.insert(2));
        assert!(s.insert(3));
        assert!(!s.insert(2), "duplicate insert returns false");
        s.insert(4); // evicts 1 (oldest)
        assert!(!s.contains(&1), "oldest evicted");
        assert!(s.contains(&2) && s.contains(&3) && s.contains(&4));
        assert_eq!(s.len(), 3, "never exceeds capacity");
    }
}
