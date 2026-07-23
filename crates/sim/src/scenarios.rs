//! Reproducible scenarios that map to the PRD's acceptance criteria (§16).
//!
//! Each builder returns a configured [`World`] ready to `run(ticks)`. Keeping
//! them here lets both the demo binary and the integration tests exercise the
//! exact same topologies.

use crate::{Mule, World};
use lifeline_proto::{Coords, Payload, PayloadKind, Priority};

/// Phones per cluster in the partition scenarios.
const PER_CLUSTER: usize = 4;

/// **UC2 / UC3 / NFR-3** — a 3-cluster partition with **no** infrastructure,
/// bridged only by a single moving data mule. This is the headline acceptance
/// criterion: ≥95% of messages must eventually deliver (PRD FR-29 AC).
///
/// Clusters have no radio overlap; the only way a bundle crosses from cluster A
/// to cluster C is to be physically carried by the mule (store-carry-forward).
pub fn three_cluster_mule(seed: u64) -> World {
    let mut w = World::new(seed);
    let clusters = 3usize;

    // Phones: cluster c owns indices [c*PER_CLUSTER .. +PER_CLUSTER).
    for c in 0..clusters {
        for _ in 0..PER_CLUSTER {
            w.add_node(c, false, vec![]);
        }
    }
    // One mule looping 0 → 1 → 2, dwelling a few ticks in each.
    let mule = w.add_node(0, false, vec![]);
    w.add_mule(Mule {
        node: mule,
        route: vec![0, 1, 2],
        dwell: 3,
    });

    // Cross-cluster traffic: each phone messages its counterpart one and two
    // clusters over (so every message must traverse the mule).
    for c in 0..clusters {
        for k in 0..PER_CLUSTER {
            let from = c * PER_CLUSTER + k;
            let to1 = ((c + 1) % clusters) * PER_CLUSTER + k;
            let to2 = ((c + 2) % clusters) * PER_CLUSTER + k;
            w.send_text(from, to1, "are you safe?");
            w.send_text(from, to2, "meet at the relief camp");
        }
    }
    w
}

/// **UC4** — "one gateway lights the mesh". Two clusters, each an isolated local
/// mesh with a single internet-uplink gateway; there is no mule. Messages from
/// the disaster-zone cluster reach recipients in the other cluster purely
/// because each side can touch *a* gateway and the gateways bridge over the
/// internet fabric. Delivery returns a verifiable receipt.
pub fn one_gateway_lights_mesh(seed: u64) -> World {
    let mut w = World::new(seed);

    // Cluster 0: disaster zone — phones + one internet gateway.
    for _ in 0..PER_CLUSTER {
        w.add_node(0, false, vec![]);
    }
    let _gw0 = w.add_node(0, true, vec!["internet".into()]);

    // Cluster 1: the connected world — phones + one internet gateway.
    for _ in 0..PER_CLUSTER {
        w.add_node(1, false, vec![]);
    }
    let _gw1 = w.add_node(1, true, vec!["internet".into()]);

    // Every disaster-zone phone messages a counterpart in cluster 1.
    let cluster1_start = PER_CLUSTER + 1; // after cluster0 phones + gw0
    for k in 0..PER_CLUSTER {
        let from = k;
        let to = cluster1_start + k;
        w.send_text(from, to, "we are trapped, send help");
    }
    w
}

/// **UC5** — an SOS with GPS from an isolated cluster still escapes via the mule
/// and preempts normal traffic. Returns the SOS bundle id for inspection.
pub fn sos_over_mule(seed: u64) -> (World, lifeline_proto::Bytes) {
    let mut w = World::new(seed);
    // Two clusters, mule between them.
    for c in 0..2 {
        for _ in 0..PER_CLUSTER {
            w.add_node(c, false, vec![]);
        }
    }
    let mule = w.add_node(0, false, vec![]);
    w.add_mule(Mule {
        node: mule,
        route: vec![0, 1],
        dwell: 3,
    });

    // Background bulk traffic to compete with the SOS.
    for k in 0..PER_CLUSTER {
        w.send(k, PER_CLUSTER + k, text("status update"), Priority::Bulk);
    }
    // The SOS from an isolated phone with no saved contacts still targets a peer.
    let sos = Payload {
        kind: PayloadKind::Sos,
        body: Some("trapped 2nd floor".into()),
        coords: Some(Coords {
            lat: 12.9716,
            lon: 77.5946,
            acc_m: 6,
        }),
        battery_pct: Some(14),
        attach: None,
        group_id: None,
    };
    let id = w.send(0, PER_CLUSTER, sos, Priority::Sos);
    (w, id)
}

