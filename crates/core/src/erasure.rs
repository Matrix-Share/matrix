//! Erasure / fountain coding for cross-partition resilience (PRD FR-28;
//! network-layer "Problem C").
//!
//! > *"Split a message into coded fragments so it reconstructs even if only some
//! > carriers make it out — resilient to partial escape."*
//!
//! A sealed [`Bundle`]'s ciphertext is split with a Reed-Solomon (MDS) code into
//! `k` data + `m` parity shards (`n = k + m`). **Any `k` of the `n`** shards
//! reconstruct the original. Each shard is wrapped in its own independent
//! fragment bundle that sprays and mules on its own, so if only some carriers
//! escape a partition, the message still gets through — at a fraction of the
//! overhead of replicating whole copies.
//!
//! Integrity is preserved end-to-end: the fragment header is *not* signed, but a
//! tampered shard yields a wrong reconstruction whose recovered ciphertext fails
//! the sender's signature check at [`crate::message::open_bundle`] — so a forged
//! fragment can prevent delivery but never forge content.

use crate::{CoreError, Result};
use lifeline_proto::{Bundle, Bytes, FragHeader};
use reed_solomon_erasure::galois_8::ReedSolomon;
use std::collections::{HashMap, HashSet};

/// Default coding rate: 4 data + 4 parity ⇒ any 4 of 8 fragments reconstruct.
pub const DEFAULT_K: usize = 4;
pub const DEFAULT_M: usize = 4;

/// Bound on concurrently-reassembling groups (memory safety against junk).
const MAX_GROUPS: usize = 4096;

fn encode_shards(data: &[u8], k: usize, m: usize) -> Result<Vec<Vec<u8>>> {
    let shard_len = data.len().div_ceil(k).max(1);
    let mut shards: Vec<Vec<u8>> = Vec::with_capacity(k + m);
    for i in 0..k {
        let start = i * shard_len;
        let mut s = vec![0u8; shard_len];
        if start < data.len() {
            let end = (start + shard_len).min(data.len());
            s[..end - start].copy_from_slice(&data[start..end]);
        }
        shards.push(s);
    }
    shards.resize(k + m, vec![0u8; shard_len]);
    let r = ReedSolomon::new(k, m).map_err(|e| CoreError::Crypto(format!("rs new: {e}")))?;
    r.encode(&mut shards)
        .map_err(|e| CoreError::Crypto(format!("rs encode: {e}")))?;
    Ok(shards)
}

fn decode_shards(
    present: &[(usize, Vec<u8>)],
    k: usize,
    m: usize,
    orig_len: usize,
) -> Result<Vec<u8>> {
    if present.len() < k {
        return Err(CoreError::Crypto("not enough shards".into()));
    }
    let n = k + m;
    let mut opt: Vec<Option<Vec<u8>>> = vec![None; n];
    for (idx, s) in present {
        if *idx < n {
            opt[*idx] = Some(s.clone());
        }
    }
    let r = ReedSolomon::new(k, m).map_err(|e| CoreError::Crypto(format!("rs new: {e}")))?;
    r.reconstruct(&mut opt)
        .map_err(|e| CoreError::Crypto(format!("rs reconstruct: {e}")))?;
    let mut out = Vec::with_capacity(k * present[0].1.len());
    for shard in opt.into_iter().take(k) {
        out.extend_from_slice(
            &shard.ok_or_else(|| CoreError::Crypto("missing data shard".into()))?,
        );
    }
    out.truncate(orig_len);
    Ok(out)
}

/// Split a sealed bundle into `n = k + m` erasure-coded fragment bundles.
pub fn fragment_bundle(original: &Bundle, k: usize, m: usize) -> Result<Vec<Bundle>> {
    let cipher = original.ciphertext.as_slice();
    let shards = encode_shards(cipher, k, m)?;
    let group = original.bundle_id.clone();
    let n = k + m;
    let mut out = Vec::with_capacity(n);
    for (idx, shard) in shards.into_iter().enumerate() {
        let mut b = original.clone();
        b.bundle_id = Bytes::new(crate::crypto::random_bytes(16));
        b.ciphertext = Bytes::new(shard);
        b.frag = Some(FragHeader {
            group: group.clone(),
            idx: idx as u16,
            k: k as u16,
            n: n as u16,
            orig_len: cipher.len() as u32,
        });
        out.push(b);
    }
    Ok(out)
}

struct GroupBuf {
    template: Bundle,
    k: usize,
    m: usize,
    orig_len: usize,
    shards: HashMap<u16, Vec<u8>>,
}

