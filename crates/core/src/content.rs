//! Content-addressed blocks (PRD FR-13; Benet — IPFS/IPLD Merkle-DAG model).
//!
//! Large attachments and cached content (official alerts, web pages) are split
//! into fixed-size **blocks**, each named by its BLAKE3 hash — its **CID**
//! (content id). An ordered [`Manifest`] of block CIDs (itself named by a root
//! CID) describes the whole object. Because names *are* hashes:
//!
//! * **Integrity is intrinsic** — a fetched block is trusted only if it hashes
//!   to the CID that was requested; tampering is detected, not assumed away.
//! * **Dedup is free** — identical content yields identical CIDs, so a block is
//!   stored and carried once no matter how many messages reference it.
//! * **Fetch-by-hash** — a small manifest travels in a message; recipients pull
//!   the referenced blocks from whichever peer has them (the mesh fetch protocol
//!   is a thin layer on top of this store).

use crate::crypto::blake3_32;
use crate::{CoreError, Result};
use lifeline_proto::codec::{from_cbor, to_cbor};
use lifeline_proto::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Default block size (64 KiB — matches the PRD attachment-chunk ceiling).
pub const DEFAULT_BLOCK_SIZE: usize = 64 * 1024;

/// The content id of a block: its BLAKE3-256 hash.
pub fn cid_of(block: &[u8]) -> Bytes {
    Bytes::new(blake3_32(block).to_vec())
}

/// An ordered description of a content-addressed object (the "Merkle root").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// CID of this manifest (hash of its canonical encoding).
    pub root: Bytes,
    /// Total byte length of the reassembled object.
    pub total_len: u64,
    /// Block size used when chunking.
    pub block_size: u32,
    /// Ordered CIDs of the object's blocks.
    pub blocks: Vec<Bytes>,
}

fn manifest_root(total_len: u64, block_size: u32, blocks: &[Bytes]) -> Bytes {
    // Root = hash of the canonical (len, size, block-cids) tuple.
    let encoded = to_cbor(&(total_len, block_size, blocks)).expect("cbor manifest");
    cid_of(&encoded)
}

/// Split `data` into content-addressed blocks. Returns the manifest and the
/// `(cid, block)` pairs to store/carry.
pub fn chunk(data: &[u8], block_size: usize) -> (Manifest, Vec<(Bytes, Vec<u8>)>) {
    let block_size = block_size.max(1);
    let mut blocks = Vec::new();
    let mut cids = Vec::new();
    for chunk in data.chunks(block_size) {
        let cid = cid_of(chunk);
        cids.push(cid.clone());
        blocks.push((cid, chunk.to_vec()));
    }
    let root = manifest_root(data.len() as u64, block_size as u32, &cids);
    let manifest = Manifest {
        root,
        total_len: data.len() as u64,
        block_size: block_size as u32,
        blocks: cids,
    };
    (manifest, blocks)
}

impl Manifest {
    /// Verify this manifest's root CID is self-consistent.
    pub fn verify_root(&self) -> bool {
        manifest_root(self.total_len, self.block_size, &self.blocks) == self.root
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        Ok(to_cbor(self)?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Manifest> {
        Ok(from_cbor(bytes)?)
    }
}

/// A content-addressed block store.
#[derive(Debug, Default)]
pub struct BlockStore {
    blocks: HashMap<Bytes, Vec<u8>>,
}

impl BlockStore {
    pub fn new() -> Self {
        BlockStore::default()
    }

    /// Store a block, returning its CID. The CID is computed here, so a block
    /// can never be stored under a wrong name.
    pub fn put(&mut self, block: Vec<u8>) -> Bytes {
        let cid = cid_of(&block);
        self.blocks.entry(cid.clone()).or_insert(block);
        cid
    }

    /// Store all `(cid, block)` pairs, verifying each hash. Returns an error if
    /// any block does not match its claimed CID.
    pub fn put_all(&mut self, items: Vec<(Bytes, Vec<u8>)>) -> Result<()> {
        for (cid, block) in items {
            if cid_of(&block) != cid {
                return Err(CoreError::Log("block cid mismatch".into()));
            }
            self.blocks.insert(cid, block);
        }
        Ok(())
    }

    pub fn get(&self, cid: &Bytes) -> Option<&Vec<u8>> {
        self.blocks.get(cid)
    }

    pub fn has(&self, cid: &Bytes) -> bool {
        self.blocks.contains_key(cid)
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// CIDs from `manifest` that this store is missing (what to fetch next).
    pub fn missing(&self, manifest: &Manifest) -> Vec<Bytes> {
        manifest
            .blocks
            .iter()
            .filter(|c| !self.has(c))
            .cloned()
            .collect()
    }

    /// Reassemble the object described by `manifest` from stored blocks,
    /// verifying every block hash and the total length.
    pub fn reassemble(&self, manifest: &Manifest) -> Result<Vec<u8>> {
        if !manifest.verify_root() {
            return Err(CoreError::Log("manifest root mismatch".into()));
        }
        let mut out = Vec::with_capacity(manifest.total_len as usize);
        for cid in &manifest.blocks {
            let block = self
                .get(cid)
                .ok_or_else(|| CoreError::Log("missing block".into()))?;
            if &cid_of(block) != cid {
                return Err(CoreError::Log("block cid mismatch".into()));
            }
            out.extend_from_slice(block);
        }
        if out.len() as u64 != manifest.total_len {
            return Err(CoreError::Log("reassembled length mismatch".into()));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_store_reassemble_roundtrip() {
        let data: Vec<u8> = (0..200_000u32).map(|i| (i % 253) as u8).collect();
        let (manifest, blocks) = chunk(&data, 64 * 1024);
        assert!(manifest.verify_root());
        assert!(manifest.blocks.len() >= 3);

        let mut store = BlockStore::new();
        store.put_all(blocks).unwrap();
        assert!(store.missing(&manifest).is_empty());
        assert_eq!(store.reassemble(&manifest).unwrap(), data);
    }

    #[test]
    fn identical_content_dedups() {
        let mut store = BlockStore::new();
        let a = store.put(b"the flood is receding".to_vec());
        let b = store.put(b"the flood is receding".to_vec());
        assert_eq!(a, b);
        assert_eq!(store.len(), 1, "same content stored once");
    }

    #[test]
    fn corrupted_block_is_detected() {
        let data = vec![7u8; 100_000];
        let (_manifest, mut blocks) = chunk(&data, 32 * 1024);
        // Corrupt one block's bytes but keep its claimed CID.
        blocks[0].1[0] ^= 0xFF;
        let mut store = BlockStore::new();
        assert!(
            store.put_all(blocks).is_err(),
            "cid mismatch must be caught"
        );
    }

    #[test]
    fn missing_blocks_reported_and_reassembly_fails() {
        let data = vec![1u8; 100_000];
        let (manifest, blocks) = chunk(&data, 32 * 1024);
        let mut store = BlockStore::new();
        // Store all but the last block.
        let mut partial = blocks;
        partial.pop();
        store.put_all(partial).unwrap();
        assert_eq!(store.missing(&manifest).len(), 1);
        assert!(store.reassemble(&manifest).is_err());
    }
}
