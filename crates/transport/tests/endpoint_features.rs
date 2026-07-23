//! Endpoint features over the real transport: "I'm safe" broadcast (FR-41),
//! location sharing (FR-43), and blocklist enforcement (FR-48).

use lifeline_core::Identity;
use lifeline_proto::PayloadKind;
use lifeline_transport::{EngineConfig, InterfaceCaps, NodeEngine, SharedMedium};

fn drive(engines: &mut [NodeEngine], ticks: u64) {
    for t in 0..ticks {
        for e in engines.iter_mut() {
            e.tick(t);
        }
    }
}

#[test]
fn im_safe_broadcasts_to_all_contacts() {
    let med = SharedMedium::new();
    let mut asha = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut ravi = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut meera = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    for e in [&mut asha, &mut ravi, &mut meera] {
        e.add_interface(Box::new(med.attach(InterfaceCaps::ble())));
    }
    asha.add_contact(ravi.public());
    asha.add_contact(meera.public());

    asha.broadcast_safe(Some("I'm safe".into()), 0);

    let mut ravi_safe = 0;
    let mut meera_safe = 0;
    for t in 0..60u64 {
        for e in [&mut asha, &mut ravi, &mut meera] {
            e.tick(t);
        }
        ravi_safe += ravi
            .take_inbox()
            .iter()
            .filter(|m| m.payload.kind == PayloadKind::Safe)
            .count();
        meera_safe += meera
            .take_inbox()
            .iter()
            .filter(|m| m.payload.kind == PayloadKind::Safe)
            .count();
    }
    assert!(
        ravi_safe >= 1 && meera_safe >= 1,
        "both contacts must receive 'I'm safe'"
    );
}

#[test]
fn location_sharing_delivers_coords() {
    let med = SharedMedium::new();
    let mut a = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut b = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    a.add_interface(Box::new(med.attach(InterfaceCaps::ble())));
    b.add_interface(Box::new(med.attach(InterfaceCaps::ble())));
    a.add_contact(b.public());

    a.submit_location(&b.public().id, 12.9716, 77.5946, 8, 0);

    let mut got = None;
    for t in 0..60u64 {
        a.tick(t);
        b.tick(t);
        for m in b.take_inbox() {
            if m.payload.kind == PayloadKind::Location {
                got = m.payload.coords;
            }
        }
        if got.is_some() {
            break;
        }
    }
    let c = got.expect("location must be delivered");
    assert!((c.lat - 12.9716).abs() < 1e-9 && c.acc_m == 8);
}

#[test]
fn blocked_sender_is_dropped_at_endpoint() {
    let med = SharedMedium::new();
    let mut a = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut b = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    a.add_interface(Box::new(med.attach(InterfaceCaps::ble())));
    b.add_interface(Box::new(med.attach(InterfaceCaps::ble())));
    a.add_contact(b.public());

    // Bob blocks Alice.
    b.block(a.public().id);
    assert!(b.is_blocked(&a.public().id));

    a.submit(
        &b.public(),
        lifeline_proto::Payload {
            kind: PayloadKind::Text,
            body: Some("let me in".into()),
            coords: None,
            battery_pct: None,
            attach: None,
            group_id: None,
        },
        lifeline_proto::Priority::Normal,
        0,
    );

    let mut delivered = 0;
    let mut engines = [a, b];
    drive(&mut engines, 60);
    delivered += engines[1].take_inbox().len();

    // Blocked: nothing reaches Bob's app, and Alice never gets a receipt.
    assert_eq!(delivered, 0, "blocked sender's message must be dropped");
    assert_eq!(
        engines[0].verified_count(),
        0,
        "no receipt for a blocked message"
    );
}
