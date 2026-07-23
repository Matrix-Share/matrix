//! Last-Writer-Wins register with a deterministic tie-break.
//!
//! Used for single-value shared fields like presence, where the newest write
//! should win. Concurrent writes at the same logical time are broken by the
//! writer's [`Address`] so every replica picks the *same* winner (determinism
//! is what makes the merge conflict-free).

use lifeline_proto::Address;
use serde::{Deserialize, Serialize};

/// A last-writer-wins register over value `V`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "V: Clone + Serialize + serde::de::DeserializeOwned")]
pub struct Lww<V: Clone> {
    value: V,
    /// Logical (Lamport-style) timestamp of the write.
    ts: u64,
    /// Writer, for deterministic tie-break at equal `ts`.
    actor: Address,
}

impl<V: Clone> Lww<V> {
    pub fn new(value: V, ts: u64, actor: Address) -> Self {
        Lww { value, ts, actor }
    }

    pub fn value(&self) -> &V {
        &self.value
    }

    pub fn timestamp(&self) -> u64 {
        self.ts
    }

    /// Apply a local write if it is newer than the current one.
    pub fn set(&mut self, value: V, ts: u64, actor: Address) {
        if Self::wins(ts, &actor, self.ts, &self.actor) {
            self.value = value;
            self.ts = ts;
            self.actor = actor;
        }
    }

    /// Merge another register in, keeping the winning write.
    pub fn merge(&mut self, other: &Lww<V>) {
        if Self::wins(other.ts, &other.actor, self.ts, &self.actor) {
            self.value = other.value.clone();
            self.ts = other.ts;
            self.actor = other.actor.clone();
        }
    }

    /// Does write `(ts_a, actor_a)` beat `(ts_b, actor_b)`? Higher ts wins; ties
    /// broken by the larger actor address (total, deterministic order).
    fn wins(ts_a: u64, actor_a: &Address, ts_b: u64, actor_b: &Address) -> bool {
        (ts_a, actor_a) > (ts_b, actor_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(b: u8) -> Address {
        Address::from_hash_bytes([b; 16])
    }

    #[test]
    fn newer_write_wins() {
        let mut r = Lww::new(1u8, 5, a(1));
        r.set(2u8, 6, a(1));
        assert_eq!(*r.value(), 2);
        r.set(3u8, 4, a(1)); // older, ignored
        assert_eq!(*r.value(), 2);
    }

    #[test]
    fn merge_is_deterministic_on_ties() {
        // Same ts, different actors — the larger actor must win on both replicas.
        let mut x = Lww::new(10u8, 7, a(1));
        let y = Lww::new(20u8, 7, a(2));
        let mut y2 = y.clone();
        x.merge(&y);
        y2.merge(&Lww::new(10u8, 7, a(1)));
        assert_eq!(x.value(), y2.value());
        assert_eq!(*x.value(), 20); // actor a(2) > a(1)
    }
}
