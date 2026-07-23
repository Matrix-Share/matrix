//! Append-only, hash-linked per-identity log (PRD L5, FR-30, §11.7).
//!
//! Each identity keeps a log; every sent bundle, received bundle, and returned
//! receipt appends one entry. Entries chain by `prev = blake3(prev_entry)` and
//! are individually signed, so the log is **tamper-evident**: you cannot alter
//! or reorder history without invalidating a signature or a link (SSB-style,
//! Tarr 2019). This is the substrate the sender uses to match returned receipts
//! against sent messages and establish verifiable delivery (FR-32).

use crate::identity::{verify_sig, Identity};
use crate::{CoreError, Result};
use lifeline_proto::{codec, Address, Bytes, LogEntry, LogEvent};
use serde::{Deserialize, Serialize};

/// Canonical, signature-excluded view of an entry (what gets signed & hashed
/// for the `prev` link).
#[derive(Serialize)]
struct EntryCore<'a> {
    seq: u64,
    prev: &'a Bytes,
    event: LogEvent,
    ref_id: &'a Bytes,
    at: u64,
}

fn entry_signing_bytes(e: &LogEntry) -> Result<Vec<u8>> {
    let core = EntryCore {
        seq: e.seq,
        prev: &e.prev,
        event: e.event,
        ref_id: &e.ref_id,
        at: e.at,
    };
    Ok(codec::to_cbor(&core)?)
}

/// A signed commitment to pruned log history (SSB-style compaction).
///
/// The hash-linked log grows without bound (Tarr 2019, "append-only logs grow
/// forever"). Because the log is **single-writer**, its owner can safely prune
/// old entries locally and keep this checkpoint instead: it records how many
/// entries were dropped and the hash of the last dropped entry, signed. The
/// first retained entry still links (`prev == tip_hash`), so the chain remains
/// verifiable from the checkpoint forward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Sequence number of the last pruned entry.
    pub up_to_seq: u64,
    /// Cumulative count of entries pruned across all checkpoints.
    pub count: u64,
    /// Hash of the last pruned entry (the link the first retained entry points to).
    pub tip_hash: Bytes,
    /// Author signature over `(up_to_seq, count, tip_hash)`.
    pub sig: Bytes,
}

fn checkpoint_signing_bytes(up_to_seq: u64, count: u64, tip_hash: &Bytes) -> Result<Vec<u8>> {
    Ok(codec::to_cbor(&(up_to_seq, count, tip_hash))?)
}

/// An append-only hash-linked log owned by one identity, with optional
/// compaction.
#[derive(Debug, Clone)]
pub struct HashLog {
    author: Address,
    entries: Vec<LogEntry>,
    /// Present once the log has been compacted at least once.
    checkpoint: Option<Checkpoint>,
}

impl HashLog {
    /// Create an empty log for `author`.
    pub fn new(author: Address) -> Self {
        HashLog {
            author,
            entries: Vec::new(),
            checkpoint: None,
        }
    }

    /// The current checkpoint, if the log has been compacted.
    pub fn checkpoint_ref(&self) -> Option<&Checkpoint> {
        self.checkpoint.as_ref()
    }

    /// Next sequence number to assign, accounting for pruned history.
    fn next_seq(&self) -> u64 {
        if let Some(e) = self.entries.last() {
            e.seq + 1
        } else if let Some(c) = &self.checkpoint {
            c.up_to_seq + 1
        } else {
            0
        }
    }

