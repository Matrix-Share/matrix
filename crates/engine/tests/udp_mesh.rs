//! Two real `NodeEngine`s meshing over actual UDP sockets — no relay, no server.
//! Proves the infrastructureless peer-to-peer path end to end (seal → UDP →
//! decrypt → signed receipt → offline verification).

use lifeline_core::Identity;
use lifeline_engine::{EngineConfig, NodeEngine};
use lifeline_proto::{Payload, PayloadKind, Priority};
use lifeline_transport::UdpInterface;
use std::net::{Ipv4Addr, SocketAddrV4};

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

fn local_v4(iface: &UdpInterface) -> SocketAddrV4 {
    match iface.local_addr().unwrap() {
        std::net::SocketAddr::V4(v) => SocketAddrV4::new(Ipv4Addr::LOCALHOST, v.port()),
        _ => unreachable!(),
    }
}

#[test]
fn two_nodes_mesh_over_real_udp_no_relay() {
    // Bind two UDP interfaces on loopback (ephemeral ports), no multicast —
    // seed each to the other so the test doesn't depend on multicast support.
    let mut a_if = UdpInterface::bind(0, None, vec![]).unwrap();
    let mut b_if = UdpInterface::bind(0, None, vec![]).unwrap();
    let a_addr = local_v4(&a_if);
    let b_addr = local_v4(&b_if);
    a_if.add_seed(b_addr);
    b_if.add_seed(a_addr);

    let mut alice = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut bob = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    alice.add_interface(Box::new(a_if));
    bob.add_interface(Box::new(b_if));

    alice.add_contact(bob.public());
    alice.submit(
        &bob.public(),
        text("meet at the shelter"),
        Priority::Normal,
        0,
    );

    let mut bob_inbox = 0;
    for t in 0..120u64 {
        alice.tick(t);
        bob.tick(t);
        bob_inbox += bob.take_inbox().len();
        std::thread::sleep(std::time::Duration::from_millis(5));
        if alice.verified_count() == 1 {
            break;
        }
    }

    assert!(bob_inbox >= 1, "Bob must receive the message over UDP");
    assert_eq!(
        alice.verified_count(),
        1,
        "Alice must get a verified receipt over UDP"
    );
}
