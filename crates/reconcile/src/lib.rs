//! **Range-based set reconciliation** — the differential-transfer primitive.
//!
//! Two nodes hold sets of keys (for Lifeline: the `bundle_id`s each carries).
//! Naively syncing means re-offering everything and de-duplicating by trial. This
//! instead converges to the exact **symmetric difference** `A △ B` with
//! communication **proportional to the difference**, not to the set size — the
//! set-valued form of "transmit only the residual against what the peer already
//! knows" (see [`docs/differential-transfer.md`]).
//!
//! **Why this algorithm (validity/inversion).** Set reconciliation is textbook
//! (Minsky–Trachtenberg 2003; Eppstein et al. 2011; Meyer 2023) and deployed in
//! Dynamo/Cassandra, Bitcoin, and Nostr (NIP-77 negentropy). We deliberately use
//! the **range-based** family rather than IBLT: IBLT is *probabilistic* and, if
//! mis-sized, can **silently omit** elements — unacceptable when a missed element
//! is a dropped SOS. Range-based is **deterministic**: a mismatching range is
//! always split further, bottoming out at explicit id lists, so nothing is
//! silently missed. Its only failure is a fingerprint collision, made negligible
//! by a **BLAKE3** fingerprint over the sorted ids. It also **degrades to O(n)**
//! when sets are disjoint — never worse than re-offering everything.
//!
//! Reconciliation only moves ids + fingerprints; transferring the bundle *bodies*
//! for the discovered [`Reconciler::wants`] is a separate, difference-sized step.
//! Content stays end-to-end-authenticated, so a lying peer can only make its own
//! sync incomplete — it adds no attack surface on integrity.
//!
//! [`docs/differential-transfer.md`]: https://github.com/nometria/project-lifeline/blob/main/docs/differential-transfer.md

use serde::{Deserialize, Serialize};

/// Below this many items in a mismatching range, send the ids directly instead of
/// splitting further.
const LIST_THRESHOLD: usize = 16;
/// Fan-out when splitting a mismatching range.
const BUCKETS: usize = 16;

/// What a peer asserts about one contiguous range `[lo, hi)` of the key order
/// (`None` bound = unbounded).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Payload<K> {
    /// "My items in this range fingerprint to this." The receiver compares.
    Fingerprint([u8; 32]),
    /// "Here are my exact ids in this range — reply with yours." (Leaf; expects a
    /// [`Payload::IdListFinal`] back.)
    IdList(Vec<K>),
    /// "Here are my exact ids in this range; this range is resolved." (Leaf; no
    /// reply.)
    IdListFinal(Vec<K>),
}

/// One range of a reconciliation message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range<K> {
    /// Inclusive lower bound (`None` = −∞).
    pub lo: Option<K>,
    /// Exclusive upper bound (`None` = +∞).
    pub hi: Option<K>,
    pub payload: Payload<K>,
}

/// A reconciliation message: a set of ranges partitioning (part of) the key order.
pub type Message<K> = Vec<Range<K>>;

/// Ids in `theirs` that are absent from the sorted `mine` (what we're missing).
fn missing_from<K: Ord + Clone>(mine: &[K], theirs: &[K]) -> Vec<K> {
    theirs
        .iter()
        .filter(|id| mine.binary_search(id).is_err())
        .cloned()
        .collect()
}

/// Fingerprint of a sorted id slice: BLAKE3 over each id's BLAKE3 hash (fixed
/// width, so concatenation is unambiguous; order-canonical since the slice is
/// sorted; collision-resistant against malicious inputs).
pub fn fingerprint<K: AsRef<[u8]>>(sorted: &[K]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    for it in sorted {
        h.update(blake3::hash(it.as_ref()).as_bytes());
    }
    *h.finalize().as_bytes()
}

/// One side of a reconciliation. Feed it the peer's messages; it accumulates the
/// ids **this** node is missing in [`wants`](Self::wants).
pub struct Reconciler<K> {
    items: Vec<K>,
    wants: Vec<K>,
}

impl<K: Ord + Clone + AsRef<[u8]>> Reconciler<K> {
    /// A reconciler over this node's keys (deduplicated and sorted internally).
    pub fn new(mut items: Vec<K>) -> Self {
        items.sort();
        items.dedup();
        Reconciler {
            items,
            wants: Vec::new(),
        }
    }

