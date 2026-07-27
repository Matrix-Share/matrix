//! **Black-hole attribution** — turning live delivery evidence into the credit /
//! penalty signals that drive [`crate::reputation`] (FR-47, network-layer
//! "black-hole relays").
//!
//! The scoring primitive already exists; what was missing is *what feeds it*.
//! End-to-end delivery receipts are sealed to the **original sender**, so only
//! the node that originated a bundle can read one — which makes the source the
//! natural, and only sound, place to attribute. A source observes two things
//! about a bundle it sent:
//!
//! * **custody was taken** — a downstream peer signed a [`CustodyReceipt`],
//!   committing to carry it. We record that peer as a *custodian* of the bundle.
//! * **delivery happened** — a verified [`DeliveryReceipt`] came back. Every
//!   custodian that had committed to that bundle demonstrably contributed →
//!   **credit** them.
//!
//! If instead a bundle a peer took custody of **expires with no delivery**, that
//! is a *miss*. A black hole racks up misses (it swallows everything and delivers
//! nothing); an honest carrier that just lacked contact opportunities racks up a
//! few. So misses are penalized **conservatively** — a grace count before any
//! penalty, small weight, and the router only ever *routes around* a demoted peer
//! when an alternative exists — keeping the ≥95%-delivery acceptance target safe.
//!
//! This layer is passive: it never changes what the source stores or forwards,
//! only the reputation scores, which are a soft routing hint.
//!
//! [`CustodyReceipt`]: lifeline_proto::CustodyReceipt
//! [`DeliveryReceipt`]: lifeline_proto::DeliveryReceipt

use lifeline_proto::{Address, Bytes};
use std::collections::HashMap;

/// Reward weight applied to each custodian of a bundle we saw delivered.
pub const CREDIT_W: f32 = 0.30;
/// Penalty weight applied once a custodian is past its grace misses.
pub const PENALIZE_W: f32 = 0.40;
/// Misses a custodian gets for free before any penalty — absorbs the normal DTN
/// case where delivery legitimately didn't happen (partition, no contact).
pub const GRACE_MISSES: u32 = 2;
/// Cap on outstanding (unconfirmed) bundles tracked, so a high send rate — or an
/// attacker who solicits many custody receipts — can't grow this unbounded. When
/// full, the soonest-to-expire entry is dropped.
pub const MAX_PENDING: usize = 4096;

/// Minimum custody observations (delivered + missed) for a custodian before the
/// **grey-hole** delivery-ratio rule can fire — enough signal to distinguish a
/// selective dropper from an unlucky honest carrier.
pub const GREY_MIN_SAMPLES: u32 = 8;
/// A custodian whose lifetime delivery ratio falls below this (with at least
/// [`GREY_MIN_SAMPLES`] observations) is penalized as a grey hole — even if it
/// never strings together [`GRACE_MISSES`] *consecutive* misses.
pub const GREY_MIN_DELIVERY_RATIO: f32 = 0.5;

/// One bundle we originated that at least one peer took custody of, awaiting
/// delivery confirmation.
#[derive(Debug, Clone)]
struct Pending {
    custodians: Vec<Address>,
    /// Wall-clock second after which, if still unconfirmed, this counts as a miss.
    deadline: u64,
}

/// Lifetime custody outcomes for one custodian, used by the grey-hole rule.
/// Unlike `misses` (a *consecutive* black-hole counter that a single delivery
/// clears), these totals persist across deliveries, so a peer that reliably
/// drops *some* fraction of what it carries cannot launder its record by
/// occasionally delivering.
#[derive(Debug, Clone, Copy, Default)]
struct CustodianStats {
    delivered: u32,
    missed: u32,
}

impl CustodianStats {
    fn samples(&self) -> u32 {
        self.delivered.saturating_add(self.missed)
    }
    /// True once we have enough samples and the delivery ratio is below the
    /// grey-hole floor.
    fn is_grey_hole(&self) -> bool {
        let n = self.samples();
        if n < GREY_MIN_SAMPLES {
            return false;
        }
        (self.delivered as f32 / n as f32) < GREY_MIN_DELIVERY_RATIO
    }
}

