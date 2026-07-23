//! Integration tests: the *same* end-to-end-encrypted bundle delivers over
//! completely different network types, fragmenting to each interface's MTU.
//! This is the concrete proof of "bind to no single transport" (spectrum §3).

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

/// Drive two engines that share one medium until Alice's message is verified.
fn deliver_over(caps: InterfaceCaps, body: &str) -> (bool, usize) {
    let med = SharedMedium::new();
    let mut alice = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut bob = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    alice.add_interface(Box::new(med.attach(caps.clone())));
    bob.add_interface(Box::new(med.attach(caps)));

    // Alice knows Bob (as if scanned by QR) and sends.
    alice.add_contact(bob.public());
    alice.submit(&bob.public(), text(body), Priority::Normal, 0);

    let mut bob_inbox = 0usize;
    for t in 0..80u64 {
        alice.tick(t);
        bob.tick(t);
        bob_inbox += bob.take_inbox().len();
        if alice.verified_count() == 1 {
            break;
        }
    }
    (alice.verified_count() == 1, bob_inbox)
}

/// A payload big enough to force many fragments on small-MTU links.
fn big_body() -> String {
    "help ".repeat(400) // ~2 KB → dozens of fragments over ultrasound/LoRa
}

#[test]
fn delivers_over_ultrasound_with_fragmentation() {
    // Ultrasound MTU is 128 B — a 2 KB message must fragment and reassemble.
    let (verified, inbox) = deliver_over(InterfaceCaps::ultrasound(), &big_body());
    assert!(verified, "ultrasound delivery must be verified end-to-end");
    assert!(inbox >= 1, "Bob must receive the reassembled message");
}

#[test]
fn delivers_over_ble() {
    let (verified, inbox) = deliver_over(InterfaceCaps::ble(), &big_body());
    assert!(verified);
    assert!(inbox >= 1);
}

#[test]
fn delivers_over_lora() {
    let (verified, _) = deliver_over(InterfaceCaps::lora_in865(), &big_body());
    assert!(verified);
}

#[test]
fn delivers_over_internet_single_frame() {
    // Internet MTU (64 KB) carries the whole bundle in one frame.
    let (verified, _) = deliver_over(InterfaceCaps::internet(), "quick note");
    assert!(verified);
}

/// FR-22: a node running BLE *and* ultrasound *and* internet concurrently still
/// delivers — the same bundle can take whichever link is available.
#[test]
fn concurrent_heterogeneous_interfaces() {
    let ble = SharedMedium::new();
    let sound = SharedMedium::new();
    let net = SharedMedium::new();

    let mut alice = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut bob = NodeEngine::new(Identity::generate(0), EngineConfig::default());

    // Alice reaches Bob only over ultrasound; both also hold other idle links.
    alice.add_interface(Box::new(ble.attach(InterfaceCaps::ble())));
    alice.add_interface(Box::new(sound.attach(InterfaceCaps::ultrasound())));
    alice.add_interface(Box::new(net.attach(InterfaceCaps::internet())));
    bob.add_interface(Box::new(sound.attach(InterfaceCaps::ultrasound())));
    bob.add_interface(Box::new(InterfaceCaps::wifi_aware()).map_medium(&SharedMedium::new()));

    assert_eq!(alice.interface_count(), 3);

    alice.add_contact(bob.public());
    alice.submit(&bob.public(), text(&big_body()), Priority::Normal, 0);

    let mut delivered = 0;
    for t in 0..80u64 {
        alice.tick(t);
        bob.tick(t);
        delivered += bob.take_inbox().len();
        if alice.verified_count() == 1 {
            break;
        }
    }
    assert!(
        alice.verified_count() == 1,
        "delivered over the shared ultrasound link"
    );
    assert!(delivered >= 1);
}

/// Tiny helper so the test above reads cleanly.
trait CapsAttach {
    fn map_medium(self, med: &SharedMedium) -> Box<dyn lifeline_transport::Interface>;
}
impl CapsAttach for InterfaceCaps {
    fn map_medium(self, med: &SharedMedium) -> Box<dyn lifeline_transport::Interface> {
        Box::new(med.attach(self))
    }
}
