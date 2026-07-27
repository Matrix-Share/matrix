//! Anti-entropy held-bundle digest wired into the live node: a node advertises
//! the bundle ids it holds, and a neighbour records them (to suppress
//! re-offering bundles the peer already has — retiring the previously-inert
//! `PeerInfo.known`). Uses `lifeline-reconcile::fingerprint` to send the digest
//! only when the held set changes.

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
fn a_peers_held_bundle_digest_is_received_and_recorded() {
    let med = SharedMedium::new();
    let mut holder = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut observer = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    holder.add_interface(Box::new(med.attach(InterfaceCaps::internet())));
    observer.add_interface(Box::new(med.attach(InterfaceCaps::internet())));

    // The holder originates (and thus stores) a bundle to an offline recipient,
    // so its held-bundle set — and therefore its digest — is non-empty.
    let recipient = Identity::generate(0);
    holder.add_contact(recipient.public());
    holder.submit_to_addr(recipient.address(), text("hold me"), Priority::Normal, 0);

    // Before any contact the observer knows nothing about the holder's store.
    assert_eq!(observer.peer_digest_ids(), 0);

    // Run contact; the holder broadcasts a digest, the observer records it.
    let mut recorded = false;
    for t in 0..60u64 {
        holder.tick(t);
        observer.tick(t);
        let _ = (holder.take_inbox(), observer.take_inbox());
        if observer.peer_digest_ids() > 0 {
            recorded = true;
            break;
        }
    }
    assert!(
        recorded,
        "observer should record the holder's held-bundle digest over contact"
    );
}

#[test]
fn an_empty_store_advertises_no_digest() {
    // A node holding nothing sends no digest, so a neighbour records no held-bundle
    // facts from it (the differential summary is only sent when there's something
    // in it, and only when it changes).
    let med = SharedMedium::new();
    let mut empty = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut observer = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    empty.add_interface(Box::new(med.attach(InterfaceCaps::internet())));
    observer.add_interface(Box::new(med.attach(InterfaceCaps::internet())));

    for t in 0..40u64 {
        empty.tick(t);
        observer.tick(t);
        let _ = (empty.take_inbox(), observer.take_inbox());
    }
    assert_eq!(
        observer.peer_digest_ids(),
        0,
        "an empty store should advertise no held-bundle digest"
    );
}