    /// The opening message: a single fingerprint over the whole key space. The
    /// *initiator* sends this; the responder [`reconcile`](Self::reconcile)s it.
    pub fn initiate(&self) -> Message<K> {
        vec![Range {
            lo: None,
            hi: None,
            payload: Payload::Fingerprint(fingerprint(&self.items)),
        }]
    }

    /// Process the peer's `incoming` message, returning our response. Recording
    /// any ids we learn we're missing. Reconciliation is complete once both sides
    /// produce an **empty** response.
    pub fn reconcile(&mut self, incoming: &Message<K>) -> Message<K> {
        let mut out = Message::new();
        for r in incoming {
            match &r.payload {
                Payload::Fingerprint(their_fp) => {
                    let slice = self.slice(r.lo.as_ref(), r.hi.as_ref());
                    if &fingerprint(slice) == their_fp {
                        continue; // this range is already identical
                    }
                    if slice.len() <= LIST_THRESHOLD {
                        // Small range → list our ids so the peer can diff.
                        out.push(Range {
                            lo: r.lo.clone(),
                            hi: r.hi.clone(),
                            payload: Payload::IdList(slice.to_vec()),
                        });
                    } else {
                        // Large range → split and let the peer narrow the mismatch.
                        out.extend(self.split(slice, r.lo.as_ref(), r.hi.as_ref()));
                    }
                }
                Payload::IdList(their_ids) => {
                    // Scope the immutable slice borrow before touching `wants`.
                    let (missing, mine) = {
                        let slice = self.slice(r.lo.as_ref(), r.hi.as_ref());
                        (missing_from(slice, their_ids), slice.to_vec())
                    };
                    self.wants.extend(missing);
                    // Reply with our ids so the peer learns *its* misses; resolved.
                    out.push(Range {
                        lo: r.lo.clone(),
                        hi: r.hi.clone(),
                        payload: Payload::IdListFinal(mine),
                    });
                }
                Payload::IdListFinal(their_ids) => {
                    let missing = {
                        let slice = self.slice(r.lo.as_ref(), r.hi.as_ref());
                        missing_from(slice, their_ids)
                    };
                    self.wants.extend(missing); // resolved — no reply
                }
            }
        }
        out
    }

    /// The ids this node discovered it is missing (deduplicated). Fetch their
    /// bodies from the peer after reconciliation.
    pub fn wants(&self) -> &[K] {
        &self.wants
    }

    /// Consume the reconciler, yielding the missing ids.
    pub fn into_wants(mut self) -> Vec<K> {
        self.wants.sort();
        self.wants.dedup();
        self.wants
    }

    // --- internals ---

    /// Our items in `[lo, hi)`.
    fn slice(&self, lo: Option<&K>, hi: Option<&K>) -> &[K] {
        let start = match lo {
            Some(lo) => self.items.partition_point(|x| x < lo),
            None => 0,
        };
        let end = match hi {
            Some(hi) => self.items.partition_point(|x| x < hi),
            None => self.items.len(),
        };
        &self.items[start..end]
    }

