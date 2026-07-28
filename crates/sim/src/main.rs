//! Project Lifeline — simulator demo runner.
//!
//! Runs the PRD acceptance scenarios and prints a report. This is the "kill the
//! towers, whole room still messages out with proof" demo made runnable:
//!
//! ```text
//! cargo run -p lifeline-sim --release
//! ```

use lifeline_sim::scenarios;
use lifeline_sim::{bench, DeliveryReport, World};

fn banner(title: &str) {
    println!("\n\x1b[1m━━ {title} ━━\x1b[0m");
}

fn print_report(r: &DeliveryReport) {
    let ok = |p: f64| if p >= 95.0 { "\x1b[32m" } else { "\x1b[33m" };
    println!(
        "  sent {:>3} · delivered {}{:>3} ({:.1}%)\x1b[0m · verified {}{:>3} ({:.1}%)\x1b[0m",
        r.sent,
        ok(r.pct_delivered()),
        r.delivered,
        r.pct_delivered(),
        ok(r.pct_verified()),
        r.verified,
        r.pct_verified(),
    );
    println!(
        "  ticks {} · copies forwarded {} · dupes suppressed {} · expired {}",
        r.ticks, r.forwarded_copies, r.duplicates_suppressed, r.dropped_expired
    );
}

fn run(title: &str, mut w: World, ticks: u64) -> DeliveryReport {
    banner(title);
    let report = w.run(ticks);
    print_report(&report);
    assert!(
        w.all_logs_valid(),
        "every node's hash-linked log must verify"
    );
    println!("  hash-linked logs: \x1b[32mall valid\x1b[0m");
    report
}

fn main() {
    // `cargo run -p lifeline-sim -- bench` prints the comparative evaluation
    // table (spray-and-wait vs. epidemic vs. direct on the same partitioned
    // scenario) instead of the acceptance demo.
    // `cargo run -p lifeline-sim -- containment` runs the offline over-spend
    // containment measurement (single-agent vs transferable/chase-escape).
    if std::env::args().nth(1).as_deref() == Some("containment") {
        banner("Offline over-spend containment measurement");
        print!("{}", lifeline_sim::containment::report());
        return;
    }

    if std::env::args().nth(1).as_deref() == Some("bench") {
        banner("Comparative routing evaluation · 3-cluster partition + mule (seed 42, 700 ticks)");
        print!("{}", bench::format_table(&bench::run_comparison(42, 700)));
        banner("Comparative routing evaluation · Random Waypoint, 24 nodes (seed 42, 700 ticks)");
        print!(
            "{}",
            bench::format_table(&bench::run_comparison_rwp(42, 700, 24))
        );
        return;
    }

    println!("\x1b[1mProject Lifeline — decentralized offline mesh · acceptance simulator\x1b[0m");

    run(
        "UC2/UC3/NFR-3 · 3-cluster partition + one moving data mule (target ≥95% delivery)",
        scenarios::three_cluster_mule(42),
        700,
    );

    run(
        "UC4 · one gateway lights the mesh (two isolated clusters, internet bridge)",
        scenarios::one_gateway_lights_mesh(42),
        400,
    );

    {
        banner("UC5 · SOS preempts bulk traffic and escapes via the mule");
        let (mut w, sos_id) = scenarios::sos_over_mule(42);
        let report = w.run(400);
        print_report(&report);
        println!("  SOS bundle id: {}", sos_id.to_b64url());
    }

    run(
        "NFR-6 · dense 60-node cluster, 120 messages (no broadcast storm)",
        scenarios::dense_cluster(42, 60, 120),
        60,
    );

    {
        banner("FR-33 · CRDT group membership converges across a 3-cluster partition");
        let mut w = scenarios::group_partition_merge(42);
        w.run(700);
        let converged = w.all_agree_on_group("relief");
        let members = w.group_members(0, "relief");
        let color = if converged { "\x1b[32m" } else { "\x1b[31m" };
        println!(
            "  all {} nodes agree on membership: {}{}\x1b[0m · |relief| = {}",
            w.node_count(),
            color,
            converged,
            members.len()
        );
    }

    {
        banner("FR-46 · PoW postage throttles a 200-message spam flood; SOS unaffected");
        let mut w = scenarios::postage_throttles_spam(42);
        let report = w.run(400);
        print_report(&report);
        println!(
            "  unpaid spam dropped at relays: \x1b[32m{}\x1b[0m",
            report.dropped_nopostage
        );
    }

    println!("\nDone.");
}
