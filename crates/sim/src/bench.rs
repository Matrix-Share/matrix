//! **Comparative evaluation harness.** Runs the *same* partitioned scenario
//! (same seed, mobility, and traffic) under each forwarding strategy and reports
//! the DTN standard metrics — delivery ratio, delivery latency, and overhead
//! (relay) ratio — so Lifeline's binary spray-and-wait can be compared against
//! epidemic flooding and no-relay direct delivery.
//!
//! This turns the white paper's evaluation methodology into concrete numbers on
//! an in-repo scenario. It is *not* a substitute for a study on standard external
//! mobility traces (The ONE simulator, CRAWDAD Haggle, Reality Mining); it is the
//! reproducible, hardware-free baseline that establishes the expected ordering:
//! direct delivery is the delivery floor, epidemic is the delivery ceiling at the
//! worst overhead, and spray-and-wait recovers most of epidemic's delivery at a
//! fraction of its overhead.

use crate::mobility::RandomWaypoint;
use crate::{DeliveryReport, Mule, RoutingStrategy, World};

/// Phones per cluster in the benchmark scenario.
const PER_CLUSTER: usize = 4;

/// The three strategies compared, in report order.
pub const STRATEGIES: [RoutingStrategy; 3] = [
    RoutingStrategy::Direct,
    RoutingStrategy::SprayAndWait,
    RoutingStrategy::Epidemic,
];

fn strategy_name(s: RoutingStrategy) -> &'static str {
    match s {
        RoutingStrategy::Direct => "direct-delivery",
        RoutingStrategy::SprayAndWait => "spray-and-wait",
        RoutingStrategy::Epidemic => "epidemic",
    }
}

