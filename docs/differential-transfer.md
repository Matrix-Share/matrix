# The differential principle as a transfer-optimization mechanism

"Differential GPS" is one instance of a far more general idea. This document
states it mathematically, validates it against the literature (so we implement
textbook-correct algorithms, not home-grown bugs), and maps it across Lifeline.

## 1. The mathematics

A node needs a quantity **Q** — to *transmit*, *store*, or *estimate*. Pick a
**reference R** that is either already known to the receiver or is a correlated
anchor. The whole trick is to operate on the **residual**

$$\Delta \;=\; Q - \mathbb{E}[\,Q \mid R\,]$$

— the part of Q that is *not* explained by R — and never on Q directly, because
the shared part carries no new information. This shows up in two dual regimes:

**Regime A — differential *coding* (save bits).** If R is known to the receiver,
the minimum cost to convey Q is the **conditional entropy** $H(Q\mid R)\le H(Q)$;
you save the **mutual information** $I(Q;R)$ by sending $\Delta$ instead of Q.
(Slepian–Wolf; predictive/delta coding.)

**Regime B — differential *estimation* (cut variance).** If R is a correlated
anchor, the **control-variate** estimator $\hat Q = Q-\beta(R-\mathbb{E}[R])$ is
unbiased with

$$\mathrm{Var}(\hat Q)=\mathrm{Var}(Q)\,(1-\rho^2),\qquad \beta^\star=\tfrac{\mathrm{Cov}(Q,R)}{\mathrm{Var}(R)}$$

so correlation ρ with the reference cuts variance by ρ². **DGPS is exactly this
with β = 1**, R being the reference's measurement of the common-mode error. The
time/geo work is the estimation corner; for a bandwidth-starved DTN, **Regime A is
the bigger prize.**

The set-valued case — two nodes doing "knowledge transfer" — is **set
reconciliation**: convey the symmetric difference $A\triangle B$ with cost
$O(|A\triangle B|)$, proportional to *what differs*, not to what either holds.

## 2. Validity & inversion test (literature)

Set reconciliation is textbook, with a 20-year lineage: Minsky–Trachtenberg–Zippel
2003 (characteristic-polynomial / CPISync), Eppstein–Goodrich–Uyeda–Varghese 2011
("What's the Difference?", IBLT), Meyer 2023 (Range-Based Set Reconciliation).
Deployed in Dynamo/Cassandra anti-entropy (Merkle trees), Bitcoin (minisketch),
and **Nostr NIP-77 (negentropy)** — which Lifeline already integrates with.

**The inversion — where each variant creates bugs:**

| Algorithm | Failure mode | Verdict for an emergency messenger |
|---|---|---|
| **IBLT** | *Probabilistic* — mis-sized vs true \|Δ\| can **silently omit** items | ❌ silent loss = a dropped SOS |
| **CPISync** | Needs an a-priori bound on \|Δ\|; heavier compute | ⚠️ fragile |
| **Range-based** | *Deterministic* — mismatching ranges always recurse to explicit id lists; only a **fingerprint collision** can miss items | ✅ safe with a crypto fingerprint |

**Choice: range-based, with a BLAKE3 fingerprint over the sorted ids.** Rationale:
deterministic (no silent omission), degrades to O(n) when sets are disjoint (never
worse than today's re-offer), and matches the proven Nostr choice. Three
Lifeline-specific inversions checked: a malicious peer can only make *its own* sync
incomplete (content stays E2E-authenticated → **no new integrity attack surface**);
worst case is no regression; it needs only a total order on `bundle_id`.

## 3. Where the differential pattern applies in Lifeline

| Feature | Q | Reference R | Δ | Regime | Value |
|---|---|---|---|---|---|
| **Bundle anti-entropy at contact** | "which bundles do I hold" | peer's set | $A\triangle B$ | **A (set-recon)** | **DTN core; fixes the dead `PeerInfo.known`** |
| **CRDT state sync** | full `SharedState` | peer's version vector | delta-ops since VV | **A** | fixes full-state resend (audit red flag) |
| **Reputation gossip** | full score map | last snapshot / anchor | changed scores + anchor correction | A+B | also cures defamation |
| **Beacons / announces** | re-broadcast state | last acked beacon | changed fields | A | medium |
| **Message bodies** | text / SOS updates | thread history or a disaster-phrase codebook | dictionary-coded residual | A | high on tiny bearers |
| **Header scalars** | `created_at`/`ttl`/`epoch` | a session base | offsets | A | small |
| **Time / position / congestion / sensing** | clock / coords / load / reading | GPS anchor / gateway / known conditions | control-variate correction | B | time ✅, rest roadmap |

The through-line: **condition every transfer on the mutual information the two
nodes already share.** For Lifeline, whose binding constraint is bytes over lossy,
tiny-MTU bearers, Regime-A is the highest-leverage optimization in the system.

## 4. Build order

1. **`lifeline-reconcile` (this PR).** Range-based set reconciliation with a
   BLAKE3 fingerprint — the safe variant from §2. Pure, tested library
   (converges to the exact symmetric difference; cost ∝ difference; graceful when
   disjoint). Then wire it into the router's contact-time bundle exchange
   (retiring the dead `PeerInfo.known`).
2. **Delta-state CRDT sync** — `delta_since(peer_vv)` instead of full `SharedState`.
3. **Shared disaster-phrase dictionary** — predictive coding of message bodies
   (biggest per-byte win on ultrasound-class bearers).
4. **Differential reputation / positioning / congestion** — the Regime-B set.
