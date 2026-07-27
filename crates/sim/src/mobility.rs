//! Mobility models that drive opportunistic contacts for the evaluation harness.
//!
//! The default [`World`](crate::World) contact model is cluster-based (nodes in
//! the same cluster are in radio range). For a comparative evaluation on the
//! terms the DTN literature uses, this module adds two standard drivers, both
//! expressed as "which node pairs are in contact this tick":
//!
//! * [`RandomWaypoint`] — the canonical synthetic model (Broch et al., 1998;
//!   Johnson & Maltz): nodes roam a square toward random waypoints and are in
//!   contact when within radio range.
//! * [`TraceMobility`] — replays an explicit, timestamped contact list, so a real
//!   contact trace (e.g. the CRAWDAD `cambridge/haggle` Bluetooth traces or MIT
//!   Reality Mining) can be converted to `(tick, a, b)` events and fed in
//!   verbatim. We ship the *ingestion* path (and a small synthetic trace for
//!   tests); the copyrighted trace data itself is not vendored.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::HashMap;

/// Yields the set of in-contact, unordered node pairs `(a, b)` with `a < b` for a
/// given tick. The harness drives an exchange for each pair exactly as it does
/// for co-located cluster members.
pub trait Mobility {
    fn contacts(&mut self, tick: u64, node_count: usize) -> Vec<(usize, usize)>;
}

/// Random Waypoint on a wrapped (torus) square. Each node heads toward a random
/// waypoint at a fixed speed; on arrival it draws a new one. Two nodes are in
/// contact when their (toroidal) distance is within `range`. Deterministic for a
/// given seed.
pub struct RandomWaypoint {
    pos: Vec<(f64, f64)>,
    dst: Vec<(f64, f64)>,
    size: f64,
    speed: f64,
    range: f64,
    rng: StdRng,
    inited: bool,
}

impl RandomWaypoint {
    /// `size` = side length of the square area; `speed` = distance per tick;
    /// `range` = radio contact radius.
    pub fn new(seed: u64, size: f64, speed: f64, range: f64) -> Self {
        RandomWaypoint {
            pos: Vec::new(),
            dst: Vec::new(),
            size: size.max(1.0),
            speed: speed.max(0.0),
            range: range.max(0.0),
            rng: StdRng::seed_from_u64(seed),
            inited: false,
        }
    }

    fn rand_point(&mut self) -> (f64, f64) {
        (
            self.rng.gen::<f64>() * self.size,
            self.rng.gen::<f64>() * self.size,
        )
    }

    fn init(&mut self, n: usize) {
        self.pos = (0..n).map(|_| self.rand_point()).collect();
        self.dst = (0..n).map(|_| self.rand_point()).collect();
        self.inited = true;
    }

    /// Advance every node toward its waypoint by up to `speed`; on arrival pick a
    /// new waypoint.
    fn advance(&mut self) {
        for i in 0..self.pos.len() {
            let (px, py) = self.pos[i];
            let (dx, dy) = self.dst[i];
            let (vx, vy) = (dx - px, dy - py);
            let dist = (vx * vx + vy * vy).sqrt();
            if dist <= self.speed || dist == 0.0 {
                self.pos[i] = self.dst[i];
                self.dst[i] = self.rand_point();
            } else {
                self.pos[i] = (px + vx / dist * self.speed, py + vy / dist * self.speed);
            }
        }
    }

    /// Toroidal distance on the square (wraps at the edges).
    fn toroidal_dist(&self, a: (f64, f64), b: (f64, f64)) -> f64 {
        let dx = (a.0 - b.0).abs();
        let dy = (a.1 - b.1).abs();
        let dx = dx.min(self.size - dx);
        let dy = dy.min(self.size - dy);
        (dx * dx + dy * dy).sqrt()
    }
}

impl Mobility for RandomWaypoint {
    fn contacts(&mut self, _tick: u64, node_count: usize) -> Vec<(usize, usize)> {
        if !self.inited || self.pos.len() != node_count {
            self.init(node_count);
        }
        self.advance();
        let mut pairs = Vec::new();
        for a in 0..node_count {
            for b in (a + 1)..node_count {
                if self.toroidal_dist(self.pos[a], self.pos[b]) <= self.range {
                    pairs.push((a, b));
                }
            }
        }
        pairs
    }
}

/// Replays a fixed, timestamped contact list. Convert a real trace's contact
/// events to `(tick, a, b)` and hand them here.
pub struct TraceMobility {
    by_tick: HashMap<u64, Vec<(usize, usize)>>,
}

impl TraceMobility {
    /// Build from `(tick, a, b)` events. Pairs are normalized to `a < b` and
    /// deduplicated per tick.
    pub fn from_events(events: impl IntoIterator<Item = (u64, usize, usize)>) -> Self {
        let mut by_tick: HashMap<u64, Vec<(usize, usize)>> = HashMap::new();
        for (t, a, b) in events {
            if a == b {
                continue;
            }
            let pair = if a < b { (a, b) } else { (b, a) };
            let slot = by_tick.entry(t).or_default();
            if !slot.contains(&pair) {
                slot.push(pair);
            }
        }
        TraceMobility { by_tick }
    }
}

impl Mobility for TraceMobility {
    fn contacts(&mut self, tick: u64, _node_count: usize) -> Vec<(usize, usize)> {
        self.by_tick.get(&tick).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_waypoint_eventually_produces_contacts() {
        // A modest area with a decent range must yield some contacts over time.
        let mut m = RandomWaypoint::new(1, 50.0, 5.0, 15.0);
        let mut any = false;
        for t in 0..200 {
            if !m.contacts(t, 12).is_empty() {
                any = true;
                break;
            }
        }
        assert!(any, "random waypoint should produce contacts within range");
    }

    #[test]
    fn random_waypoint_is_deterministic_for_a_seed() {
        let mut a = RandomWaypoint::new(7, 40.0, 4.0, 12.0);
        let mut b = RandomWaypoint::new(7, 40.0, 4.0, 12.0);
        for t in 0..50 {
            assert_eq!(a.contacts(t, 10), b.contacts(t, 10));
        }
    }

    #[test]
    fn trace_mobility_replays_events_at_their_tick() {
        let mut m = TraceMobility::from_events([(5, 0, 1), (5, 1, 0), (7, 2, 3)]);
        assert_eq!(m.contacts(5, 4), vec![(0, 1)]); // deduped despite (0,1)+(1,0)
        assert!(m.contacts(6, 4).is_empty());
        assert_eq!(m.contacts(7, 4), vec![(2, 3)]);
    }
}
