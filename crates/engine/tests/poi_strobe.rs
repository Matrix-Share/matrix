//! POI (wayfinding) and strobe (crowd-finding) beacons travel to every contact
//! over the mesh and arrive in the recipient's inbox with their content intact —
//! the engine half of the "find each other / find your way / strobe" features.

use lifeline_core::Identity;
use lifeline_engine::{EngineConfig, NodeEngine};
use lifeline_proto::PayloadKind;
use lifeline_transport::{InterfaceCaps, SharedMedium};

#[test]
fn poi_and_strobe_reach_a_contact_over_the_mesh() {
    let med = SharedMedium::new();
    let mut alice = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut bob = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    for e in [&mut alice, &mut bob] {
        e.add_interface(Box::new(med.attach(InterfaceCaps::ble())));
    }
    // Both broadcasts fan out to Alice's contacts, so she must know Bob.
    alice.add_contact(bob.public());

    // Share a place (Main Stage at a known point) and raise a strobe.
    let poi_ids = alice.broadcast_poi("stage\u{1f}Main Stage".into(), 37.7955, -122.3937, 0);
    let strobe_ids = alice.broadcast_strobe("100\u{1f}120\u{1f}30".into(), 0);
    assert!(
        !poi_ids.is_empty(),
        "a POI broadcast should produce a bundle"
    );
    assert!(!strobe_ids.is_empty(), "a strobe should produce a bundle");

    let mut poi = None;
    let mut strobe = None;
    for t in 0..80u64 {
        for e in [&mut alice, &mut bob] {
            e.tick(t);
        }
        for m in bob.take_inbox() {
            match m.payload.kind {
                PayloadKind::Poi => poi = Some(m.payload.clone()),
                PayloadKind::Strobe => strobe = Some(m.payload.clone()),
                _ => {}
            }
        }
    }

    // The POI arrives as a Poi payload carrying its label AND its coordinates.
    let poi = poi.expect("Bob must receive the shared POI");
    assert_eq!(poi.body.as_deref(), Some("stage\u{1f}Main Stage"));
    let c = poi.coords.expect("a POI carries a location");
    assert!((c.lat - 37.7955).abs() < 1e-9 && (c.lon - -122.3937).abs() < 1e-9);

    // The strobe arrives with its shared (start, bpm, seconds) and no location.
    let strobe = strobe.expect("Bob must receive the strobe beacon");
    assert_eq!(strobe.body.as_deref(), Some("100\u{1f}120\u{1f}30"));
    assert!(strobe.coords.is_none(), "a strobe carries no location");
}
