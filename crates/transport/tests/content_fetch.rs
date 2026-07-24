//! Mesh fetch-by-CID (FR-13; IPFS model). A provider stores a large object as
//! content-addressed blocks and shares only the small manifest; the recipient
//! pulls the missing blocks by CID over the mesh, verifying each hash, and
//! reassembles the exact bytes. A block already cached is never re-fetched.

use lifeline_core::Identity;
use lifeline_transport::{EngineConfig, InterfaceCaps, NodeEngine, SharedMedium};

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