    /// Compact the log, pruning all but the most recent `keep_recent` entries
    /// into a signed [`Checkpoint`]. Safe because the log is single-writer.
    /// Returns `true` if anything was pruned. `verify_chain` still passes after
    /// this; only entries older than the window become unqueryable by
    /// [`HashLog::find_by_ref`].
    pub fn checkpoint(&mut self, identity: &Identity, keep_recent: usize) -> Result<bool> {
        debug_assert_eq!(identity.address(), &self.author, "log author mismatch");
        if self.entries.len() <= keep_recent {
            return Ok(false);
        }
        let cut = self.entries.len() - keep_recent;
        let last_pruned = &self.entries[cut - 1];
        let tip_hash = Bytes::new(crate::crypto::blake3_32(&codec::to_cbor(last_pruned)?).to_vec());
        let up_to_seq = last_pruned.seq;
        let prev_count = self.checkpoint.as_ref().map(|c| c.count).unwrap_or(0);
        let count = prev_count + cut as u64;
        let signing = checkpoint_signing_bytes(up_to_seq, count, &tip_hash)?;
        let sig = identity.sign(&signing);
        self.entries.drain(0..cut);
        self.checkpoint = Some(Checkpoint {
            up_to_seq,
            count,
            tip_hash,
            sig,
        });
        Ok(true)
    }