/// **NFR-6** — a single dense cluster: many nodes, all in range, all messaging.
/// Used to check the network stays stable (dedup + hop limits prevent broadcast
/// storms) rather than to test partitions.
pub fn dense_cluster(seed: u64, nodes: usize, messages: usize) -> World {
    let mut w = World::new(seed);
    for _ in 0..nodes {
        w.add_node(0, false, vec![]);
    }
    for m in 0..messages {
        let from = m % nodes;
        let to = (m * 7 + 3) % nodes;
        if from != to {
            w.send_text(from, to, "roll call");
        }
    }
    w
}

/// **FR-33 in a real mesh** — three partitioned clusters make divergent group
/// edits offline; a single mule ferries CRDT state between them. After enough
/// ferrying, every node converges to identical group membership.
///
/// Expected final membership of "relief" = {0,1,2,3} ∪ {4} ∪ {8,9} with 5
/// removed by its own adder (observed remove sticks).
pub fn group_partition_merge(seed: u64) -> World {
    let mut w = World::new(seed);
    let clusters = 3usize;
    for c in 0..clusters {
        for _ in 0..PER_CLUSTER {
            w.add_node(c, false, vec![]);
        }
    }
    let mule = w.add_node(0, false, vec![]);
    w.add_mule(Mule {
        node: mule,
        route: vec![0, 1, 2],
        dwell: 3,
    });

    // Divergent, concurrent edits in each partition (nodes can only edit what
    // they know; edits propagate via contact + mule).
    let g = "relief";
    w.group_add(0, g, 0);
    w.group_add(0, g, 1);
    w.group_add(0, g, 2);
    w.group_add(0, g, 3);

    w.group_add(4, g, 4); // cluster 1 adds itself
    w.group_add(4, g, 5);
    w.group_remove(4, g, 5); // …then removes 5 (observed remove)

    w.group_add(8, g, 8); // cluster 2 adds two members
    w.group_add(8, g, 9);
    w
}

/// **FR-46 AC** — a spammer floods unpaid BULK traffic while legitimate messages
/// (including an SOS) flow. With postage gating on, the flood is dropped at
/// relays and never delays the real traffic.
pub fn postage_throttles_spam(seed: u64) -> World {
    let mut w = World::new(seed);
    w.set_require_postage(true); // must precede add_node
    for c in 0..2 {
        for _ in 0..PER_CLUSTER {
            w.add_node(c, false, vec![]);
        }
    }
    let mule = w.add_node(0, false, vec![]);
    w.add_mule(Mule {
        node: mule,
        route: vec![0, 1],
        dwell: 3,
    });

    // Legitimate traffic (postage minted automatically): normal + one SOS.
    for k in 0..PER_CLUSTER {
        w.send_text(k, PER_CLUSTER + k, "checking in");
    }
    let sos = Payload {
        kind: PayloadKind::Sos,
        body: Some("collapsed building".into()),
        coords: Some(Coords {
            lat: 13.08,
            lon: 80.27,
            acc_m: 9,
        }),
        battery_pct: Some(22),
        attach: None,
        group_id: None,
    };
    w.send(0, PER_CLUSTER, sos, Priority::Sos);

    // A spammer in cluster 0 floods 200 unpaid bulk messages.
    w.inject_unpaid_bulk(1, PER_CLUSTER + 1, 200);
    w
}

fn text(body: &str) -> Payload {
    Payload {
        kind: PayloadKind::Text,
        body: Some(body.to_string()),
        coords: None,
        battery_pct: None,
        attach: None,
        group_id: None,
    }
}
