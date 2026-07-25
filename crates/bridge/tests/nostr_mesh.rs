//! End-to-end: two full Lifeline `NodeEngine`s reach each other **over Nostr**.
//! Each node's only interface is a Nostr adapter pointed at a shared relay — no
//! `lifeline-relay`, no radios. Discovery, an end-to-end-encrypted message, and
//! the signed delivery receipt all travel as real signed Nostr events that the
//! relay stores and forwards. This is "Lifeline over the Nostr network."

use lifeline_bridge::nostr::{MockRelay, NostrIdentity, NostrNet};
use lifeline_core::Identity;
use lifeline_proto::{Payload, PayloadKind, Priority};
use lifeline_transport::{BridgeInterface, EngineConfig, NodeEngine};

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

#[test]
fn two_lifeline_nodes_talk_over_nostr() {
    let relay = MockRelay::new();

    let mut alice = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut bob = NodeEngine::new(Identity::generate(0), EngineConfig::default());

    // Each node's sole interface is a Nostr adapter on the shared relay.
    alice.add_interface(Box::new(BridgeInterface::new(NostrNet::new(
        NostrIdentity::from_seed(&[11u8; 32]).unwrap(),
        relay.clone(),
    ))));
    bob.add_interface(Box::new(BridgeInterface::new(NostrNet::new(
        NostrIdentity::from_seed(&[22u8; 32]).unwrap(),
        relay.clone(),
    ))));

    let bob_addr = bob.address().clone();

    // Beacons flow over Nostr so the nodes discover each other.
    for t in 0..6u64 {
        alice.tick(t);
        bob.tick(t);
        let _ = (alice.take_inbox(), bob.take_inbox());
    }
    assert!(
        alice.contact(&bob_addr).is_some(),
        "Alice discovers Bob over the Nostr relay"
    );

    // Alice sends Bob an end-to-end-encrypted message.
    alice.submit_to_addr(
        &bob_addr,
        text("evac point is the stadium"),
        Priority::Normal,
        6,
    );

    let mut got = Vec::new();
    for t in 6..160u64 {
        alice.tick(t);
        bob.tick(t);
        got.extend(bob.take_inbox());
        let _ = alice.take_inbox();
        if alice.verified_count() == 1 {
            break;
        }
    }

    assert!(
        got.iter()
            .any(|m| m.payload.body.as_deref() == Some("evac point is the stadium")),
        "Bob receives the message over Nostr"
    );
    assert!(
        alice.verified_count() == 1,
        "the signed delivery receipt returns to Alice over Nostr"
    );
}
