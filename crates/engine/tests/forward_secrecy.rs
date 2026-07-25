//! Forward-secret delivery through the live engine (FR-44). A node advertises a
//! rotating prekey in its beacon; a sender that has heard the beacon seals to
//! that prekey, and the recipient opens it via its prekey ring. The message
//! delivers end-to-end and the reply receipt still returns — the prekey path is
//! transparent to routing, only the encryption key rotates.

use lifeline_core::Identity;
use lifeline_engine::{EngineConfig, NodeEngine};
use lifeline_proto::{Payload, PayloadKind, Priority};
use lifeline_transport::{InterfaceCaps, SharedMedium};

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
fn message_delivers_over_the_forward_secret_prekey_path() {
    let med = SharedMedium::new();
    let mut alice = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut bob = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    alice.add_interface(Box::new(med.attach(InterfaceCaps::internet())));
    bob.add_interface(Box::new(med.attach(InterfaceCaps::internet())));

    let bob_addr = bob.address().clone();

    // Let beacons flow so Alice learns Bob's signed prekey.
    for t in 0..4u64 {
        alice.tick(t);
        bob.tick(t);
        let _ = (alice.take_inbox(), bob.take_inbox());
    }
    // Alice must have learned Bob's prekey from his beacon.
    let learned = alice.contact(&bob_addr).expect("Alice discovered Bob");
    assert!(
        learned.prekey.is_some(),
        "Bob's beacon must advertise a forward-secret prekey"
    );

    // Send to the *learned contact* (carrying the prekey) — the engine seals to it.
    alice.submit_to_addr(&bob_addr, text("rendezvous confirmed"), Priority::Normal, 4);

    let mut got = Vec::new();
    for t in 4..120u64 {
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
            .any(|m| m.payload.body.as_deref() == Some("rendezvous confirmed")),
        "Bob opens the prekey-sealed message via his ring"
    );
    assert!(
        alice.verified_count() == 1,
        "the delivery receipt still returns over the forward-secret path"
    );
}
