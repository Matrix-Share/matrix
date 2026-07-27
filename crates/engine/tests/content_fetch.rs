//! Mesh fetch-by-CID (FR-13; IPFS model). A provider stores a large object as
//! content-addressed blocks and shares only the small manifest; the recipient
//! pulls the missing blocks by CID over the mesh, verifying each hash, and
//! reassembles the exact bytes. A block already cached is never re-fetched.

use lifeline_core::content::{chunk, DEFAULT_BLOCK_SIZE};
use lifeline_core::Identity;
use lifeline_engine::{EngineConfig, NodeEngine};
use lifeline_transport::{InterfaceCaps, SharedMedium};

fn object(n: usize) -> Vec<u8> {
    (0..n as u32)
        .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
        .collect()
}

#[test]
fn recipient_pulls_blocks_by_cid_and_reassembles() {
    let med = SharedMedium::new();
    let mut provider = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut recipient = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    provider.add_interface(Box::new(med.attach(InterfaceCaps::internet())));
    recipient.add_interface(Box::new(med.attach(InterfaceCaps::internet())));

    // Provider chunks a ~200 KB object into content-addressed blocks.
    let data = object(200_000);
    let manifest = provider.store_content(&data);
    assert!(manifest.blocks.len() >= 3, "object spans several blocks");

    // The recipient must know the provider's key to seal requests to it — a
    // couple of ticks of beaconing establishes that, then it starts the fetch.
    let provider_addr = provider.address().clone();
    let mut started = false;
    let mut got: Option<Vec<u8>> = None;
    for t in 0..200u64 {
        provider.tick(t);
        recipient.tick(t);
        let _ = (provider.take_inbox(), recipient.take_inbox());
        if !started && recipient.contact(&provider_addr).is_some() {
            recipient.fetch_content(manifest.clone(), provider_addr.clone(), t);
            started = true;
        }
        if let Some((root, bytes)) = recipient.take_fetched_content().into_iter().next() {
            assert_eq!(root, manifest.root);
            got = Some(bytes);
            break;
        }
    }

    assert_eq!(got.expect("object fetched over the mesh"), data);
}

#[test]
fn cached_blocks_complete_without_network() {
    // If a node already holds the blocks (same content, dedup by CID), fetching
    // completes immediately with no peer.
    let mut node = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let data = object(150_000);
    let manifest = node.store_content(&data);
    // A fabricated provider address that is never reachable.
    let ghost = Identity::generate(0).address().clone();

    node.fetch_content(manifest.clone(), ghost, 0);
    let fetched = node.take_fetched_content();
    assert_eq!(fetched.len(), 1, "already-cached object completes at once");
    assert_eq!(fetched[0].1, data);
    // Every block was already present.
    assert!(manifest.blocks.iter().all(|c| node.has_block(c)));
}

#[test]
fn swarm_pulls_disjoint_blocks_from_two_providers() {
    // Two providers each hold only HALF the blocks (disjoint sets). Neither can
    // serve the whole object alone, so completing the fetch *requires* pulling
    // from both — the BitTorrent-over-DTN multi-source path.
    let med = SharedMedium::new();
    let mut prov_a = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut prov_b = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut recipient = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    for n in [&mut prov_a, &mut prov_b, &mut recipient] {
        n.add_interface(Box::new(med.attach(InterfaceCaps::internet())));
    }

    // Build the manifest + blocks, then seed even blocks to A, odd blocks to B.
    let data = object(300_000);
    let (manifest, blocks) = chunk(&data, DEFAULT_BLOCK_SIZE);
    assert!(blocks.len() >= 4, "object spans several blocks");
    for (i, (_cid, block)) in blocks.into_iter().enumerate() {
        if i % 2 == 0 {
            prov_a.store_block(block);
        } else {
            prov_b.store_block(block);
        }
    }

    let a_addr = prov_a.address().clone();
    let b_addr = prov_b.address().clone();
    let mut started = false;
    let mut got: Option<Vec<u8>> = None;
    for t in 0..400u64 {
        prov_a.tick(t);
        prov_b.tick(t);
        recipient.tick(t);
        let _ = (
            prov_a.take_inbox(),
            prov_b.take_inbox(),
            recipient.take_inbox(),
        );
        if !started && recipient.contact(&a_addr).is_some() && recipient.contact(&b_addr).is_some()
        {
            recipient.fetch_content_swarm(
                manifest.clone(),
                vec![a_addr.clone(), b_addr.clone()],
                t,
            );
            started = true;
        }
        if let Some((root, bytes)) = recipient.take_fetched_content().into_iter().next() {
            assert_eq!(root, manifest.root);
            got = Some(bytes);
            break;
        }
    }
    assert_eq!(
        got.expect("object completed only by combining both providers"),
        data
    );
}

#[test]
fn swarm_routes_around_a_dead_provider() {
    // Both providers hold the whole object, but provider A goes dark right after
    // contact is established (we stop ticking it). Listing A first, the recipient
    // must rotate its requests to the live provider B and still complete — a
    // black-hole/offline provider cannot stall the transfer.
    let med = SharedMedium::new();
    let mut prov_a = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut prov_b = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    let mut recipient = NodeEngine::new(Identity::generate(0), EngineConfig::default());
    for n in [&mut prov_a, &mut prov_b, &mut recipient] {
        n.add_interface(Box::new(med.attach(InterfaceCaps::internet())));
    }

    let data = object(200_000);
    let manifest = prov_a.store_content(&data);
    let _ = prov_b.store_content(&data);

    let a_addr = prov_a.address().clone();
    let b_addr = prov_b.address().clone();
    let mut started = false;
    let mut got: Option<Vec<u8>> = None;
    for t in 0..500u64 {
        // Let both providers beacon until the recipient knows both keys and has
        // started; afterwards, provider A is "dead" (never ticked again).
        if !started {
            prov_a.tick(t);
        }
        prov_b.tick(t);
        recipient.tick(t);
        let _ = (prov_b.take_inbox(), recipient.take_inbox());
        if !started && recipient.contact(&a_addr).is_some() && recipient.contact(&b_addr).is_some()
        {
            // A listed FIRST so the naive single-source path would target it.
            recipient.fetch_content_swarm(
                manifest.clone(),
                vec![a_addr.clone(), b_addr.clone()],
                t,
            );
            started = true;
        }
        if let Some((root, bytes)) = recipient.take_fetched_content().into_iter().next() {
            assert_eq!(root, manifest.root);
            got = Some(bytes);
            break;
        }
    }
    assert_eq!(
        got.expect("completed via the live provider after A went dark"),
        data
    );
}
