//! Identity key rotation & revocation through the live engine (G4). A contact can
//! publish a signed certificate retiring its key in favour of a successor (or with
//! no successor); peers that trust the old key migrate their directory. This is
//! the mechanism Nostr lacks natively — made cryptographic and machine-checkable.

use lifeline_core::rotation::RetireReason;
use lifeline_core::Identity;
use lifeline_engine::{EngineConfig, NodeEngine};
use lifeline_transport::{InterfaceCaps, SharedMedium};

/// Bring two nodes onto a shared medium and let beacons make them mutual contacts.
fn pair() -> (NodeEngine, NodeEngine, SharedMedium) {
    let med = SharedMedium::new();
    let mut a = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut b = NodeEngine::new(Identity::generate(1), EngineConfig::default());
    a.add_interface(Box::new(med.attach(InterfaceCaps::internet())));
    b.add_interface(Box::new(med.attach(InterfaceCaps::internet())));
    for t in 0..4u64 {
        a.tick(t);
        b.tick(t);
        let _ = (a.take_inbox(), b.take_inbox());
    }
    (a, b, med)
}

fn pump(a: &mut NodeEngine, b: &mut NodeEngine, from: u64, to: u64) {
    for t in from..to {
        a.tick(t);
        b.tick(t);
        let _ = (a.take_inbox(), b.take_inbox());
    }
}

#[test]
fn a_contact_rotation_migrates_the_directory() {
    let (mut alice, mut bob, _med) = pair();
    let alice_old = alice.address().clone();
    assert!(
        bob.contact(&alice_old).is_some(),
        "Bob must know Alice before she rotates"
    );

    // Alice rolls to a fresh identity and announces it to her contacts.
    let alice_new = Identity::generate(99);
    let alice_new_addr = alice_new.address().clone();
    alice.broadcast_key_rotation(&alice_new.public(), RetireReason::Rotated, 4);

    pump(&mut alice, &mut bob, 4, 80);

    // Bob has migrated: the old address is gone, the successor is known, and a
    // lookup on the retired address redirects to the live one.
    assert!(
        bob.contact(&alice_old).is_none(),
        "Bob must retire Alice's old key"
    );
    assert!(
        bob.contact(&alice_new_addr).is_some(),
        "Bob must adopt Alice's new key"
    );
    assert_eq!(
        bob.resolve_identity(&alice_old),
        &alice_new_addr,
        "a lookup on the old address must resolve to the successor"
    );
}

#[test]
fn a_revocation_drops_the_contact() {
    let (mut alice, mut bob, _med) = pair();
    let alice_addr = alice.address().clone();
    assert!(bob.contact(&alice_addr).is_some());

    alice.broadcast_key_revocation(RetireReason::Compromised, 4);
    pump(&mut alice, &mut bob, 4, 80);

    assert!(
        bob.contact(&alice_addr).is_none(),
        "a verified revocation must retire the contact"
    );
}

#[test]
fn an_older_rotation_cannot_override_a_newer_one() {
    let (mut alice, mut bob, _med) = pair();
    let alice_old = alice.address().clone();

    // Newer rotation (issued_at=10) → key2. Applied first, in order.
    let key2 = Identity::generate(21);
    let key2_addr = key2.address().clone();
    alice.broadcast_key_rotation(&key2.public(), RetireReason::Rotated, 10);
    pump(&mut alice, &mut bob, 4, 50);
    assert_eq!(bob.resolve_identity(&alice_old), &key2_addr);

    // A replayed OLDER cert (issued_at=5 < 10) for the same identity, pointing at a
    // different key, must be ignored by the monotonicity guard.
    let key3 = Identity::generate(22);
    let key3_addr = key3.address().clone();
    alice.broadcast_key_rotation(&key3.public(), RetireReason::Rotated, 5);
    pump(&mut alice, &mut bob, 50, 100);

    assert_eq!(
        bob.resolve_identity(&alice_old),
        &key2_addr,
        "a stale (older) rotation must not revert Bob off the newer key"
    );
    assert!(
        bob.contact(&key3_addr).is_none(),
        "the stale rotation's target must never be adopted"
    );
}
