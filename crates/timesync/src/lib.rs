//! **Differential time synchronisation** — the DGPS pattern applied to clocks.
//!
//! GPS gives a device precise *time* for free, but only with a view of the sky —
//! useless indoors, underground, in an urban canyon (the disaster case). So do
//! for time what Differential GPS does for position:
//!
//! - A node with GPS time is a **reference** (stratum 1). It broadcasts its time.
//! - A node *without* sky view **disciplines its cheap oscillator** to the
//!   reference (becoming stratum 2), and can itself broadcast — a reference for
//!   nodes even further from ground truth (stratum 3, …).
//! - Quality degrades with **stratum** (hop-distance from GPS) exactly as DGPS
//!   accuracy degrades with baseline. A few sky-view nodes become a **timing
//!   backbone** for a whole GPS-denied cluster.
//!
//! Why it matters: a shared clock unlocks TDMA slotting on the contended bearers
//! (ultrasound/LoRa/BLE-adv) → collision-free scheduling and less energy;
//! coordinated **duty-cycling / rendezvous** so battery-scarce nodes agree on
//! wake windows; and loose-sync **broadcast authentication** (TESLA/μTESLA).
//!
//! The estimate is robust to variable mesh latency: offset samples are held in a
//! small window and combined with the **median**, so one slow beacon can't skew
//! the clock. This crate is pure logic in whatever integer time unit the caller
//! uses (milliseconds recommended); the engine wires `advertise()` into a beacon
//! and feeds received beacons to [`TimeSync::observe`].
//!
//! # ⚠️ Security — read before wiring this into anything that gates message life
//!
//! Time gates message security: bundle **expiry (TTL)** and **replay/dedup**
//! windows depend on the clock. A hostile beacon that a victim disciplines to
//! could otherwise fast-forward its clock to *prematurely expire its whole store*
//! (including queued SOS) or rewind it to *un-expire replays*. Two defences:
//!
//! 1. **The caller MUST authenticate beacons.** `observe` must only be fed
//!    beacons whose sender is verified and trusted (a paired contact / known
//!    time-backbone key). An unauthenticated broadcast must never move a clock
//!    that gates message lifetime. (`stratum` is self-asserted — do not trust it
//!    for admission, only for tie-breaking among *already-authenticated*
//!    references.)
//! 2. **This crate bounds the blast radius**: a single accepted beacon can move
//!    the clock by at most [`max_step`](TimeSync::with_max_step) (bounded slew),
//!    and all arithmetic is saturating, so one bad reference cannot jump the clock
//!    arbitrarily even if it slips past (1).
//!
//! Until the caller does (1), keep gating TTL/replay on the raw local clock, not
//! `corrected()`.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Stratum value meaning "no time fix" (mirrors NTP's unreachable stratum 16→∞).
pub const NO_FIX: u8 = u8::MAX;
/// Default cap on how far from ground truth we still trust a reference.
pub const DEFAULT_MAX_STRATUM: u8 = 12;
/// Default bound on how far one accepted beacon may move the clock (in the
/// caller's time unit; assumes ms → 60 s). Bounds a hostile-beacon jump.
pub const DEFAULT_MAX_STEP: i64 = 60_000;
/// Offset-sample window used for the robust (median) estimate.
const WINDOW: usize = 8;

/// A time beacon a node broadcasts (and neighbours [`observe`](TimeSync::observe)):
/// the sender's current best estimate of network time and its stratum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeBeacon {
    /// The sender's corrected network time (same unit the mesh agrees on).
    pub time: i64,
    /// The sender's distance from ground truth (1 = has GPS).
    pub stratum: u8,
}

/// A node's differential clock: its offset from network time and how far it sits
/// from a GPS reference.
#[derive(Debug, Clone)]
pub struct TimeSync {
    stratum: u8,
    /// Correction added to local time to get network time.
    offset: i64,
    /// Recent offset samples from the current reference tier (robust estimate).
    samples: VecDeque<i64>,
    /// Local time of the last accepted reference (for staleness).
    last_update: i64,
    max_stratum: u8,
    /// Max clock movement per accepted beacon (bounded slew) — the security clamp.
    max_step: i64,
    fixed: bool,
}

impl Default for TimeSync {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeSync {
    pub fn new() -> Self {
        Self::with_max_stratum(DEFAULT_MAX_STRATUM)
    }

    pub fn with_max_stratum(max_stratum: u8) -> Self {
        TimeSync {
            stratum: NO_FIX,
            offset: 0,
            samples: VecDeque::new(),
            last_update: 0,
            max_stratum,
            max_step: DEFAULT_MAX_STEP,
            fixed: false,
        }
    }

    /// Set the per-beacon clock-movement bound (the security clamp). A smaller
    /// value limits how fast a (hopefully-authenticated) reference can move the
    /// clock, at the cost of slower convergence to a large genuine correction.
    pub fn with_max_step(mut self, max_step: i64) -> Self {
        self.max_step = max_step.max(0);
        self
    }

