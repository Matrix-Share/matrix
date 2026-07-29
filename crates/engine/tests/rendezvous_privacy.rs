//! Recipient-unlinkable private delivery (G2). A private send addresses the
//! bundle to the recipient's *rotating rendezvous address* — `HKDF(recipient
//! sign_pub, epoch)` — instead of their real network address. This test proves,
//! end to end through the live engine, that:
//!   1. the address on the wire is NOT the recipient's real address,
//!   2. the intended recipient still receives the message (it recognizes its own
//!      rendezvous address), and
//!   3. a third party on the same medium does not receive it, and
//!   4. it is receipt-less (no delivery receipt leaks the recipient's real
//!      address back to the sender).

use lifeline_core::rendezvous::{epoch_of, rendezvous_addr};
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
fn private_send_hides_the_recipient_address_yet_still_delivers() {
    let med = SharedMedium::new();
    let mut alice = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut bob = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut carol = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    alice.add_interface(Box::new(med.attach(InterfaceCaps::internet())));
    bob.add_interface(Box::new(med.attach(InterfaceCaps::internet())));
    carol.add_interface(Box::new(med.attach(InterfaceCaps::internet())));

    let bob_addr = bob.address().clone();

    // Let beacons flow so Alice learns Bob's public identity (incl. sign_pub).
    for t in 0..4u64 {
        alice.tick(t);
        bob.tick(t);
        carol.tick(t);
        let _ = (alice.take_inbox(), bob.take_inbox(), carol.take_inbox());
    }
    let bob_pub = alice.contact(&bob_addr).expect("Alice discovered Bob");

    // Sanity: the rendezvous address Alice will use is NOT Bob's real address —
    // so a carrier never sees Bob's stable address on this bundle.
    let send_t = 4u64;
    let rv = rendezvous_addr(bob_pub.sign_pub.as_slice(), epoch_of(send_t));
    assert_ne!(
        rv, bob_addr,
        "the rendezvous address must differ from the real recipient address"
    );

    // Private send: addressed to the rendezvous tag, sealed to Bob's real key.
    alice.submit_private(
        &bob_pub,
        text("meet at the bridge"),
        Priority::Normal,
        send_t,
    );

    let mut bob_got = Vec::new();
    let mut carol_got = Vec::new();
    for t in send_t..160u64 {
        alice.tick(t);
        bob.tick(t);
        carol.tick(t);
        bob_got.extend(bob.take_inbox());
        carol_got.extend(carol.take_inbox());
        let _ = alice.take_inbox();
        if !bob_got.is_empty() {
            break;
        }
    }

    // 2. Bob received it despite the address never being his real one.
    assert_eq!(bob_got.len(), 1, "Bob must receive the private message");
    assert_eq!(
        bob_got[0].payload.body.as_deref(),
        Some("meet at the bridge")
    );
    // 3. Carol, a third party on the same medium, must not receive it.
    assert!(
        carol_got.is_empty(),
        "a non-recipient must not receive a rendezvous-addressed bundle"
    );
    // 4. Receipt-less: no delivery receipt returns to Alice (a receipt would carry
    //    Bob's real address and defeat the point).
    for t in 160..200u64 {
        alice.tick(t);
        bob.tick(t);
        let _ = (alice.take_inbox(), bob.take_inbox());
    }
    assert_eq!(
        alice.verified_count(),
        0,
        "a private (rendezvous) send is receipt-less by design"
    );
}