/// Source-side ledger of custody → delivery outcomes, per bundle and per peer.
#[derive(Debug, Clone, Default)]
pub struct ForwardLedger {
    pending: HashMap<Bytes, Pending>,
    /// Accumulated *consecutive* misses per custodian (cleared by a delivery) —
    /// the black-hole signal.
    misses: HashMap<Address, u32>,
    /// Lifetime delivered/missed totals per custodian — the grey-hole signal.
    stats: HashMap<Address, CustodianStats>,
}

impl ForwardLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `custodian` took custody of our bundle `bundle_id`, to be
    /// confirmed (or missed) by `deadline`. Idempotent per (bundle, custodian).
    pub fn record_custody(&mut self, bundle_id: &Bytes, custodian: &Address, deadline: u64) {
        if let Some(p) = self.pending.get_mut(bundle_id) {
            if !p.custodians.contains(custodian) {
                p.custodians.push(custodian.clone());
            }
            // A later custody extends the observation window.
            p.deadline = p.deadline.max(deadline);
            return;
        }
        if self.pending.len() >= MAX_PENDING {
            self.evict_soonest();
        }
        self.pending.insert(
            bundle_id.clone(),
            Pending {
                custodians: vec![custodian.clone()],
                deadline,
            },
        );
    }

    /// The bundle was delivered (verified receipt). Returns the custodians to
    /// **credit** (each contributed) and clears any accrued misses for them, so a
    /// peer that recovers isn't dragged down by stale strikes.
    pub fn confirm_delivery(&mut self, bundle_id: &Bytes) -> Vec<Address> {
        let Some(p) = self.pending.remove(bundle_id) else {
            return Vec::new();
        };
        for c in &p.custodians {
            // Clear the *consecutive* black-hole strikes...
            self.misses.remove(c);
            // ...but persist the lifetime tally, so a grey hole can't launder a
            // poor delivery ratio by delivering here and there.
            self.stats.entry(c.clone()).or_default().delivered += 1;
        }
        p.custodians
    }

    /// Expire everything past `now`. Returns the custodians to **penalize** —
    /// those that are either a **black hole** (more than [`GRACE_MISSES`]
    /// *consecutive* misses) or a **grey hole** (a lifetime delivery ratio below
    /// [`GREY_MIN_DELIVERY_RATIO`] over at least [`GREY_MIN_SAMPLES`]
    /// observations, so a selective dropper that occasionally delivers to reset
    /// its consecutive strikes is still caught). Honest carriers that just lacked
    /// contact opportunities clear both bars. A peer may appear once per bundle it
    /// dropped.
    pub fn expire(&mut self, now: u64) -> Vec<Address> {
        let expired: Vec<Bytes> = self
            .pending
            .iter()
            .filter(|(_, p)| p.deadline <= now)
            .map(|(id, _)| id.clone())
            .collect();
        let mut penalize = Vec::new();
        for id in expired {
            let Some(p) = self.pending.remove(&id) else {
                continue;
            };
            for c in p.custodians {
                let n = self.misses.entry(c.clone()).or_insert(0);
                *n += 1;
                let black_hole = *n > GRACE_MISSES;
                let st = self.stats.entry(c.clone()).or_default();
                st.missed += 1;
                if black_hole || st.is_grey_hole() {
                    penalize.push(c);
                }
            }
        }
        penalize
    }

    /// Outstanding tracked bundles (diagnostics/tests).
    #[cfg(test)]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Drop the entry that will expire soonest — the least useful to keep when at
    /// capacity, since it's closest to resolving one way or the other anyway.
    fn evict_soonest(&mut self) {
        if let Some(id) = self
            .pending
            .iter()
            .min_by_key(|(_, p)| p.deadline)
            .map(|(id, _)| id.clone())
        {
            self.pending.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(b: u8) -> Address {
        Address::from_hash_bytes([b; 16])
    }
    fn bid(b: u8) -> Bytes {
        Bytes::new(vec![b; 8])
    }

    #[test]
    fn delivery_confirms_and_returns_custodians_to_credit() {
        let mut l = ForwardLedger::new();
        l.record_custody(&bid(1), &addr(9), 100);
        l.record_custody(&bid(1), &addr(8), 100); // a second custodian of the same bundle
        let credit = l.confirm_delivery(&bid(1));
        assert_eq!(credit.len(), 2);
        assert!(credit.contains(&addr(9)) && credit.contains(&addr(8)));
        assert_eq!(l.pending_len(), 0);
    }

    #[test]
    fn misses_within_grace_do_not_penalize() {
        let mut l = ForwardLedger::new();
        // The first GRACE_MISSES misses are free.
        for i in 0..GRACE_MISSES {
            l.record_custody(&bid(i as u8), &addr(5), 10);
            let pen = l.expire(20);
            assert!(pen.is_empty(), "miss {i} is within grace, no penalty yet");
        }
    }

    #[test]
    fn a_black_hole_is_penalized_after_grace() {
        let mut l = ForwardLedger::new();
        let mut penalized = false;
        // GRACE_MISSES free, then penalties.
        for i in 0..(GRACE_MISSES + 2) {
            l.record_custody(&bid(i as u8), &addr(7), 10);
            if l.expire(20).contains(&addr(7)) {
                penalized = true;
            }
        }
        assert!(
            penalized,
            "a peer that swallows every bundle must be penalized"
        );
    }

    #[test]
    fn delivery_clears_prior_misses() {
        let mut l = ForwardLedger::new();
        // Two misses (within grace) then a delivery → strikes reset.
        l.record_custody(&bid(1), &addr(3), 10);
        l.expire(20);
        l.record_custody(&bid(2), &addr(3), 10);
        l.expire(20);
        l.record_custody(&bid(3), &addr(3), 10);
        l.confirm_delivery(&bid(3)); // clears misses for addr(3)
                                     // Now it takes a fresh GRACE_MISSES+1 to penalize again.
        let mut penalized = false;
        for i in 10..(10 + GRACE_MISSES + 2) {
            l.record_custody(&bid(i as u8), &addr(3), 10);
            if l.expire(20).contains(&addr(3)) {
                penalized = true;
            }
        }
        assert!(penalized);
    }

    #[test]
    fn a_grey_hole_is_caught_despite_resetting_consecutive_misses() {
        // addr(6) delivers only 1 of every 3 bundles (a ~33% carrier) — a clear
        // grey hole. The delivery on every third round resets the *consecutive*
        // miss counter (which never exceeds GRACE_MISSES = 2), so the black-hole
        // rule alone would never fire; the lifetime ratio rule must catch it.
        let mut l = ForwardLedger::new();
        let mut penalized = false;
        for i in 0..30u8 {
            l.record_custody(&bid(i), &addr(6), 10);
            if i % 3 == 0 {
                l.confirm_delivery(&bid(i)); // delivered (clears consecutive strikes)
            } else if l.expire(20).contains(&addr(6)) {
                penalized = true; // dropped
            }
        }
        assert!(
            penalized,
            "a selective (grey-hole) dropper must be penalized by the ratio rule"
        );
    }

    #[test]
    fn a_mostly_reliable_carrier_is_not_grey_holed() {
        // addr(4) delivers ~90% and drops ~10% — an honest, lossy carrier. Its
        // ratio stays above the floor, so it is never penalized.
        let mut l = ForwardLedger::new();
        let mut penalized = false;
        for i in 0..40u8 {
            l.record_custody(&bid(i), &addr(4), 10);
            if i % 10 == 9 {
                if l.expire(20).contains(&addr(4)) {
                    penalized = true;
                }
            } else {
                l.confirm_delivery(&bid(i));
            }
        }
        assert!(!penalized, "a mostly-reliable carrier must not be demoted");
    }

    #[test]
    fn pending_is_bounded() {
        let mut l = ForwardLedger::new();
        for i in 0..(MAX_PENDING + 50) {
            let id = Bytes::new((i as u64).to_le_bytes().to_vec());
            l.record_custody(&id, &addr(1), (i as u64) + 1000);
        }
        assert!(l.pending_len() <= MAX_PENDING);
    }
}
