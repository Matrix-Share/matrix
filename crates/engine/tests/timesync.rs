//! Differential time-sync wired into the live node: a node with a GPS reference
//! disciplines a neighbour's "mesh time" over ordinary beacon contact — while the
//! clock stays advisory (never a TTL/security input).

use lifeline_core::Identity;
use lifeline_engine::{EngineConfig, NodeEngine};
use lifeline_transport::{InterfaceCaps, SharedMedium};

#[test]
fn a_gps_node_disciplines_a_peers_mesh_clock() {
    let med = SharedMedium::new();
    let mut gps = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut peer = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    gps.add_interface(Box::new(med.attach(InterfaceCaps::internet())));
    peer.add_interface(Box::new(med.attach(InterfaceCaps::internet())));

    // They must be mutual contacts for the peer to trust the GPS node's beacon
    // time (we only discipline to already-established contacts).
    gps.add_contact(peer.public());
    peer.add_contact(gps.public());

    // The GPS node's wall clock lags "true" time by 500 s; it sets an
    // authoritative reference so it becomes stratum-1.
    let skew = 500i64;
    // At tick t, local now = t; the GPS node declares true time = t + skew.
    gps.set_gps_time(skew, 0);

    // The peer has no fix yet.
    assert!(peer.mesh_time(0).is_none());

    // Run contact for a while; beacons carry the time reference.
    let mut disciplined = None;
    for t in 0..80u64 {
        // Keep the GPS reference current as the clock advances.
        gps.set_gps_time(t as i64 + skew, t);
        gps.tick(t);
        peer.tick(t);
        let _ = (gps.take_inbox(), peer.take_inbox());
        if let Some(mesh) = peer.mesh_time(t) {
            disciplined = Some((t, mesh));
            break;
        }
    }

    let (t, mesh) = disciplined.expect("peer should acquire a mesh-time fix from the GPS beacon");
    // The peer's mesh time should be close to true time (t + skew), within the
    // crate's bounded-slew convergence.
    let truth = t as i64 + skew;
    assert!(
        (mesh - truth).abs() <= skew,
        "mesh time {mesh} should be disciplined toward truth {truth}"
    );
    // And it must be a real correction away from the peer's raw local clock.
    assert!(
        mesh > t as i64,
        "mesh time must reflect the +skew reference"
    );
}

#[test]
fn an_unpaired_beacon_does_not_move_our_clock() {
    // A node that is NOT an established contact must not be able to set our time
    // (only TOFU-paired peers discipline us; a stranger's beacon is ignored).
    let med = SharedMedium::new();
    let mut gps = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut victim = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    gps.add_interface(Box::new(med.attach(InterfaceCaps::internet())));
    victim.add_interface(Box::new(med.attach(InterfaceCaps::internet())));
    // NOTE: no add_contact on the victim side — the gps node is a stranger.
    gps.set_gps_time(9_000_000, 0);

    for t in 0..40u64 {
        gps.set_gps_time(9_000_000 + t as i64, t);
        gps.tick(t);
        victim.tick(t);
        let _ = (gps.take_inbox(), victim.take_inbox());
    }
    assert!(
        victim.mesh_time(40).is_none(),
        "a stranger's beacon must not discipline our clock"
    );
}
