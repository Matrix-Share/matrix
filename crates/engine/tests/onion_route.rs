//! Onion forwarding through the live engine (FR-49). A message travels
//! A → R1 → R2 → Bob, each relay peeling exactly one layer and forwarding to the
//! next. The relays never see the payload; only Bob delivers it. The route is a
//! forced line so each hop must genuinely forward:
//!
//! ```text
//!   A ──med── R1 ──med── R2 ──med── Bob
//! ```

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
fn onion_routes_through_two_relays_hiding_the_path() {
    let m_ar1 = SharedMedium::new();
    let m_r1r2 = SharedMedium::new();
    let m_r2b = SharedMedium::new();

    let mut a = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut r1 = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut r2 = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut bob = NodeEngine::new(Identity::generate(0), EngineConfig::default());

    a.add_interface(Box::new(m_ar1.attach(InterfaceCaps::internet())));
    r1.add_interface(Box::new(m_ar1.attach(InterfaceCaps::internet())));
    r1.add_interface(Box::new(m_r1r2.attach(InterfaceCaps::internet())));
    r2.add_interface(Box::new(m_r1r2.attach(InterfaceCaps::internet())));
    r2.add_interface(Box::new(m_r2b.attach(InterfaceCaps::internet())));
    bob.add_interface(Box::new(m_r2b.attach(InterfaceCaps::internet())));

    // The sender picks the path; it knows every relay's and the recipient's key.
    let relays = [r1.public(), r2.public()];
    a.submit_onion(
        &relays,
        &bob.public(),
        text("meet at pier 7"),
        Priority::Normal,
        0,
    );

    let mut bob_inbox = Vec::new();
    let (mut r1_seen, mut r2_seen) = (0usize, 0usize);
    for t in 0..200u64 {
        a.tick(t);
        r1.tick(t);
        r2.tick(t);
        bob.tick(t);
        r1_seen += r1.take_inbox().len();
        r2_seen += r2.take_inbox().len();
        for m in bob.take_inbox() {
            bob_inbox.push(m);
        }
        if !bob_inbox.is_empty() {
            break;
        }
    }

    assert_eq!(
        bob_inbox.len(),
        1,
        "Bob must receive exactly the onion payload"
    );
    assert_eq!(
        bob_inbox[0].payload.body.as_deref(),
        Some("meet at pier 7"),
        "payload survives peeling at every hop"
    );
    assert_eq!(
        r1_seen, 0,
        "R1 only forwards — it never sees a delivered payload"
    );
    assert_eq!(
        r2_seen, 0,
        "R2 only forwards — it never sees a delivered payload"
    );
    assert_eq!(
        a.verified_count(),
        0,
        "onion delivery is receipt-less by design (no metadata leaks back)"
    );
}

#[test]
fn onion_empty_path_delivers_directly() {
    let med = SharedMedium::new();
    let mut a = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut bob = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    a.add_interface(Box::new(med.attach(InterfaceCaps::internet())));
    bob.add_interface(Box::new(med.attach(InterfaceCaps::internet())));

    a.submit_onion(
        &[],
        &bob.public(),
        text("direct onion"),
        Priority::Normal,
        0,
    );

    let mut got = Vec::new();
    for t in 0..60u64 {
        a.tick(t);
        bob.tick(t);
        got.extend(bob.take_inbox());
        if !got.is_empty() {
            break;
        }
    }
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].payload.body.as_deref(), Some("direct onion"));
}
