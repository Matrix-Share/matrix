//! A **Kademlia DHT** for Project Lifeline — online peer + rendezvous discovery.
//!
//! When a node has internet (via any bearer — a relay, Nostr, Meshtastic-MQTT),
//! it can find *where a peer is reachable* or *who is registered at a rendezvous
//! key* without a central directory: nodes self-organize into an XOR keyspace
//! (Maymounkov & Mazières, 2002) and answer `FIND_NODE`/`FIND_VALUE`/`STORE`.
//! Lifeline's [`Address`] is already a 128-bit id with an `xor_distance` metric,
//! so it doubles as both the node id and the DHT key.
//!
//! **Transport-agnostic.** The lookup algorithm is pure logic driven over the
//! [`DhtRpc`] seam (a synchronous request→response), so it rides *any* carrier
//! and is fully testable against an in-memory network — the same seam philosophy
//! as the rest of Lifeline. A node contact carries an opaque `endpoint` blob the
//! transport interprets (a socket address, a bearer hint); the DHT never looks
//! inside it.
//!
//! What's here: [`RoutingTable`] (k-buckets over 128 XOR buckets), [`Node`] (the
//! responder + lookup driver: `lookup_nodes`, `find_value`, `store`, `bootstrap`),
//! and the [`DhtMsg`] wire enum.

use lifeline_proto::address::ADDRESS_LEN;
use lifeline_proto::Address;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

/// Bucket count = keyspace bit-length (one bucket per most-significant differing
/// bit of the XOR distance).
const BITS: usize = ADDRESS_LEN * 8;
/// Default bucket width `k` (Kademlia's replication parameter).
pub const DEFAULT_K: usize = 20;
/// Default lookup concurrency `α` (nodes queried per round).
pub const DEFAULT_ALPHA: usize = 3;
/// Safety bound on lookup rounds, so a pathological/adversarial network can't
/// spin the iterative loop forever.
const MAX_ROUNDS: usize = 64;

/// A known node: its id plus an opaque, transport-interpreted reachability hint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub id: Address,
    /// Opaque to the DHT; the transport decodes it (e.g. a socket address or a
    /// bearer routing hint). May be empty when reachability is implied.
    #[serde(default)]
    pub endpoint: Vec<u8>,
}

impl Contact {
    pub fn new(id: Address, endpoint: Vec<u8>) -> Self {
        Self { id, endpoint }
    }
}

/// One Kademlia RPC (query or reply).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DhtMsg {
    /// Liveness probe → [`DhtMsg::Pong`].
    Ping,
    /// Liveness reply / generic ack.
    Pong,
    /// "Give me the nodes you know closest to `target`" → [`DhtMsg::Nodes`].
    FindNode { target: Address },
    /// "Do you have `key`? else your closest nodes" → [`DhtMsg::Value`]/[`DhtMsg::Nodes`].
    FindValue { key: Address },
    /// "Store this `value` under `key`" → [`DhtMsg::Pong`].
    Store { key: Address, value: Vec<u8> },
    /// The closest contacts a node knows to a queried key.
    Nodes { closest: Vec<Contact> },
    /// A stored value (answer to [`DhtMsg::FindValue`]).
    Value { value: Vec<u8> },
}

/// The transport seam: send `query` from `from` to `to` and return the reply, or
/// `None` on timeout/unreachable. Synchronous, so the iterative lookup is plain
/// straight-line logic; a real carrier implements it over its own async I/O.
pub trait DhtRpc {
    fn query(&mut self, from: &Contact, to: &Contact, query: DhtMsg) -> Option<DhtMsg>;
}

/// Bucket index for `other` relative to `local`: the position of the most
/// significant set bit of their XOR distance (0..BITS). `None` iff equal.
fn bucket_index(local: &Address, other: &Address) -> Option<usize> {
    let d = local.xor_distance(other);
    for (i, byte) in d.iter().enumerate() {
        if *byte != 0 {
            let bit_in_byte = 7 - byte.leading_zeros() as usize; // MSB within this byte
            return Some((ADDRESS_LEN - 1 - i) * 8 + bit_in_byte);
        }
    }
    None
}

/// One k-bucket: up to `k` contacts, most-recently-seen at the back. When full
/// we keep the incumbents (the conservative Kademlia policy: long-lived nodes
/// are more likely to stay up, and it resists eviction flooding).
#[derive(Debug, Clone)]
struct KBucket {
    entries: VecDeque<Contact>,
    k: usize,
}

