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

#[test]
fn place_channel_reaches_joiners_regardless_of_position() {
    let med = SharedMedium::new();
    // A poster, someone who joined the place channel (but is nowhere near it), and
    // a stranger who joined a *different* place. None are contacts.
    let mut poster = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut joiner = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut stranger = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    for e in [&mut poster, &mut joiner, &mut stranger] {
        e.add_interface(Box::new(med.attach(InterfaceCaps::ble())));
    }

    // Join-by-place: the channel is a geohash cell. The joiner subscribes to it
    // without ever setting a GPS position; the stranger joins an unrelated cell.
    let cell = "9q8yyk"; // downtown SF, precision 6
    joiner.join_region(cell);
    stranger.join_region("gbsuv7"); // somewhere in the UK

    let id = poster.post_to_region(cell, text("meet at the fountain"), 0);
    assert!(id.is_some(), "posting to a place channel should produce a bundle");

    let mut joiner_hits = 0usize;
    let mut stranger_got = 0usize;
    for t in 0..80u64 {
        for e in [&mut poster, &mut joiner, &mut stranger] {
            e.tick(t);
        }
        for m in joiner.take_inbox() {
            if m.payload.body.as_deref() == Some("meet at the fountain") {
                // The message must be tagged with the place channel it arrived on.
                assert_eq!(m.region.as_deref(), Some(cell));
                joiner_hits += 1;
            }
        }
        stranger_got += stranger.take_inbox().len();
    }

    assert!(joiner_hits >= 1, "a node that joined the place must receive the post");
    assert_eq!(stranger_got, 0, "a node that did not join the place must not");
}