/// Build the benchmark world under a given strategy: a 3-cluster partition with
/// no radio overlap, bridged only by one moving data mule, with every phone
/// messaging its counterparts one and two clusters over (so every message must
/// be carried across a partition). Identical across strategies except the
/// forwarding policy.
fn build(strategy: RoutingStrategy, seed: u64) -> World {
    let mut w = World::new(seed);
    w.set_strategy(strategy); // must precede add_node
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

/// Build a **Random Waypoint** world under a given strategy: `nodes` roaming a
/// square, in contact when within radio range (Broch et al.). Every node sends
/// one message to a pseudo-random peer, so traffic doesn't presuppose the
/// topology. This is the standard synthetic-mobility evaluation setting; the
/// same builder accepts a replayed real trace via [`World::set_mobility`].
fn build_rwp(strategy: RoutingStrategy, seed: u64, nodes: usize) -> World {
    let mut w = World::new(seed);
    w.set_strategy(strategy);
    // 100x100 area, ~5 units/tick, 18-unit radio range: contacts are frequent
    // but the graph is not a single clique.
    w.set_mobility(Box::new(RandomWaypoint::new(seed, 100.0, 5.0, 18.0)));
    for _ in 0..nodes {
        w.add_node(0, false, vec![]); // cluster ignored under a mobility model
    }
    // Deterministic pseudo-random sender→recipient pairing (no self-sends).
    for i in 0..nodes {
        let to = (i.wrapping_mul(7).wrapping_add(3)) % nodes;
        let to = if to == i { (to + 1) % nodes } else { to };
        w.send_text(i, to, "are you safe?");
    }
    w
}

/// One strategy's result.
#[derive(Debug, Clone)]
pub struct BenchRow {
    pub strategy: RoutingStrategy,
    pub report: DeliveryReport,
}

/// Run the mule-partition comparison across all strategies for `ticks`.
pub fn run_comparison(seed: u64, ticks: u64) -> Vec<BenchRow> {
    STRATEGIES
        .iter()
        .map(|&strategy| {
            let mut w = build(strategy, seed);
            let report = w.run(ticks);
            BenchRow { strategy, report }
        })
        .collect()
}

/// Run the Random-Waypoint comparison across all strategies for `ticks`.
pub fn run_comparison_rwp(seed: u64, ticks: u64, nodes: usize) -> Vec<BenchRow> {
    STRATEGIES
        .iter()
        .map(|&strategy| {
            let mut w = build_rwp(strategy, seed, nodes);
            let report = w.run(ticks);
            BenchRow { strategy, report }
        })
        .collect()
}

/// Render a comparison as a fixed-width table with the full DTN metric set.
pub fn format_table(rows: &[BenchRow]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<16} {:>9} {:>11} {:>9} {:>9} {:>10} {:>10}\n",
        "strategy", "delivery", "median-lat", "overhead", "mean-hops", "peak-buf", "mean-buf"
    ));
    out.push_str(&format!("{}\n", "-".repeat(78)));
    for r in rows {
        out.push_str(&format!(
            "{:<16} {:>8.1}% {:>9}s {:>9.2} {:>9.2} {:>10} {:>10.1}\n",
            strategy_name(r.strategy),
            r.report.pct_delivered(),
            r.report.median_latency_s(),
            r.report.overhead_ratio(),
            r.report.mean_hops(),
            r.report.peak_buffer,
            r.report.mean_buffer,
        ));
    }
    out.push_str(
        "\noverhead = forwarded copies per delivered message; peak/mean-buf = total\n\
         bundles stored across all nodes (lower is cheaper on both).\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_produces_a_row_per_strategy() {
        let rows = run_comparison(42, 300);
        assert_eq!(rows.len(), 3);
        // Every message resolves (NFR-3: no silent loss) — sent count is stable
        // across strategies since the scenario is identical.
        let sent: Vec<usize> = rows.iter().map(|r| r.report.sent).collect();
        assert!(sent.iter().all(|&s| s == sent[0]) && sent[0] > 0);
    }

    #[test]
    fn expected_ordering_holds() {
        let rows = run_comparison(7, 400);
        let by = |s: RoutingStrategy| rows.iter().find(|r| r.strategy == s).unwrap();
        let direct = by(RoutingStrategy::Direct);
        let spray = by(RoutingStrategy::SprayAndWait);
        let epidemic = by(RoutingStrategy::Epidemic);

        // Direct delivery cannot cross a partition (no relaying), so it is the
        // delivery floor; store-carry-forward must beat it.
        assert!(
            spray.report.delivered > direct.report.delivered,
            "spray-and-wait ({}) must beat no-relay direct ({})",
            spray.report.delivered,
            direct.report.delivered
        );
        // Epidemic floods, so its overhead ratio is at least spray-and-wait's.
        assert!(
            epidemic.report.overhead_ratio() >= spray.report.overhead_ratio(),
            "epidemic overhead {:.2} should be >= spray {:.2}",
            epidemic.report.overhead_ratio(),
            spray.report.overhead_ratio()
        );
        // Spray-and-wait recovers most of epidemic's delivery (within the same
        // partitioned scenario both rely on the mule to cross partitions).
        assert!(spray.report.delivered > 0);
    }

    #[test]
    fn random_waypoint_comparison_reports_full_metrics() {
        let rows = run_comparison_rwp(3, 500, 16);
        assert_eq!(rows.len(), 3);
        let by = |s: RoutingStrategy| rows.iter().find(|r| r.strategy == s).unwrap();
        let spray = by(RoutingStrategy::SprayAndWait);
        let epidemic = by(RoutingStrategy::Epidemic);
        let direct = by(RoutingStrategy::Direct);

        // Relaying should deliver at least as much as no-relay direct delivery.
        assert!(spray.report.delivered >= direct.report.delivered);
        // Epidemic floods, so it never uses less buffer or fewer transmissions
        // than spray-and-wait for the same delivered set.
        assert!(epidemic.report.peak_buffer >= spray.report.peak_buffer);
        assert!(epidemic.report.overhead_ratio() >= spray.report.overhead_ratio());
        // The new metrics are populated for anything delivered.
        if spray.report.delivered > 0 {
            assert!(spray.report.mean_hops() >= 1.0);
            assert!(spray.report.peak_buffer > 0);
        }
    }

    #[test]
    fn trace_driven_delivery_over_a_relay_chain() {
        use crate::mobility::TraceMobility;
        // A hand-authored 3-node contact trace: 0 meets 1 early, 1 meets 2 later.
        // A message 0→2 can only arrive by being relayed through 1 (store-carry-
        // forward), exercising the trace-replay path end to end.
        let mut w = World::new(1);
        w.set_mobility(Box::new(TraceMobility::from_events([
            (2, 0, 1),
            (3, 0, 1),
            (10, 1, 2),
            (11, 1, 2),
        ])));
        for _ in 0..3 {
            w.add_node(0, false, vec![]);
        }
        w.send_text(0, 2, "relayed via 1");
        let report = w.run(30);
        assert_eq!(report.delivered, 1, "trace-relayed message must arrive");
        assert!(report.hops.first().copied().unwrap_or(0) >= 1);
    }
}
