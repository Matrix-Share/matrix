//! Geocast over the real transport: a message addressed to a **region** reaches
//! every node whose GPS position is inside it — strangers the sender holds no key
//! for — and no one outside it ("SOS to anyone near here").

use lifeline_core::Identity;
use lifeline_engine::{EngineConfig, NodeEngine};
use lifeline_proto::{Payload, PayloadKind};
use lifeline_transport::{InterfaceCaps, SharedMedium};

fn text(body: &str) -> Payload {
    Payload {
        kind: PayloadKind::Text,
        body: Some(body.into()),
        coords: None,
        battery_pct: None,
        attach: None,
        group_id: None,
    }
}

#[test]
fn geocast_reaches_nodes_in_the_region_only() {
    let med = SharedMedium::new();
    // Sender, a node inside the target area, and a node far away. None are
    // contacts of each other — geocast reaches strangers.
    let mut sender = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut near = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut far = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    for e in [&mut sender, &mut near, &mut far] {
        e.add_interface(Box::new(med.attach(InterfaceCaps::ble())));
    }

    // Target area: downtown SF. `near` is right there; `far` is ~55 km north.
    let (lat, lon) = (37.7749, -122.4194);
    near.set_position(lat, lon);
    far.set_position(lat + 0.5, lon);

    // Geocast to everyone within 500 m of the target.
    let ids = sender.broadcast_geo(lat, lon, 500.0, text("shelter open at the school"), 0);
    assert!(
        !ids.is_empty(),
        "geocast should produce covering-cell bundles"
    );

    let mut near_got = 0usize;
    let mut far_got = 0usize;
    for t in 0..80u64 {
        for e in [&mut sender, &mut near, &mut far] {
            e.tick(t);
        }
        near_got += near
            .take_inbox()
            .iter()
            .filter(|m| m.payload.body.as_deref() == Some("shelter open at the school"))
            .count();
        far_got += far.take_inbox().len();
    }

    assert!(
        near_got >= 1,
        "a node inside the region must receive the geocast"
    );
    assert_eq!(far_got, 0, "a node outside the region must not");
}