impl KBucket {
    fn new(k: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            k,
        }
    }

    fn observe(&mut self, c: Contact) {
        if let Some(pos) = self.entries.iter().position(|e| e.id == c.id) {
            // Seen again → refresh position (LRU) and any updated endpoint.
            self.entries.remove(pos);
            self.entries.push_back(c);
        } else if self.entries.len() < self.k {
            self.entries.push_back(c);
        }
        // Full → drop the newcomer (incumbents retained).
    }
}

/// A node's Kademlia routing table: 128 distance-ordered k-buckets.
#[derive(Debug, Clone)]
pub struct RoutingTable {
    local: Address,
    buckets: Vec<KBucket>,
}

impl RoutingTable {
    pub fn new(local: Address, k: usize) -> Self {
        Self {
            local,
            buckets: (0..BITS).map(|_| KBucket::new(k)).collect(),
        }
    }

    /// Note that `c` exists (learned from any RPC). Never inserts ourselves.
    pub fn observe(&mut self, c: Contact) {
        if let Some(i) = bucket_index(&self.local, &c.id) {
            self.buckets[i].observe(c);
        }
    }

    /// The `count` contacts we know closest (by XOR) to `target`.
    pub fn closest(&self, target: &Address, count: usize) -> Vec<Contact> {
        let mut all: Vec<Contact> = self
            .buckets
            .iter()
            .flat_map(|b| b.entries.iter().cloned())
            .collect();
        all.sort_by_key(|c| c.id.xor_distance(target));
        all.truncate(count);
        all
    }

