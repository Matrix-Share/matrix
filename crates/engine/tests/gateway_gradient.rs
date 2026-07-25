//! Gateway-awareness in the live node (FR-35/36/37). A gateway emits signed
//! announces that propagate hop-by-hop so every node builds a **gradient** toward
//! it; bundles then flow downhill to the gateway, which **bridges** them onto its
//! uplink to reach destinations that are not on the mesh at all.

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

fn gateway_cfg() -> EngineConfig {
    EngineConfig {
        gateway_caps: vec!["internet".into()],
        ..EngineConfig::default()
    }
}

#[test]
fn gradient_forms_along_a_mesh_line_toward_the_gateway() {
    // A ── B ── G(gateway), each hop a separate medium.
    let m_ab = SharedMedium::new();
    let m_bg = SharedMedium::new();

    let mut a = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut b = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut g = NodeEngine::new(Identity::generate(0), gateway_cfg());

    a.add_interface(Box::new(m_ab.attach(InterfaceCaps::ble())));
    b.add_interface(Box::new(m_ab.attach(InterfaceCaps::ble())));
    b.add_interface(Box::new(m_bg.attach(InterfaceCaps::ble())));
    g.add_interface(Box::new(m_bg.attach(InterfaceCaps::ble())));

    for t in 0..40u64 {
        a.tick(t);
        b.tick(t);
        g.tick(t);
    }

    assert!(g.is_gateway(), "G is configured as a gateway");
    assert_eq!(g.gradient(40), Some(0), "a gateway's own gradient is 0");
    assert_eq!(b.gradient(40), Some(1), "B is one hop from the gateway");
    assert_eq!(a.gradient(40), Some(2), "A is two hops from the gateway");
    assert!(
        a.known_gateways() >= 1,
        "A learned of the gateway via gossip"
    );
}

#[test]
fn mesh_bundle_escapes_to_offmesh_destination_via_gateway_bridge() {
    // A ──ble── B ──ble── G(gateway) ──internet── D
    // D is reachable ONLY through the gateway's uplink, never on the mesh.
    let m_ab = SharedMedium::new();
    let m_bg = SharedMedium::new();
    let m_gd = SharedMedium::new();

    let mut a = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut b = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut g = NodeEngine::new(Identity::generate(0), gateway_cfg());
    let mut d = NodeEngine::new(Identity::generate(0), EngineConfig::default());

    a.add_interface(Box::new(m_ab.attach(InterfaceCaps::ble())));
    b.add_interface(Box::new(m_ab.attach(InterfaceCaps::ble())));
    b.add_interface(Box::new(m_bg.attach(InterfaceCaps::ble())));
    g.add_interface(Box::new(m_bg.attach(InterfaceCaps::ble())));
    g.add_interface(Box::new(m_gd.attach(InterfaceCaps::internet())));
    d.add_interface(Box::new(m_gd.attach(InterfaceCaps::internet())));

    // A knows D's key (scanned) but has no mesh path to it — only the gateway does.
    a.add_contact(d.public());
    a.submit(
        &d.public(),
        text("flood is receding, come home"),
        Priority::Normal,
        0,
    );

    let mut d_inbox = 0usize;
    for t in 0..200u64 {
        a.tick(t);
        b.tick(t);
        g.tick(t);
        d.tick(t);
        d_inbox += d.take_inbox().len();
        let _ = (a.take_inbox(), b.take_inbox(), g.take_inbox());
        if a.verified_count() == 1 {
            break;
        }
    }

    assert!(
        d_inbox >= 1,
        "D receives the message bridged off-mesh through the gateway"
    );
    assert!(
        a.verified_count() == 1,
        "and the delivery receipt routes all the way back to A"
    );
}