    /// Declare that this node has **authoritative time** (a GPS fix): it becomes a
    /// stratum-1 reference with an exact offset. Ground-truth nodes are never
    /// disciplined by others.
    pub fn set_reference(&mut self, gps_time: i64, local_now: i64) {
        self.stratum = 1;
        self.offset = gps_time.saturating_sub(local_now);
        self.samples.clear();
        self.last_update = local_now;
        self.fixed = true;
    }

    /// Incorporate a neighbour's [`TimeBeacon`] heard at `local_now`. Prefers
    /// references closer to ground truth; blends same-tier references; and, if our
    /// own fix has gone stale, will accept a worse one rather than drift.
    pub fn observe(&mut self, beacon: TimeBeacon, local_now: i64, stale_after: i64) {
        // We hold GPS — never disciplined by anyone downstream.
        if self.stratum == 1 && self.fixed {
            return;
        }
        // Reject references that are themselves unfixed or too far from truth.
        if beacon.stratum == NO_FIX || beacon.stratum >= self.max_stratum {
            return;
        }
        let candidate = beacon.stratum.saturating_add(1);
        let stale = self.is_stale(local_now, stale_after);
        // Ignore a strictly-worse reference unless we've gone stale.
        if self.fixed && candidate > self.stratum && !stale {
            return;
        }
        let sample = beacon.time.saturating_sub(local_now);
        if candidate != self.stratum {
            // Switched reference tier — start a fresh estimate window.
            self.samples.clear();
            self.stratum = candidate;
        }
        self.samples.push_back(sample);
        while self.samples.len() > WINDOW {
            self.samples.pop_front();
        }
        let target = median(&self.samples);
        if self.fixed {
            // Bounded slew (security clamp): one accepted beacon can move the clock
            // by at most `max_step`, so a hostile reference that slips past the
            // caller's authentication can't fast-forward us to expire the store or
            // rewind us to un-expire replays. A large *genuine* correction still
            // converges, just over several beacons.
            let delta = target
                .saturating_sub(self.offset)
                .clamp(-self.max_step, self.max_step);
            self.offset = self.offset.saturating_add(delta);
        } else {
            self.offset = target; // first fix: adopt directly
        }
        self.last_update = local_now;
        self.fixed = true;
    }

    /// Local time corrected to network time.
    pub fn corrected(&self, local_now: i64) -> i64 {
        local_now.saturating_add(self.offset)
    }

    /// Our current offset from network time (0 until we have a fix).
    pub fn offset(&self) -> i64 {
        self.offset
    }

    /// Our distance from ground truth ([`NO_FIX`] until disciplined).
    pub fn stratum(&self) -> u8 {
        self.stratum
    }

    /// Do we have a usable time fix?
    pub fn has_fix(&self) -> bool {
        self.fixed && self.stratum != NO_FIX
    }

    /// Has it been longer than `stale_after` since our last reference?
    pub fn is_stale(&self, local_now: i64, stale_after: i64) -> bool {
        !self.fixed || local_now - self.last_update > stale_after
    }

    /// What to broadcast so neighbours can discipline to us: our corrected time
    /// and stratum — `None` if we have no (fresh) fix, so we don't pollute the
    /// mesh with a guess.
    pub fn advertise(&self, local_now: i64, stale_after: i64) -> Option<TimeBeacon> {
        if !self.has_fix() || self.is_stale(local_now, stale_after) {
            return None;
        }
        Some(TimeBeacon {
            time: self.corrected(local_now),
            stratum: self.stratum,
        })
    }
}

/// Median of an offset window (robust to a single high-latency sample).
fn median(samples: &VecDeque<i64>) -> i64 {
    if samples.is_empty() {
        return 0;
    }
    let mut v: Vec<i64> = samples.iter().copied().collect();
    v.sort_unstable();
    v[v.len() / 2]
}

#[cfg(test)]
mod tests {
    use super::*;

    const STALE: i64 = 300_000; // 5 min

    #[test]
    fn a_gps_node_is_a_stratum_1_reference() {
        let mut t = TimeSync::new();
        assert!(!t.has_fix());
        // Local clock reads 1000 but GPS says it's really 5000.
        t.set_reference(5000, 1000);
        assert_eq!(t.stratum(), 1);
        assert_eq!(t.corrected(1000), 5000);
        assert_eq!(t.corrected(2000), 6000); // offset persists
    }

    #[test]
    fn a_client_disciplines_to_a_reference() {
        let mut client = TimeSync::new();
        // Reference says network time is 10_000 while our local clock reads 3_000.
        client.observe(
            TimeBeacon {
                time: 10_000,
                stratum: 1,
            },
            3_000,
            STALE,
        );
        assert_eq!(client.stratum(), 2, "one hop from GPS");
        assert_eq!(client.corrected(3_000), 10_000);
    }