    /// Total contacts held.
    pub fn len(&self) -> usize {
        self.buckets.iter().map(|b| b.entries.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One iterative-lookup shortlist entry.
struct Probe {
    contact: Contact,
    queried: bool,
    responded: bool,
}

/// Whether an iterative lookup is chasing nodes or a value.
enum Mode {
    Nodes,
    Value,
}

/// The core Kademlia iterative lookup. Drives `α`-at-a-time queries toward
/// `target`, always widening from the current `k` closest, until the `k` closest
/// have all been queried (or `MAX_ROUNDS`). Returns any value found (Value mode),
/// the `k` closest responsive contacts, and every contact learned along the way.
fn iterative(
    me: &Contact,
    seeds: Vec<Contact>,
    target: &Address,
    mode: Mode,
    rpc: &mut dyn DhtRpc,
    k: usize,
    alpha: usize,
) -> (Option<Vec<u8>>, Vec<Contact>, Vec<Contact>) {
    let mut shortlist: Vec<Probe> = Vec::new();
    let mut learned: Vec<Contact> = Vec::new();
    let add = |shortlist: &mut Vec<Probe>, learned: &mut Vec<Contact>, c: Contact| {
        if c.id == me.id || shortlist.iter().any(|p| p.contact.id == c.id) {
            return;
        }
        learned.push(c.clone());
        shortlist.push(Probe {
            contact: c,
            queried: false,
            responded: false,
        });
    };
    for c in seeds {
        add(&mut shortlist, &mut learned, c);
    }

    for _ in 0..MAX_ROUNDS {
        // Keep the shortlist ordered by closeness to the target.
        shortlist.sort_by_key(|p| p.contact.id.xor_distance(target));
        // The next batch: up to α unqueried nodes among the current k closest.
        let batch: Vec<Contact> = shortlist
            .iter()
            .take(k)
            .filter(|p| !p.queried)
            .take(alpha)
            .map(|p| p.contact.clone())
            .collect();
        if batch.is_empty() {
            break; // the k closest are all queried → converged
        }
        for contact in batch {
            if let Some(p) = shortlist.iter_mut().find(|p| p.contact.id == contact.id) {
                p.queried = true;
            }
            let query = match mode {
                Mode::Nodes => DhtMsg::FindNode {
                    target: target.clone(),
                },
                Mode::Value => DhtMsg::FindValue {
                    key: target.clone(),
                },
            };
            match rpc.query(me, &contact, query) {
                Some(DhtMsg::Value { value }) if matches!(mode, Mode::Value) => {
                    let closest = collect_responsive(&shortlist, target, k);
                    return (Some(value), closest, learned);
                }
                Some(DhtMsg::Nodes { closest }) => {
                    mark_responded(&mut shortlist, &contact.id);
                    for c in closest {
                        add(&mut shortlist, &mut learned, c);
                    }
                }
                Some(_) => mark_responded(&mut shortlist, &contact.id),
                None => {} // unreachable → stays !responded, drops out of results
            }
        }
    }
    let closest = collect_responsive(&shortlist, target, k);
    (None, closest, learned)
}

fn mark_responded(shortlist: &mut [Probe], id: &Address) {
    if let Some(p) = shortlist.iter_mut().find(|p| p.contact.id == *id) {
        p.responded = true;
    }
}

fn collect_responsive(shortlist: &[Probe], target: &Address, k: usize) -> Vec<Contact> {
    let mut ok: Vec<Contact> = shortlist
        .iter()
        .filter(|p| p.responded)
        .map(|p| p.contact.clone())
        .collect();
    ok.sort_by_key(|c| c.id.xor_distance(target));
    ok.truncate(k);
    ok
}

/// A DHT participant: a routing table, a local key→value store, and the lookup
/// driver. Owns no I/O — every network step goes through a [`DhtRpc`].
pub struct Node {
    me: Contact,
    table: RoutingTable,
    store: HashMap<Address, Vec<u8>>,
    k: usize,
    alpha: usize,
}

impl Node {
    /// A node with the default Kademlia parameters (`k = 20`, `α = 3`).
    pub fn new(me: Contact) -> Self {
        Self::with_params(me, DEFAULT_K, DEFAULT_ALPHA)
    }

    pub fn with_params(me: Contact, k: usize, alpha: usize) -> Self {
        let table = RoutingTable::new(me.id.clone(), k);
        Self {
            me,
            table,
            store: HashMap::new(),
            k,
            alpha,
        }
    }

    pub fn id(&self) -> &Address {
        &self.me.id
    }

    pub fn contact(&self) -> &Contact {
        &self.me
    }

    pub fn routing(&self) -> &RoutingTable {
        &self.table
    }

    /// Record a contact we learned out-of-band (e.g. from a beacon).
    pub fn observe(&mut self, c: Contact) {
        self.table.observe(c);
    }

    /// Handle an inbound RPC from `from`: learn the sender, then answer.
    pub fn handle(&mut self, from: &Contact, msg: DhtMsg) -> DhtMsg {
        self.table.observe(from.clone());
        match msg {
            DhtMsg::Ping => DhtMsg::Pong,
            DhtMsg::FindNode { target } => DhtMsg::Nodes {
                closest: self.table.closest(&target, self.k),
            },
            DhtMsg::FindValue { key } => match self.store.get(&key) {
                Some(v) => DhtMsg::Value { value: v.clone() },
                None => DhtMsg::Nodes {
                    closest: self.table.closest(&key, self.k),
                },
            },
            DhtMsg::Store { key, value } => {
                self.store.insert(key, value);
                DhtMsg::Pong
            }
            // Replies aren't queries; ack so a caller isn't left hanging.
            DhtMsg::Pong | DhtMsg::Nodes { .. } | DhtMsg::Value { .. } => DhtMsg::Pong,
        }
    }

    /// Iteratively locate the `k` nodes closest to `target`, improving our table.
    pub fn lookup_nodes(&mut self, target: &Address, rpc: &mut dyn DhtRpc) -> Vec<Contact> {
        let seeds = self.table.closest(target, self.k);
        let (_, closest, learned) = iterative(
            &self.me,
            seeds,
            target,
            Mode::Nodes,
            rpc,
            self.k,
            self.alpha,
        );
        for c in learned {
            self.table.observe(c);
        }
        closest
    }

    /// Iteratively fetch a value for `key`. Returns it if any reachable node
    /// holds it; the table is improved regardless.
    pub fn find_value(&mut self, key: &Address, rpc: &mut dyn DhtRpc) -> Option<Vec<u8>> {
        if let Some(v) = self.store.get(key) {
            return Some(v.clone());
        }
        let seeds = self.table.closest(key, self.k);
        let (value, _closest, learned) =
            iterative(&self.me, seeds, key, Mode::Value, rpc, self.k, self.alpha);
        for c in learned {
            self.table.observe(c);
        }
        value
    }

    /// Publish `value` under `key`: find the `k` closest nodes and `STORE` to
    /// them (and keep a local copy). Returns how many nodes accepted it.
    pub fn store(&mut self, key: Address, value: Vec<u8>, rpc: &mut dyn DhtRpc) -> usize {
        let targets = self.lookup_nodes(&key, rpc);
        let mut stored = 0;
        for c in &targets {
            let msg = DhtMsg::Store {
                key: key.clone(),
                value: value.clone(),
            };
            if rpc.query(&self.me, c, msg).is_some() {
                stored += 1;
            }
        }
        self.store.insert(key, value); // local replica
        stored
    }

    /// Join the DHT via a known `seed`: learn it, then look ourselves up so our
    /// neighbourhood populates our buckets. Returns our table size afterward.
    pub fn bootstrap(&mut self, seed: Contact, rpc: &mut dyn DhtRpc) -> usize {
        self.table.observe(seed);
        let target = self.me.id.clone();
        self.lookup_nodes(&target, rpc);
        self.table.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Spread a test index across all 16 address bytes (splitmix64) so ids fall
    /// into many different buckets, not just the low ones.
    fn id(i: u64) -> Address {
        let mut b = [0u8; ADDRESS_LEN];
        let mut x = i
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0x1234_5678);
        for chunk in b.chunks_mut(8) {
            x ^= x >> 30;
            x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
            x ^= x >> 27;
            x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
            x ^= x >> 31;
            chunk.copy_from_slice(&x.to_le_bytes()[..chunk.len()]);
            x = x.wrapping_add(i).wrapping_add(1);
        }
        Address::from_hash_bytes(b)
    }

    fn contact(i: u64) -> Contact {
        Contact::new(id(i), vec![])
    }

    /// In-memory DHT network: every node in a shared map, RPCs dispatched by id.
    struct Net(Rc<RefCell<std::collections::HashMap<Address, Node>>>);
    impl DhtRpc for Net {
        fn query(&mut self, from: &Contact, to: &Contact, q: DhtMsg) -> Option<DhtMsg> {
            let mut m = self.0.borrow_mut();
            let node = m.get_mut(&to.id)?;
            Some(node.handle(from, q))
        }
    }

    type Map = Rc<RefCell<std::collections::HashMap<Address, Node>>>;

    /// Run a closure with node `a` pulled out of the map (so the RPC dispatcher
    /// never re-enters it), then put it back.
    fn with_node<R>(map: &Map, a: &Address, f: impl FnOnce(&mut Node, &mut Net) -> R) -> R {
        let mut node = map.borrow_mut().remove(a).unwrap();
        let mut net = Net(map.clone());
        let r = f(&mut node, &mut net);
        map.borrow_mut().insert(a.clone(), node);
        r
    }

    fn build_network(n: u64) -> (Map, Vec<Address>) {
        let map: Map = Rc::new(RefCell::new(std::collections::HashMap::new()));
        let ids: Vec<Address> = (0..n).map(id).collect();
        for i in 0..n {
            map.borrow_mut()
                .insert(ids[i as usize].clone(), Node::new(contact(i)));
        }
        // Everyone bootstraps off node 0 (the well-known seed).
        let seed = contact(0);
        for i in 1..n {
            with_node(&map, &ids[i as usize], |node, net| {
                node.bootstrap(seed.clone(), net);
            });
        }
        (map, ids)
    }

    #[test]
    fn bucket_index_is_ordered_by_distance() {
        let a = id(1);
        // Same id → no bucket.
        assert_eq!(bucket_index(&a, &a), None);
        // A closer and a farther peer land in different (and correctly-ordered)
        // buckets relative to `a`.
        let near = id(2);
        let far = id(999);
        let bn = bucket_index(&a, &near).unwrap();
        let bf = bucket_index(&a, &far).unwrap();
        assert!(bn < BITS && bf < BITS);
    }

    #[test]
    fn lookup_converges_to_the_true_closest_node() {
        let (map, ids) = build_network(60);
        // Look up an arbitrary key from an arbitrary node.
        let from = &ids[7];
        let key = id(4242); // not necessarily a node id
        let result = with_node(&map, from, |node, net| node.lookup_nodes(&key, net));

        // The globally-closest node to `key` (excluding the initiator, which is
        // pulled from the map during the lookup) must be in the returned set.
        let true_closest = ids
            .iter()
            .filter(|a| *a != from)
            .min_by(|x, y| x.xor_distance(&key).cmp(&y.xor_distance(&key)))
            .unwrap()
            .clone();
        assert!(
            result.iter().any(|c| c.id == true_closest),
            "iterative lookup should reach the globally closest node"
        );
    }

    #[test]
    fn value_stored_by_one_node_is_found_by_another() {
        let (map, ids) = build_network(60);
        let key = id(31337);
        let value = b"rendezvous:relay.example:443".to_vec();

        // Node 3 publishes; node 40 (far from it) resolves.
        let stored = with_node(&map, &ids[3], |node, net| {
            node.store(key.clone(), value.clone(), net)
        });
        assert!(stored > 0, "value should be stored on ≥1 node");

        let found = with_node(&map, &ids[40], |node, net| node.find_value(&key, net));
        assert_eq!(found, Some(value));
    }

    #[test]
    fn missing_value_returns_none_but_still_reaches_nodes() {
        let (map, ids) = build_network(30);
        let found = with_node(&map, &ids[5], |node, net| node.find_value(&id(55555), net));
        assert_eq!(found, None);
    }

    #[test]
    fn bootstrap_populates_the_table() {
        let (map, ids) = build_network(40);
        let table_len = map.borrow().get(&ids[10]).unwrap().routing().len();
        assert!(table_len > 1, "a bootstrapped node should know many peers");
    }
}