    /// Split a mismatching range into up to [`BUCKETS`] sub-ranges, each carrying
    /// a fingerprint, so the peer can localise the difference.
    fn split(&self, slice: &[K], lo: Option<&K>, hi: Option<&K>) -> Message<K> {
        let n = slice.len();
        let step = n.div_ceil(BUCKETS).max(1);
        let mut out = Message::new();
        let mut i = 0;
        while i < n {
            let j = (i + step).min(n);
            out.push(Range {
                // The first sub-range keeps the outer lower bound; the last keeps
                // the outer upper bound; interior bounds are the item at the split.
                lo: if i == 0 {
                    lo.cloned()
                } else {
                    Some(slice[i].clone())
                },
                hi: if j == n {
                    hi.cloned()
                } else {
                    Some(slice[j].clone())
                },
                payload: Payload::Fingerprint(fingerprint(&slice[i..j])),
            });
            i = j;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type K = Vec<u8>;

    fn key(n: u32) -> K {
        n.to_be_bytes().to_vec()
    }

    /// Run a full reconciliation between two id sets; return (a_wants, b_wants,
    /// rounds, ids_transferred).
    fn run(a: Vec<K>, b: Vec<K>) -> (Vec<K>, Vec<K>, usize, usize) {
        let mut ra = Reconciler::new(a);
        let mut rb = Reconciler::new(b);
        let mut msg = ra.initiate(); // A initiates; B responds first.
        let mut to_b = true;
        let mut rounds = 0;
        let mut ids_moved = 0usize;
        for _ in 0..64 {
            // Count ids carried in this message (the "difference-sized" traffic).
            for r in &msg {
                if let Payload::IdList(v) | Payload::IdListFinal(v) = &r.payload {
                    ids_moved += v.len();
                }
            }
            msg = if to_b {
                rb.reconcile(&msg)
            } else {
                ra.reconcile(&msg)
            };
            to_b = !to_b;
            rounds += 1;
            if msg.is_empty() {
                break;
            }
        }
        assert!(msg.is_empty(), "reconciliation must converge");
        (ra.into_wants(), rb.into_wants(), rounds, ids_moved)
    }

    fn set_diff(a: &[K], b: &[K]) -> Vec<K> {
        let mut d: Vec<K> = a.iter().filter(|x| !b.contains(x)).cloned().collect();
        d.sort();
        d
    }

    #[test]
    fn converges_to_the_exact_symmetric_difference() {
        // A = {0..80}, B = {40..120}: overlap 40..80.
        let a: Vec<K> = (0..80).map(key).collect();
        let b: Vec<K> = (40..120).map(key).collect();
        let (a_wants, b_wants, _rounds, _ids) = run(a.clone(), b.clone());
        assert_eq!(
            a_wants,
            set_diff(&b, &a),
            "A learns exactly what B has and it lacks"
        );
        assert_eq!(
            b_wants,
            set_diff(&a, &b),
            "B learns exactly what A has and it lacks"
        );
    }

    #[test]
    fn identical_sets_need_no_id_transfer() {
        let a: Vec<K> = (0..200).map(key).collect();
        let (a_wants, b_wants, rounds, ids) = run(a.clone(), a.clone());
        assert!(a_wants.is_empty() && b_wants.is_empty());
        assert_eq!(
            ids, 0,
            "no ids move when the sets match — only one fingerprint"
        );
        assert_eq!(rounds, 1, "a single fingerprint round proves equality");
    }

    #[test]
    fn cost_is_proportional_to_the_difference_not_the_set() {
        // Two large, nearly-identical sets differing by only a handful of items.
        let a: Vec<K> = (0..1000).map(key).collect();
        let mut b = a.clone();
        b.retain(|k| k != &key(123) && k != &key(777)); // B misses 2
        b.push(key(5000)); // B has 1 extra
        let (a_wants, b_wants, _rounds, ids) = run(a.clone(), b.clone());
        assert_eq!(a_wants, set_diff(&b, &a)); // {5000}
        assert_eq!(b_wants, set_diff(&a, &b)); // {123, 777}
                                               // Difference is 3 items; traffic is a small multiple of that, nowhere
                                               // near the 1000-item set size.
        assert!(ids < 100, "moved {ids} ids for a 3-item difference");
    }

    #[test]
    fn disjoint_sets_degrade_gracefully_and_stay_correct() {
        let a: Vec<K> = (0..30).map(key).collect();
        let b: Vec<K> = (100..130).map(key).collect();
        let (a_wants, b_wants, _r, _ids) = run(a.clone(), b.clone());
        // Each side must learn it needs the whole of the other set.
        assert_eq!(a_wants, {
            let mut x = b.clone();
            x.sort();
            x
        });
        assert_eq!(b_wants, {
            let mut x = a.clone();
            x.sort();
            x
        });
    }

    #[test]
    fn one_empty_set() {
        let a: Vec<K> = (0..50).map(key).collect();
        let (a_wants, b_wants, _r, _i) = run(a.clone(), Vec::new());
        assert!(a_wants.is_empty(), "A (full) needs nothing");
        assert_eq!(b_wants.len(), 50, "B (empty) needs all of A");
    }
}
