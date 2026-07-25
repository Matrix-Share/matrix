//! Proof-of-relay (PRD FR-47 incentive note; network-layer §"optional incentive
//! layer"; Helium precedent).
//!
//! Gateways and relays spend battery and bandwidth carrying other people's
//! traffic. An optional incentive layer can reward that — but it must **never
//! sit in the delivery path** (the PRD's explicit lesson from Nakamoto: no
//! global consensus where a disaster destroys connectivity). So this module only
//! produces a *portable, verifiable claim* that can be settled later, when
//! online, entirely off-path.
//!
//! ## Why credits are witnessed, not self-signed
//! Helium's proof-of-location was gamed because a device could attest to its own
//! coverage. Here a relay cannot mint its own evidence: a [`RelayCredit`] is
//! signed by the **next hop** ("I received bundle X from relay R"), so the
//! evidence comes from a counterparty. `verify_proof` rejects self-witnessed
//! credits outright.
//!
//! Residual risk (documented, not solved here): a relay colluding with Sybil
//! witnesses it controls can still manufacture credits. That is bounded by the
//! PoW identity cost (FR-46) and earned reputation (FR-47) — the same defences
//! the PRD assigns to Sybil resistance — and is why settlement should weight
//! credits by witness reputation.

use crate::identity::{address_of, verify_sig, Identity};
use crate::{CoreError, Result};
use lifeline_proto::codec::to_cbor;
use lifeline_proto::{Address, Bytes};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A counterparty's acknowledgement that `relay` forwarded it a bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayCredit {
    /// The relay being credited (who handed us the bundle).
    pub relay: Address,
    /// The node issuing this credit (the next hop).
    pub witness: Address,
    /// The witness's Ed25519 public key.
    pub witness_sign_pub: Bytes,
    pub bundle_id: Bytes,
    pub at: u64,
    pub sig: Bytes,
}

fn credit_signing_bytes(
    relay: &Address,
    witness: &Address,
    bundle_id: &Bytes,
    at: u64,
) -> Result<Vec<u8>> {
    Ok(to_cbor(&(
        crate::domain::RELAY_CREDIT,
        relay,
        witness,
        bundle_id,
        at,
    ))?)
}

/// Issue a credit as the **witness** (the node that received `bundle_id` from
/// `relay`). Only the witness can produce this — the relay cannot self-mint.
pub fn issue_credit(
    witness: &Identity,
    relay: &Address,
    bundle_id: &Bytes,
    at: u64,
) -> Result<RelayCredit> {
    let signing = credit_signing_bytes(relay, witness.address(), bundle_id, at)?;
    Ok(RelayCredit {
        relay: relay.clone(),
        witness: witness.address().clone(),
        witness_sign_pub: Bytes::new(witness.verifying_key().as_bytes().to_vec()),
        bundle_id: bundle_id.clone(),
        at,
        sig: witness.sign(&signing),
    })
}

/// Verify a single credit: the witness key matches its address and signed it.
pub fn verify_credit(credit: &RelayCredit) -> Result<()> {
    if address_of(credit.witness_sign_pub.as_slice())? != credit.witness {
        return Err(CoreError::Log("witness key does not match address".into()));
    }
    if credit.witness == credit.relay {
        return Err(CoreError::Log("self-witnessed credit".into()));
    }
    let signing =
        credit_signing_bytes(&credit.relay, &credit.witness, &credit.bundle_id, credit.at)?;
    verify_sig(
        credit.witness_sign_pub.as_slice(),
        &signing,
        credit.sig.as_slice(),
    )
}

/// A relay's collected evidence of work done, settled off-path when online.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofOfRelay {
    pub relay: Address,
    pub credits: Vec<RelayCredit>,
}

/// What a verified proof actually establishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofSummary {
    /// Credits that verified and belong to this relay.
    pub valid_credits: usize,
    /// Distinct bundles relayed (duplicates collapse).
    pub distinct_bundles: usize,
    /// Distinct counterparties that vouched — the anti-Sybil signal to weight by.
    pub distinct_witnesses: usize,
}

/// Verify a proof-of-relay, ignoring any invalid, foreign, self-witnessed, or
/// duplicate credit. Pure and offline — no ledger, no consensus.
pub fn verify_proof(proof: &ProofOfRelay) -> ProofSummary {
    let mut seen: HashSet<(Bytes, Address)> = HashSet::new();
    let mut bundles: HashSet<Bytes> = HashSet::new();
    let mut witnesses: HashSet<Address> = HashSet::new();
    let mut valid = 0usize;

    for c in &proof.credits {
        if c.relay != proof.relay || verify_credit(c).is_err() {
            continue;
        }
        // One credit per (bundle, witness) pair.
        if !seen.insert((c.bundle_id.clone(), c.witness.clone())) {
            continue;
        }
        valid += 1;
        bundles.insert(c.bundle_id.clone());
        witnesses.insert(c.witness.clone());
    }

    ProofSummary {
        valid_credits: valid,
        distinct_bundles: bundles.len(),
        distinct_witnesses: witnesses.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bid(b: u8) -> Bytes {
        Bytes::new(vec![b; 16])
    }

    #[test]
    fn witnessed_credits_verify_and_summarize() {
        let relay = Identity::generate(0);
        let w1 = Identity::generate(0);
        let w2 = Identity::generate(0);

        let credits = vec![
            issue_credit(&w1, relay.address(), &bid(1), 10).unwrap(),
            issue_credit(&w2, relay.address(), &bid(2), 11).unwrap(),
            issue_credit(&w2, relay.address(), &bid(3), 12).unwrap(),
        ];
        let proof = ProofOfRelay {
            relay: relay.address().clone(),
            credits,
        };
        let s = verify_proof(&proof);
        assert_eq!(s.valid_credits, 3);
        assert_eq!(s.distinct_bundles, 3);
        assert_eq!(s.distinct_witnesses, 2);
    }

    #[test]
    fn relay_cannot_self_witness() {
        let relay = Identity::generate(0);
        // The relay tries to credit itself.
        let forged = issue_credit(&relay, relay.address(), &bid(9), 10).unwrap();
        assert!(verify_credit(&forged).is_err(), "self-witnessing must fail");

        let proof = ProofOfRelay {
            relay: relay.address().clone(),
            credits: vec![forged],
        };
        assert_eq!(verify_proof(&proof).valid_credits, 0);
    }

    #[test]
    fn duplicate_and_tampered_credits_are_ignored() {
        let relay = Identity::generate(0);
        let w = Identity::generate(0);
        let c = issue_credit(&w, relay.address(), &bid(1), 10).unwrap();

        // Same credit twice → counted once.
        let mut tampered = c.clone();
        tampered.bundle_id = bid(7); // signature no longer matches
        let proof = ProofOfRelay {
            relay: relay.address().clone(),
            credits: vec![c.clone(), c.clone(), tampered],
        };
        let s = verify_proof(&proof);
        assert_eq!(s.valid_credits, 1, "duplicates collapse, tampered dropped");
        assert_eq!(s.distinct_bundles, 1);
    }

    #[test]
    fn credits_for_another_relay_are_ignored() {
        let relay = Identity::generate(0);
        let other = Identity::generate(0);
        let w = Identity::generate(0);
        // A credit that names a different relay.
        let c = issue_credit(&w, other.address(), &bid(1), 10).unwrap();
        let proof = ProofOfRelay {
            relay: relay.address().clone(),
            credits: vec![c],
        };
        assert_eq!(verify_proof(&proof).valid_credits, 0);
    }
}
