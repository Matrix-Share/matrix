//! Lossy-link ARQ (Dhwani "noise robustness"): a message must still deliver —
//! and its receipt return — over a channel that drops a large fraction of every
//! frame, which is the reality of ultrasound and LoRa. Without selective-repeat
//! retransmission a single lost fragment would stall the whole reassembly
//! forever.

use lifeline_core::Identity;
use lifeline_proto::{Payload, PayloadKind, Priority};
use lifeline_transport::{EngineConfig, InterfaceCaps, NodeEngine, SharedMedium};

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

/// ~2 KB → dozens of fragments over a 128 B ultrasound MTU. At 30% per-frame
/// loss the odds of all ~45 fragments surviving a single blast are ~1e-7, so a
/// verified delivery is proof that ARQ retransmitted the lost ones.
fn big_body() -> String {
    "help ".repeat(400)
}

#[test]
fn delivers_and_verifies_over_30pct_lossy_ultrasound() {
    let med = SharedMedium::new_lossy(300, 0xC0FFEE);
    let mut alice = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut bob = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    alice.add_interface(Box::new(med.attach(InterfaceCaps::ultrasound())));
    bob.add_interface(Box::new(med.attach(InterfaceCaps::ultrasound())));

    alice.add_contact(bob.public());
    alice.submit(&bob.public(), text(&big_body()), Priority::Normal, 0);

    let mut bob_inbox = 0usize;
    for t in 0..600u64 {
        alice.tick(t);
        bob.tick(t);
        bob_inbox += bob.take_inbox().len();
        if alice.verified_count() == 1 {
            break;
        }
    }

    assert!(
        alice.verified_count() == 1,
        "message + receipt must survive a 30%-loss channel via ARQ retransmission"
    );
    assert!(bob_inbox >= 1, "Bob must reassemble the message");
    assert!(
        alice.arq_retransmits() > 0,
        "ARQ must have retransmitted lost fragments on a lossy link"
    );
    assert_eq!(
        alice.arq_pending(),
        0,
        "every reliable message should end fully acknowledged"
    );
}

/// No-regression: on a reliable link the happy path completes within the RTO, so
/// ARQ never needlessly retransmits.
#[test]
fn reliable_link_triggers_no_retransmissions() {
    let med = SharedMedium::new();
    let mut alice = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut bob = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    alice.add_interface(Box::new(med.attach(InterfaceCaps::ultrasound())));
    bob.add_interface(Box::new(med.attach(InterfaceCaps::ultrasound())));

    alice.add_contact(bob.public());
    alice.submit(&bob.public(), text(&big_body()), Priority::Normal, 0);

    for t in 0..80u64 {
        alice.tick(t);
        bob.tick(t);
        let _ = bob.take_inbox();
        if alice.verified_count() == 1 {
            break;
        }
    }
    assert!(alice.verified_count() == 1);
    assert_eq!(
        alice.arq_retransmits(),
        0,
        "a reliable link must not provoke any retransmission"
    );
}