    pub fn author(&self) -> &Address {
        &self.author
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    /// Latest sequence number, or `None` if empty. Used as the "frontier"
    /// exchanged during anti-entropy sync (§12.3).
    pub fn frontier(&self) -> Option<u64> {
        self.entries.last().map(|e| e.seq)
    }

    /// Hash of the last entry (the link the next entry points back to). Falls
    /// back to the checkpoint tip when all in-memory entries were pruned.
    fn tip_hash(&self) -> Bytes {
        match self.entries.last() {
            Some(e) => {
                let bytes = codec::to_cbor(e).expect("entry re-encode");
                Bytes::new(crate::crypto::blake3_32(&bytes).to_vec())
            }
            None => match &self.checkpoint {
                Some(c) => c.tip_hash.clone(),
                None => Bytes::new(Vec::new()), // genesis has empty prev
            },
        }
    }

    /// Append a signed entry recording `event` about bundle `ref_id`.
    pub fn append(
        &mut self,
        identity: &Identity,
        event: LogEvent,
        ref_id: &Bytes,
        at: u64,
    ) -> Result<&LogEntry> {
        debug_assert_eq!(identity.address(), &self.author, "log author mismatch");
        let seq = self.next_seq();
        let prev = self.tip_hash();
        let mut entry = LogEntry {
            seq,
            prev,
            event,
            ref_id: ref_id.clone(),
            at,
            sig: Bytes::default(),
        };
        let signing = entry_signing_bytes(&entry)?;
        entry.sig = identity.sign(&signing);
        self.entries.push(entry);
        Ok(self.entries.last().unwrap())
    }

    /// Verify the whole chain against `author_sign_pub`: every signature valid
    /// and every `prev` link correct. If the log was compacted, the checkpoint
    /// signature is verified and the chain is validated from it forward.
    pub fn verify_chain(&self, author_sign_pub: &[u8]) -> Result<()> {
        // Establish the starting link/seq from the checkpoint, if any.
        let (mut expected_prev, mut expected_seq) = match &self.checkpoint {
            Some(c) => {
                let signing = checkpoint_signing_bytes(c.up_to_seq, c.count, &c.tip_hash)?;
                verify_sig(author_sign_pub, &signing, c.sig.as_slice())
                    .map_err(|_| CoreError::Log("bad checkpoint signature".into()))?;
                (c.tip_hash.clone(), c.up_to_seq + 1)
            }
            None => (Bytes::new(Vec::new()), 0u64),
        };
        for e in &self.entries {
            if e.seq != expected_seq {
                return Err(CoreError::Log(format!(
                    "seq gap: expected {expected_seq}, got {}",
                    e.seq
                )));
            }
            if e.prev != expected_prev {
                return Err(CoreError::Log(format!("broken hash link at seq {}", e.seq)));
            }
            let signing = entry_signing_bytes(e)?;
            verify_sig(author_sign_pub, &signing, e.sig.as_slice())
                .map_err(|_| CoreError::Log(format!("bad signature at seq {}", e.seq)))?;
            // Next expected prev = hash of this full entry (including its sig).
            let bytes = codec::to_cbor(e)?;
            expected_prev = Bytes::new(crate::crypto::blake3_32(&bytes).to_vec());
            expected_seq += 1;
        }
        Ok(())
    }

    /// Find the log entry that recorded sending/receiving a given bundle id
    /// (used to match a returned receipt to a sent message, FR-32).
    pub fn find_by_ref(&self, ref_id: &Bytes) -> Option<&LogEntry> {
        self.entries.iter().find(|e| &e.ref_id == ref_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_verify_chain() {
        let id = Identity::generate(1000);
        let mut log = HashLog::new(id.address().clone());
        for i in 0..5u8 {
            let ref_id = Bytes::new(vec![i; 16]);
            log.append(&id, LogEvent::Sent, &ref_id, 1000 + i as u64)
                .unwrap();
        }
        assert_eq!(log.len(), 5);
        assert_eq!(log.frontier(), Some(4));
        log.verify_chain(id.verifying_key().as_bytes()).unwrap();
    }

    #[test]
    fn tamper_breaks_chain() {
        let id = Identity::generate(1000);
        let mut log = HashLog::new(id.address().clone());
        log.append(&id, LogEvent::Sent, &Bytes::new(vec![1; 16]), 1000)
            .unwrap();
        log.append(&id, LogEvent::Sent, &Bytes::new(vec![2; 16]), 1001)
            .unwrap();
        // Mutate a recorded event after the fact.
        log.entries[0].at = 9999;
        assert!(log.verify_chain(id.verifying_key().as_bytes()).is_err());
    }

    #[test]
    fn checkpoint_compacts_and_still_verifies() {
        let id = Identity::generate(1000);
        let mut log = HashLog::new(id.address().clone());
        for i in 0..20u8 {
            log.append(
                &id,
                LogEvent::Sent,
                &Bytes::new(vec![i; 16]),
                1000 + i as u64,
            )
            .unwrap();
        }
        // Compact, keeping only the last 5 entries.
        assert!(log.checkpoint(&id, 5).unwrap());
        assert_eq!(log.len(), 5);
        assert_eq!(log.checkpoint_ref().unwrap().count, 15);
        // Chain still verifies from the checkpoint forward.
        log.verify_chain(id.verifying_key().as_bytes()).unwrap();
        // Frontier/seq continuity preserved; appending continues correctly.
        assert_eq!(log.frontier(), Some(19));
        log.append(&id, LogEvent::Sent, &Bytes::new(vec![99; 16]), 2000)
            .unwrap();
        assert_eq!(log.frontier(), Some(20));
        log.verify_chain(id.verifying_key().as_bytes()).unwrap();
    }

    #[test]
    fn tampered_checkpoint_fails_verification() {
        let id = Identity::generate(1000);
        let mut log = HashLog::new(id.address().clone());
        for i in 0..10u8 {
            log.append(
                &id,
                LogEvent::Sent,
                &Bytes::new(vec![i; 16]),
                1000 + i as u64,
            )
            .unwrap();
        }
        log.checkpoint(&id, 3).unwrap();
        // Forge the pruned count.
        let mut cp = log.checkpoint_ref().unwrap().clone();
        cp.count = 0;
        log.checkpoint = Some(cp);
        assert!(log.verify_chain(id.verifying_key().as_bytes()).is_err());
    }

    #[test]
    fn find_by_ref_works() {
        let id = Identity::generate(1000);
        let mut log = HashLog::new(id.address().clone());
        let target = Bytes::new(vec![42; 16]);
        log.append(&id, LogEvent::Sent, &Bytes::new(vec![1; 16]), 1000)
            .unwrap();
        log.append(&id, LogEvent::Sent, &target, 1001).unwrap();
        assert_eq!(log.find_by_ref(&target).unwrap().seq, 1);
        assert!(log.find_by_ref(&Bytes::new(vec![7; 16])).is_none());
    }
}
