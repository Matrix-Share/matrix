//! End-to-end group messaging over the real transport (FR-12): sender-key
//! distribution + fan-out to members + decrypt on receive.

use lifeline_core::Identity;
use lifeline_engine::{EngineConfig, NodeEngine};
use lifeline_proto::{Payload, PayloadKind};
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

/// Count group text messages (kind Text, carried via GroupOp) an engine received.
fn drain_text(e: &mut NodeEngine) -> Vec<String> {
    e.take_inbox()
        .into_iter()
        .filter(|m| m.payload.kind == PayloadKind::Text)
        .filter_map(|m| m.payload.body)
        .collect()
}

#[test]
fn group_message_fans_out_and_decrypts_for_all_members() {
    let med = SharedMedium::new();
    let mut asha = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut ravi = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut meera = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    for e in [&mut asha, &mut ravi, &mut meera] {
        e.add_interface(Box::new(med.attach(InterfaceCaps::ble())));
    }

    // Asha creates the group and adds Ravi + Meera.
    asha.create_group("relief");
    asha.add_group_member("relief", ravi.public());
    asha.add_group_member("relief", meera.public());

    // Asha sends a group message.
    asha.send_group("relief", text("water point at the school gate"), 0);

    let mut ravi_msgs = Vec::new();
    let mut meera_msgs = Vec::new();
    for t in 0..80u64 {
        for e in [&mut asha, &mut ravi, &mut meera] {
            e.tick(t);
        }
        ravi_msgs.extend(drain_text(&mut ravi));
        meera_msgs.extend(drain_text(&mut meera));
        if !ravi_msgs.is_empty() && !meera_msgs.is_empty() {
            break;
        }
    }

    assert_eq!(
        ravi_msgs,
        vec!["water point at the school gate".to_string()]
    );
    assert_eq!(
        meera_msgs,
        vec!["water point at the school gate".to_string()]
    );
    // Membership converged (Asha + Ravi + Meera).
    assert_eq!(asha.group_members("relief").len(), 3);
}

#[test]
fn non_member_cannot_read_group_messages() {
    let med = SharedMedium::new();
    let mut asha = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut ravi = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut eve = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    for e in [&mut asha, &mut ravi, &mut eve] {
        e.add_interface(Box::new(med.attach(InterfaceCaps::ble())));
    }

    asha.create_group("relief");
    asha.add_group_member("relief", ravi.public());
    // Eve is NOT added, but she is on the same medium and hears everything.
    asha.send_group("relief", text("rendezvous at dawn"), 0);

    let mut ravi_got = Vec::new();
    let mut eve_got = Vec::new();
    for t in 0..80u64 {
        for e in [&mut asha, &mut ravi, &mut eve] {
            e.tick(t);
        }
        ravi_got.extend(drain_text(&mut ravi));
        eve_got.extend(drain_text(&mut eve));
    }

    assert_eq!(ravi_got, vec!["rendezvous at dawn".to_string()]);
    assert!(
        eve_got.is_empty(),
        "a non-member must not read group traffic"
    );
}

#[test]
fn multiple_senders_in_a_group() {
    let med = SharedMedium::new();
    let mut a = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut b = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    for e in [&mut a, &mut b] {
        e.add_interface(Box::new(med.attach(InterfaceCaps::ble())));
    }
    // Both know each other and the group.
    a.create_group("g");
    a.add_group_member("g", b.public());
    b.create_group("g");
    b.add_group_member("g", a.public());

    a.send_group("g", text("from A"), 0);
    b.send_group("g", text("from B"), 0);

    let mut a_got = Vec::new();
    let mut b_got = Vec::new();
    for t in 0..120u64 {
        a.tick(t);
        b.tick(t);
        a_got.extend(drain_text(&mut a));
        b_got.extend(drain_text(&mut b));
    }
    assert!(
        a_got.contains(&"from B".to_string()),
        "A must read B's message"
    );
    assert!(
        b_got.contains(&"from A".to_string()),
        "B must read A's message"
    );
}

/// Security regression: a **direct** 1:1 message that sets `payload.group_id`
/// must NOT be threaded as a group message. The engine exposes the *authenticated*
/// group via `Inbound::group` (set only by the verified sender-keys path), so a
/// spoofed `group_id` in a plain payload is ignored by the UI threading.
#[test]
fn direct_message_cannot_spoof_a_group_thread() {
    let med = SharedMedium::new();
    let mut a = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut b = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    for e in [&mut a, &mut b] {
        e.add_interface(Box::new(med.attach(InterfaceCaps::ble())));
    }
    a.add_contact(b.public());

    // A direct Text to B, but with a forged group tag impersonating an official thread.
    let mut payload = text("EVACUATE NOW — order from command");
    payload.group_id = Some("Rescue Coordination".into());
    a.submit(&b.public(), payload, lifeline_proto::Priority::Normal, 0);

    let mut got = None;
    for t in 0..60u64 {
        for e in [&mut a, &mut b] {
            e.tick(t);
        }
        if let Some(m) = b.take_inbox().into_iter().next() {
            got = Some(m);
        }
    }
    let m = got.expect("B receives the direct message");
    assert_eq!(
        m.group, None,
        "a direct message must never be attributed to a group thread"
    );
}
