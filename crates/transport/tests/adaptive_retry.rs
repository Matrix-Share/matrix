//! Adaptive retry (FR-32): an unverified message is re-sprayed on new paths
//! after the retry window; a verified one is not retried; and retries stop after
//! delivery / the cap.

use lifeline_core::Identity;
use lifeline_proto::{Payload, PayloadKind, Priority};
use lifeline_transport::{EngineConfig, InterfaceCaps, NodeEngine, SharedMedium};

fn cfg() -> EngineConfig {
    EngineConfig {
        retry_window: 5,
        max_retries: 3,
        respray_copies: 6,
        ..Default::default()
    }
}

fn text() -> Payload {
    Payload {
        kind: PayloadKind::Text,
        body: Some("are you safe?".into()),
        coords: None,
        battery_pct: None,
        attach: None,
        group_id: None,
    }
}

#[test]
fn unreachable_message_retries_up_to_cap_then_stops() {
    // Alice knows Bob's key but has no link to him (no shared medium): the
    // message can never deliver, so it must retry exactly `max_retries` times.
    let mut alice = NodeEngine::new(Identity::generate(0), cfg());
    let bob = Identity::generate(0);
    alice.add_interface(Box::new(SharedMedium::new().attach(InterfaceCaps::ble())));
    alice.add_contact(bob.public());
    alice.submit(&bob.public(), text(), Priority::Normal, 0);

    for t in 0..40u64 {
        alice.tick(t);
    }
    assert_eq!(
        alice.retry_count(),
        3,
        "should retry exactly max_retries times"
    );
    assert_eq!(alice.verified_count(), 0);
}

#[test]
fn delivered_message_is_not_retried() {
    // Alice and Bob share a link: the message delivers and verifies well within
    // the retry window, so no re-spray happens.
    let med = SharedMedium::new();
    let mut alice = NodeEngine::new(Identity::generate(0), cfg());
    let mut bob = NodeEngine::new(Identity::generate(0), cfg());
    alice.add_interface(Box::new(med.attach(InterfaceCaps::ble())));
    bob.add_interface(Box::new(med.attach(InterfaceCaps::ble())));
    alice.add_contact(bob.public());
    alice.submit(&bob.public(), text(), Priority::Normal, 0);

    for t in 0..40u64 {
        alice.tick(t);
        bob.tick(t);
        let _ = bob.take_inbox();
        if alice.verified_count() == 1 {
            break;
        }
    }
    assert_eq!(alice.verified_count(), 1);
    assert_eq!(
        alice.retry_count(),
        0,
        "a promptly-verified message must not retry"
    );
}
