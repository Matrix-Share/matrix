//! End-to-end erasure coding over the real transport (FR-28): a message sent as
//! `k + m` coded fragment bundles reassembles at the recipient and returns a
//! verified delivery receipt for the group.

use lifeline_core::Identity;
use lifeline_proto::{Payload, PayloadKind, Priority};
use lifeline_transport::{EngineConfig, InterfaceCaps, NodeEngine, SharedMedium};

fn big_payload() -> Payload {
    Payload {
        kind: PayloadKind::Text,
        body: Some("evacuate via the north bridge — ".repeat(60)), // ~1.8 KB
        coords: None,
        battery_pct: None,
        attach: None,
        group_id: None,
    }
}

#[test]
fn erasure_coded_message_reassembles_and_verifies() {
    // Small MTU so fragments themselves also get frame-fragmented — exercises
    // both layers (erasure shards + MTU framing).
    let med = SharedMedium::new();
    let mut alice = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut bob = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    alice.add_interface(Box::new(med.attach(InterfaceCaps::ultrasound())));
    bob.add_interface(Box::new(med.attach(InterfaceCaps::ultrasound())));

    alice.add_contact(bob.public());
    // 3 data + 3 parity: any 3 of 6 fragments reconstruct.
    alice.submit_erasure(&bob.public(), big_payload(), 3, 3, Priority::Normal, 0);

    let mut bob_inbox = 0;
    for t in 0..150u64 {
        alice.tick(t);
        bob.tick(t);
        bob_inbox += bob.take_inbox().len();
        if alice.verified_count() == 1 {
            break;
        }
    }

    assert!(bob_inbox >= 1, "Bob must reconstruct + receive the message");
    assert_eq!(
        alice.verified_count(),
        1,
        "Alice must get a verified receipt for the erasure-coded group"
    );
}
