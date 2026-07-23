//! Acceptance tests mapped to PRD §16 exit criteria and §9 NFRs. These fail the
//! build if the router/crypto regress below the required delivery guarantees.

use lifeline_sim::scenarios;
use lifeline_sim::{Mule, World};

/// NFR-3 / FR-29 AC: "Under a simulated 3-cluster partition with one moving
/// mule, ≥ 95% of messages eventually deliver." We additionally require every
/// delivery to be cryptographically **verified** (FR-31..34).
#[test]
fn three_cluster_mule_meets_95pct() {
    let mut w = scenarios::three_cluster_mule(42);
    let r = w.run(700);
    assert!(
        r.pct_delivered() >= 95.0,
        "delivery {:.1}% < 95% ({} / {})",
        r.pct_delivered(),
        r.delivered,
        r.sent
    );
    assert!(
        r.pct_verified() >= 95.0,
        "verified {:.1}% < 95%",
        r.pct_verified()
    );
    assert!(
        w.all_logs_valid(),
        "hash-linked logs must all verify (FR-30)"
    );
}

/// Determinism: the same seed must produce the same outcome (reproducible AC).
#[test]
fn runs_are_deterministic() {
    let a = scenarios::three_cluster_mule(7).run(400);
    let b = scenarios::three_cluster_mule(7).run(400);
    assert_eq!(a.delivered, b.delivered);
    assert_eq!(a.verified, b.verified);
    assert_eq!(a.forwarded_copies, b.forwarded_copies);
}

/// UC4: one internet gateway per isolated cluster restores cross-cluster reach
/// with verifiable delivery.
#[test]
fn one_gateway_lights_the_mesh() {
    let mut w = scenarios::one_gateway_lights_mesh(42);
    let r = w.run(400);
    assert_eq!(r.delivered, r.sent, "all messages must bridge and deliver");
    assert_eq!(r.verified, r.sent, "all deliveries must be verified");
}

/// UC5: an SOS from an isolated cluster escapes via the mule despite competing
/// bulk traffic, and is delivered + verified.
#[test]
fn sos_escapes_and_is_verified() {
    let (mut w, _sos_id) = scenarios::sos_over_mule(42);
    let r = w.run(400);
    assert!(
        r.pct_delivered() >= 95.0,
        "SOS scenario delivery {:.1}%",
        r.pct_delivered()
    );
    assert_eq!(r.verified, r.sent);
}

/// FR-33: CRDT group membership converges to identical state across a
/// partitioned mesh after mule-ferried anti-entropy sync.
#[test]
fn group_membership_converges_across_partition() {
    let mut w = scenarios::group_partition_merge(42);
    w.run(700);
    assert!(
        w.all_agree_on_group("relief"),
        "all nodes must converge to identical membership"
    );
    // Expected: {0,1,2,3,4,8,9} — 5 was removed by its own adder.
    let members = w.group_members(0, "relief");
    assert_eq!(members.len(), 7, "unexpected membership {members:?}");
    assert!(
        !members.contains(&w.address(5)),
        "removed member 5 must be absent"
    );
    assert!(members.contains(&w.address(0)) && members.contains(&w.address(9)));
}

/// FR-46 AC: a low-priority flood is throttled without delaying legitimate or
/// SOS traffic.
#[test]
fn postage_throttles_flood_without_delaying_sos() {
    let mut w = scenarios::postage_throttles_spam(42);
    let r = w.run(400);
    // Every legitimate message (incl. the SOS) still delivers and verifies.
    assert_eq!(r.delivered, r.sent);
    assert_eq!(r.verified, r.sent);
    // The unpaid spam was dropped at relays rather than propagated.
    assert!(
        r.dropped_nopostage > 0,
        "spam should be dropped for missing postage"
    );
}

/// FR-28 / Problem C: erasure-coded messages survive a partitioned network with
/// **lossy** carriers (partial escape). 3 clusters bridged by one mule, 20% of
/// every handoff dropped — any 2 of 8 fragments still reconstruct the message.
#[test]
fn erasure_survives_lossy_partition() {
    let mut w = World::new(3);
    for c in 0..3 {
        for _ in 0..4 {
            w.add_node(c, false, vec![]);
        }
    }
    let mule = w.add_node(0, false, vec![]);
    w.add_mule(Mule {
        node: mule,
        route: vec![0, 1, 2],
        dwell: 3,
    });
    w.set_loss(0.20); // 20% of handoffs drop

    // Many erasure-coded messages from clusters 0 & 1 to cluster-2 phones.
    for round in 0..4 {
        for k in 0..4usize {
            let from = if round % 2 == 0 { k } else { 4 + k };
            let to = 8 + ((k + round) % 4);
            w.send_erasure(from, to, "evacuate to higher ground", 2, 6); // any 2 of 8
        }
    }
    w.run(1200);

    let pct = 100.0 * w.erasure_delivered_count() as f64 / w.erasure_sent_count() as f64;
    assert!(
        pct >= 90.0,
        "erasure reconstruction under 20% loss was {pct:.0}% ({}/{})",
        w.erasure_delivered_count(),
        w.erasure_sent_count()
    );
}

/// FR-47: a black-hole relay demotion, observed by one node, propagates to the
/// whole mesh via reputation gossip — after which every node avoids it.
#[test]
fn reputation_demotion_gossips_across_the_mesh() {
    // A single dense cluster so gossip can reach everyone; node 7 is the "relay".
    let mut w = lifeline_sim::scenarios::dense_cluster(42, 12, 0);
    let target = 7;
    assert_eq!(w.demoting_count(target), 0);

    // Node 0 detects the black hole and penalizes it hard.
    w.penalize_relay(0, target, 0.99);
    w.penalize_relay(0, target, 0.99);
    assert_eq!(
        w.demoting_count(target),
        1,
        "only the observer demotes it at first"
    );

    // Let the mesh gossip for a few contact rounds.
    w.run(10);
    assert_eq!(
        w.demoting_count(target),
        w.node_count(),
        "every node must learn the demotion via gossip"
    );
}

/// NFR-6: a dense cluster stays stable — everything delivers and spray copies
/// stay bounded (no broadcast storm). With a spray budget of 6 per message,
/// forwarded copies must be far below the naive all-pairs flood.
#[test]
fn dense_cluster_no_storm() {
    let nodes = 60;
    let messages = 120;
    let mut w = scenarios::dense_cluster(42, nodes, messages);
    let r = w.run(60);
    assert_eq!(r.delivered, r.sent, "all intra-cluster messages deliver");
    // A flood would be O(messages * nodes^2); bound well under that.
    let flood_ceiling = (messages as u64) * (nodes as u64);
    assert!(
        r.forwarded_copies < flood_ceiling,
        "forwarded {} exceeded storm ceiling {}",
        r.forwarded_copies,
        flood_ceiling
    );
}
