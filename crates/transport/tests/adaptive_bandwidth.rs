//! Bandwidth-adaptive bearer selection (gateway doc §4: a shared gateway is
//! "a straw, not a firehose" — be asynchronous-first and priority-aware). Over a
//! low-bandwidth relay link, a bulky NORMAL message is held back to wait for a
//! fatter bearer, while an SOS (and a small message) still gets through. The same
//! bulky message flows freely once a high-bandwidth path exists.

use lifeline_core::Identity;
use lifeline_proto::{Coords, Payload, PayloadKind, Priority};
use lifeline_transport::{EngineConfig, InterfaceCaps, NodeEngine, SharedMedium};

/// High-entropy body of `n` chars — deliberately *incompressible*, so payload
/// compression can't shrink it under the bearer's size cap.
fn big_body(n: usize) -> String {
    (0..n)
        .map(|i| {
            let x = (i as u64)
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            char::from(33 + ((x >> 33) % 94) as u8)
        })
        .collect()
}

fn text(body: String) -> Payload {
    Payload {
        kind: PayloadKind::Text,
        body: Some(body),
        coords: None,
        battery_pct: None,
        attach: None,
        group_id: None,
    }
}

fn sos() -> Payload {
    Payload {
        kind: PayloadKind::Sos,
        body: Some("SOS".into()),
        coords: Some(Coords {
            lat: 1.0,
            lon: 2.0,
            acc_m: 5,
        }),
        battery_pct: Some(20),
        attach: None,
        group_id: None,
    }
}

/// Build A ──`ab`── R ──`rd`── D and return the four engines. R is a plain relay
/// (not the destination), so the bearer cap applies to A→R.
fn line(
    ab: &SharedMedium,
    rd: &SharedMedium,
    ab_caps: InterfaceCaps,
) -> (NodeEngine, NodeEngine, NodeEngine) {
    let mut a = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut r = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut d = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    a.add_interface(Box::new(ab.attach(ab_caps.clone())));
    r.add_interface(Box::new(ab.attach(ab_caps)));
    r.add_interface(Box::new(rd.attach(InterfaceCaps::internet())));
    d.add_interface(Box::new(rd.attach(InterfaceCaps::internet())));
    (a, r, d)
}

#[test]
fn bulky_normal_is_held_back_over_ultrasound_but_sos_and_small_pass() {
    let ab = SharedMedium::new();
    let rd = SharedMedium::new();
    // A reaches the relay R only over ultrasound (VeryLow, ~512 B cap).
    let (mut a, mut r, mut d) = line(&ab, &rd, InterfaceCaps::ultrasound());
    a.add_contact(d.public());

    // Three messages to D (reachable only past the low-bandwidth hop):
    a.submit(&d.public(), text(big_body(2000)), Priority::Normal, 0); // bulky NORMAL
    a.submit(&d.public(), text("on my way".into()), Priority::Normal, 0); // small NORMAL
    a.submit(&d.public(), sos(), Priority::Sos, 0); // SOS

    let mut got: Vec<String> = Vec::new();
    for t in 0..200u64 {
        a.tick(t);
        r.tick(t);
        d.tick(t);
        for m in d.take_inbox() {
            got.push(m.payload.body.unwrap_or_default());
        }
    }

    assert!(
        got.iter().any(|b| b == "on my way"),
        "a small NORMAL message fits the ultrasound bearer"
    );
    assert!(
        got.iter().any(|b| b == "SOS"),
        "SOS bypasses the bearer cap — emergencies always get out"
    );
    assert!(
        !got.iter().any(|b| b.len() == 2000),
        "the bulky NORMAL message is held back from the low-bandwidth bearer"
    );
}

#[test]
fn bulky_normal_flows_when_a_high_bandwidth_path_exists() {
    let ab = SharedMedium::new();
    let rd = SharedMedium::new();
    // Same topology, but now A↔R is internet (no cap): the bulky message flows.
    let (mut a, mut r, mut d) = line(&ab, &rd, InterfaceCaps::internet());
    a.add_contact(d.public());
    a.submit(&d.public(), text(big_body(2000)), Priority::Normal, 0);

    let mut delivered = false;
    for t in 0..200u64 {
        a.tick(t);
        r.tick(t);
        d.tick(t);
        if d.take_inbox().iter().any(|m| {
            m.payload
                .body
                .as_ref()
                .map(|b| b.len() == 2000)
                .unwrap_or(false)
        }) {
            delivered = true;
            break;
        }
    }
    assert!(
        delivered,
        "over a high-bandwidth bearer the bulky message is delivered normally"
    );
}
