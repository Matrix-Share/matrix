//! Custody transfer round-trip (FR-25). A carrier relaying a bundle for someone
//! else hands custody to a committed custodian and frees its own copy — proven
//! end-to-end — while delivery to the final recipient is never harmed.
//!
//! Topology is a forced line so nothing short-circuits the relay chain:
//!
//! ```text
//!   A ──med_AR── R ──med_RB── B(custodian) ──med_BC── C
//! ```
//!
//! A originates a message to C. R carries it and, when custodian B signs for it,
//! releases its copy. B retains custody and delivers to C.

use lifeline_core::Identity;
use lifeline_proto::{Payload, PayloadKind, Priority};
use lifeline_transport::{CustodyRole, EngineConfig, InterfaceCaps, NodeEngine, SharedMedium};

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

fn custodian_cfg() -> EngineConfig {
    EngineConfig {
        custody_role: CustodyRole::Custodian,
        ..EngineConfig::default()
    }
}

#[test]
fn carrier_releases_copy_once_custodian_signs() {
    let med_ar = SharedMedium::new();
    let med_rb = SharedMedium::new();
    let med_bc = SharedMedium::new();

    let mut a = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut r = NodeEngine::new(Identity::generate(0), EngineConfig::default()); // carrier
    let mut b = NodeEngine::new(Identity::generate(0), custodian_cfg()); // custodian
    let mut c = NodeEngine::new(Identity::generate(0), EngineConfig::default());

    a.add_interface(Box::new(med_ar.attach(InterfaceCaps::internet())));
    r.add_interface(Box::new(med_ar.attach(InterfaceCaps::internet())));
    r.add_interface(Box::new(med_rb.attach(InterfaceCaps::internet())));
    b.add_interface(Box::new(med_rb.attach(InterfaceCaps::internet())));
    b.add_interface(Box::new(med_bc.attach(InterfaceCaps::internet())));
    c.add_interface(Box::new(med_bc.attach(InterfaceCaps::internet())));

    // A can only physically reach R, but knows C's key (scanned) to seal to it.
    a.add_contact(c.public());
    let msg_id = a.submit(
        &c.public(),
        text("meet at the north gate"),
        Priority::Normal,
        0,
    );

    let mut c_inbox = 0usize;
    let mut custody_seen = false;
    for t in 0..300u64 {
        a.tick(t);
        r.tick(t);
        b.tick(t);
        c.tick(t);
        c_inbox += c.take_inbox().len();
        let _ = (a.take_inbox(), r.take_inbox(), b.take_inbox());
        if r.router_stats().custody_transfers > 0 {
            custody_seen = true;
        }
        if a.verified_count() == 1 && custody_seen {
            break;
        }
    }

    assert!(
        c_inbox >= 1,
        "the message must still reach its final recipient C"
    );
    assert!(
        r.router_stats().custody_transfers >= 1,
        "carrier R must have released its copy after custodian B signed for it"
    );
    assert!(
        !r.holds_bundle(&msg_id),
        "R should no longer store the specific bundle it handed off to custodian B"
    );
    assert!(
        b.holds_bundle(&msg_id) || c_inbox >= 1,
        "custodian B retains the bundle until it is delivered to C"
    );
    assert!(
        a.verified_count() == 1,
        "end-to-end delivery receipt still returns to origin A"
    );
}

#[test]
fn carrier_never_releases_its_own_originated_message() {
    // A originates directly to a custodian B. B stores it *addressed to itself*
    // (delivered, not relayed) so no custody receipt is generated; and even if
    // one were, A must never release a message it originated.
    let med = SharedMedium::new();
    let mut a = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut b = NodeEngine::new(Identity::generate(0), custodian_cfg());
    a.add_interface(Box::new(med.attach(InterfaceCaps::internet())));
    b.add_interface(Box::new(med.attach(InterfaceCaps::internet())));

    a.add_contact(b.public());
    a.submit(&b.public(), text("direct"), Priority::Normal, 0);

    for t in 0..60u64 {
        a.tick(t);
        b.tick(t);
        let _ = b.take_inbox();
        if a.verified_count() == 1 {
            break;
        }
    }
    assert!(a.verified_count() == 1);
    assert_eq!(
        a.router_stats().custody_transfers,
        0,
        "origin A must not release its own message on any custody signal"
    );
}