/// Collects fragments and reconstructs original bundles once `k` shards arrive.
#[derive(Default)]
pub struct FragmentCollector {
    groups: HashMap<Bytes, GroupBuf>,
    done: HashSet<Bytes>,
}

impl FragmentCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a fragment bundle. Returns the reconstructed original bundle the
    /// first time `>= k` distinct shards of its group are present.
    pub fn accept(&mut self, frag: &Bundle) -> Option<Bundle> {
        let h = frag.frag.as_ref()?.clone();
        if self.done.contains(&h.group) {
            return None;
        }
        if h.k == 0 || h.n < h.k {
            return None; // malformed header
        }
        // Bound memory against a flood of junk groups.
        if self.groups.len() >= MAX_GROUPS && !self.groups.contains_key(&h.group) {
            return None;
        }
        let ready = {
            let buf = self
                .groups
                .entry(h.group.clone())
                .or_insert_with(|| GroupBuf {
                    template: frag.clone(),
                    k: h.k as usize,
                    m: (h.n - h.k) as usize,
                    orig_len: h.orig_len as usize,
                    shards: HashMap::new(),
                });
            buf.shards
                .entry(h.idx)
                .or_insert_with(|| frag.ciphertext.0.clone());
            buf.shards.len() >= buf.k
        };
        if !ready {
            return None;
        }

        let buf = self.groups.get(&h.group)?;
        let present: Vec<(usize, Vec<u8>)> = buf
            .shards
            .iter()
            .map(|(i, s)| (*i as usize, s.clone()))
            .collect();
        let cipher = decode_shards(&present, buf.k, buf.m, buf.orig_len).ok()?;
        let mut recon = buf.template.clone();
        recon.bundle_id = h.group.clone();
        recon.ciphertext = Bytes::new(cipher);
        recon.frag = None;

        self.done.insert(h.group.clone());
        self.groups.remove(&h.group);
        Some(recon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_k_of_n_reconstructs() {
        let data: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        let (k, m) = (4, 4);
        let shards = encode_shards(&data, k, m).unwrap();
        assert_eq!(shards.len(), k + m);

        // Every choice of k shards (drop the other m) must reconstruct.
        // Test a representative spread of k-subsets by dropping each pair.
        for drop_a in 0..(k + m) {
            for drop_b in (drop_a + 1)..(k + m) {
                let present: Vec<(usize, Vec<u8>)> = shards
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != drop_a && *i != drop_b)
                    .map(|(i, s)| (i, s.clone()))
                    .collect();
                assert_eq!(present.len(), k + m - 2); // still >= k
                let out = decode_shards(&present, k, m, data.len()).unwrap();
                assert_eq!(out, data, "dropping {drop_a},{drop_b} failed");
            }
        }
    }

    #[test]
    fn fewer_than_k_fails() {
        let data = vec![7u8; 500];
        let (k, m) = (4, 4);
        let shards = encode_shards(&data, k, m).unwrap();
        let present: Vec<(usize, Vec<u8>)> = shards
            .iter()
            .enumerate()
            .take(k - 1)
            .map(|(i, s)| (i, s.clone()))
            .collect();
        assert!(decode_shards(&present, k, m, data.len()).is_err());
    }

    #[test]
    fn fragment_and_collect_with_only_k() {
        use crate::message::{seal_bundle, SealOptions};
        use crate::Identity;
        use lifeline_proto::{Payload, PayloadKind};

        let alice = Identity::generate(0);
        let bob = Identity::generate(0);
        let payload = Payload {
            kind: PayloadKind::Text,
            body: Some("a long-ish message that will be erasure coded".repeat(20)),
            coords: None,
            battery_pct: None,
            attach: None,
            group_id: None,
        };
        let bundle = seal_bundle(&alice, &bob.public(), &payload, &SealOptions::normal(0)).unwrap();
        let (k, m) = (3, 3);
        let frags = fragment_bundle(&bundle, k, m).unwrap();
        assert_eq!(frags.len(), k + m);

        // Deliver only k fragments (indices 1,3,5) — simulate partial escape.
        let mut collector = FragmentCollector::new();
        let mut reconstructed = None;
        for f in [&frags[5], &frags[3], &frags[1]] {
            if let Some(b) = collector.accept(f) {
                reconstructed = Some(b);
            }
        }
        let recon = reconstructed.expect("reconstructs from k fragments");
        assert_eq!(recon.bundle_id, bundle.bundle_id);
        assert_eq!(recon.ciphertext, bundle.ciphertext);

        // And it decrypts back to the original payload.
        let opened = crate::message::open_bundle(&bob, &recon).unwrap();
        assert_eq!(opened.payload, payload);
    }
}
