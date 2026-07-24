//! Bundle store with a size cap and priority/TTL-aware eviction (FR-52).

use lifeline_proto::{Address, Bundle, Bytes, Priority};
use std::collections::{HashMap, HashSet};

/// Default carried-bundle store cap (PRD Appendix B: 200 MB, LRU).
pub const DEFAULT_STORE_CAP_BYTES: u64 = 200 * 1024 * 1024;

/// A bundle held locally, plus per-bundle routing bookkeeping.
#[derive(Debug, Clone)]
pub struct Stored {
    pub bundle: Bundle,
    /// Peers (by address) we have already offered this bundle to.
    pub offered_to: HashSet<Address>,
    /// Whether we currently hold custody (accepted responsibility). Reserved
    /// for signed custody-receipt exchange (FR-25 follow-up); today nodes retain
    /// copies until TTL, so no bundle is silently dropped.
    #[allow(dead_code)]
    pub custody: bool,
    /// Approximate wire size in bytes (for the store cap).
    pub size: u64,
}

impl Stored {
    fn new(bundle: Bundle) -> Self {
        let size = approx_size(&bundle);
        Stored {
            bundle,
            offered_to: HashSet::new(),
            custody: true,
            size,
        }
    }
}

/// Approximate serialized size of a bundle for cap accounting.
fn approx_size(b: &Bundle) -> u64 {
    // Header overhead + the two big blobs. Good enough for eviction decisions.
    (b.ciphertext.len() + b.src_sealed.len() + b.bundle_id.len() + 64) as u64
}

/// Local bundle store keyed by `bundle_id`.
#[derive(Debug, Default)]
pub struct BundleStore {
    items: HashMap<Bytes, Stored>,
    total_bytes: u64,
    cap_bytes: u64,
}

impl BundleStore {
    pub fn new(cap_bytes: u64) -> Self {
        BundleStore {
            items: HashMap::new(),
            total_bytes: 0,
            cap_bytes,
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    pub fn contains(&self, id: &Bytes) -> bool {
        self.items.contains_key(id)
    }

    pub fn get_mut(&mut self, id: &Bytes) -> Option<&mut Stored> {
        self.items.get_mut(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &Bytes> {
        self.items.keys()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Stored> {
        self.items.values()
    }

    /// Insert (or replace) a stored bundle, evicting to stay under the cap.
    /// Returns `false` if the bundle could not be admitted (too large even
    /// after eviction, and lower priority than everything held).
    pub fn insert(&mut self, bundle: Bundle, now: u64) -> bool {
        let stored = Stored::new(bundle);
        let id = stored.bundle.bundle_id.clone();

        // Replacing an existing id: subtract its old size first.
        if let Some(old) = self.items.remove(&id) {
            self.total_bytes -= old.size;
        }

        self.evict_expired(now);
        if self.total_bytes + stored.size > self.cap_bytes {
            self.evict_for(&stored, now);
        }
        if self.total_bytes + stored.size > self.cap_bytes {
            // Still no room — refuse rather than silently over-fill.
            return false;
        }
        self.total_bytes += stored.size;
        self.items.insert(id, stored);
        true
    }

    /// Remove a bundle (e.g. after custody transfer completes or it is delivered).
    pub fn remove(&mut self, id: &Bytes) -> Option<Stored> {
        let s = self.items.remove(id);
        if let Some(ref s) = s {
            self.total_bytes -= s.size;
        }
        s
    }

    /// Drop all expired bundles; returns how many were removed.
    pub fn evict_expired(&mut self, now: u64) -> usize {
        let expired: Vec<Bytes> = self
            .items
            .values()
            .filter(|s| s.bundle.is_expired(now))
            .map(|s| s.bundle.bundle_id.clone())
            .collect();
        for id in &expired {
            self.remove(id);
        }
        expired.len()
    }

    /// Evict low-value bundles to make room for `incoming`. Eviction order:
    /// lowest priority first (Bulk before SOS), then oldest — but never evict a
    /// bundle of *higher* priority than the incoming one.
    fn evict_for(&mut self, incoming: &Stored, now: u64) {
        self.evict_expired(now);
        // Candidate victims sorted worst-to-keep first.
        let mut victims: Vec<(Bytes, Priority, u64)> = self
            .items
            .values()
            .map(|s| {
                (
                    s.bundle.bundle_id.clone(),
                    s.bundle.priority,
                    s.bundle.created_at,
                )
            })
            .collect();
        // Higher priority number = less important = evict first; then oldest.
        victims.sort_by(|a, b| b.1.as_u8().cmp(&a.1.as_u8()).then(a.2.cmp(&b.2)));
        for (id, prio, _) in victims {
            if self.total_bytes + incoming.size <= self.cap_bytes {
                break;
            }
            // Don't sacrifice a more important bundle for a less important one.
            if prio.as_u8() < incoming.bundle.priority.as_u8() {
                break;
            }
            // Never evict an SOS bundle to make room (not even for another SOS):
            // an SOS flood must not push already-held emergencies out of the
            // store. SOS only leaves via delivery or TTL expiry.
            if prio == Priority::Sos {
                continue;
            }
            self.remove(&id);
        }
    }
}