    #[test]
    fn median_rejects_a_high_latency_outlier() {
        let mut c = TimeSync::new();
        // Several good beacons (offset ≈ 500) …
        for i in 0..5 {
            let local = 1_000 + i * 10;
            c.observe(
                TimeBeacon {
                    time: local + 500,
                    stratum: 1,
                },
                local,
                STALE,
            );
        }
        // … then one badly-delayed beacon claiming a wildly different time.
        c.observe(
            TimeBeacon {
                time: 1_000_000,
                stratum: 1,
            },
            1_060,
            STALE,
        );
        // The median holds the offset near the honest 500, not the outlier.
        assert!(
            (c.offset() - 500).abs() < 50,
            "offset {} skewed",
            c.offset()
        );
    }

    #[test]
    fn a_closer_reference_wins() {
        let mut c = TimeSync::new();
        // First disciplined to a distant reference (stratum 4 → we're 5).
        c.observe(
            TimeBeacon {
                time: 8_000,
                stratum: 4,
            },
            8_000,
            STALE,
        );
        assert_eq!(c.stratum(), 5);
        // A stratum-1 reference appears → we switch closer to ground truth.
        c.observe(
            TimeBeacon {
                time: 9_000,
                stratum: 1,
            },
            9_000,
            STALE,
        );
        assert_eq!(c.stratum(), 2);
        assert_eq!(c.corrected(9_000), 9_000);
        // A worse reference is now ignored.
        c.observe(
            TimeBeacon {
                time: 999_999,
                stratum: 6,
            },
            9_100,
            STALE,
        );
        assert_eq!(c.stratum(), 2);
    }

    #[test]
    fn corrections_propagate_hop_by_hop() {
        // GPS node A (stratum 1); B disciplines to A; C disciplines to B. C is two
        // hops from ground truth but still lands on network time — the whole point.
        let mut a = TimeSync::new();
        a.set_reference(50_000, 0); // A: local 0 ↔ network 50_000

        let mut b = TimeSync::new();
        // B hears A's beacon. B's local clock happens to read 1_234.
        let a_beacon = a.advertise(0, STALE).unwrap();
        b.observe(a_beacon, 1_234, STALE);
        assert_eq!(b.stratum(), 2);

        let mut c = TimeSync::new();
        // Later, B advertises (B local 2_000) and C hears it (C local 7_777).
        // B's beacon carries the network time at that instant: 50_000 + (2_000 −
        // 1_234) = 50_766 — B's cheap clock now reads correct network time.
        let b_beacon = b.advertise(2_000, STALE).unwrap();
        assert_eq!(b_beacon.time, 50_766);
        c.observe(b_beacon, 7_777, STALE);
        assert_eq!(c.stratum(), 3);
        // C, two hops from GPS, adopts the network time from the beacon it heard
        // (one-way sync, so up to the tiny B→C propagation delay — zero here).
        assert_eq!(c.corrected(7_777), b_beacon.time);
    }

    #[test]
    fn a_hostile_beacon_cannot_jump_the_clock_more_than_max_step() {
        // Client is synced (offset ≈ 500). An attacker beacon claims the time is
        // 30 days ahead, trying to fast-forward the victim to expire its store.
        let mut c = TimeSync::new().with_max_step(60_000); // 60 s cap
        c.observe(
            TimeBeacon {
                time: 1_500,
                stratum: 1,
            },
            1_000,
            STALE,
        ); // first fix, offset 500
        let before = c.offset();
        let attack = 30 * 24 * 3_600 * 1_000i64; // +30 days
        c.observe(
            TimeBeacon {
                time: 1_100 + attack,
                stratum: 1,
            },
            1_100,
            STALE,
        );
        // The offset moved by at most one max_step, not 30 days.
        assert!(
            (c.offset() - before).abs() <= 60_000,
            "one beacon moved the clock by {} (must be ≤ max_step)",
            c.offset() - before
        );
    }

    #[test]
    fn extreme_beacon_times_do_not_panic() {
        let mut c = TimeSync::new();
        c.observe(
            TimeBeacon {
                time: i64::MAX,
                stratum: 1,
            },
            0,
            STALE,
        );
        let _ = c.corrected(i64::MAX); // saturating, no overflow panic
        c.observe(
            TimeBeacon {
                time: i64::MIN,
                stratum: 1,
            },
            0,
            STALE,
        );
        let _ = c.corrected(i64::MIN);
    }

    #[test]
    fn a_stale_fix_is_reported_and_advertise_stops() {
        let mut c = TimeSync::new();
        c.observe(
            TimeBeacon {
                time: 10_000,
                stratum: 1,
            },
            1_000,
            STALE,
        );
        assert!(!c.is_stale(1_000 + STALE, STALE));
        assert!(c.is_stale(1_000 + STALE + 1, STALE));
        assert!(c.advertise(1_000 + STALE + 1, STALE).is_none());
    }
}
