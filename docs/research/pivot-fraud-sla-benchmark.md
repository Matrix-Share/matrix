# Pivot: an offline-fraud SLA, grounded in a measurement

**What this is.** After a four-lens independent review deflated the "new theorem"
framing (the dynamics are the known *chase-escape* process), we pivoted to what a
conference/approach paper actually rewards: an **empirical result** and a
**benchmark/bridge**. This note reports the first measurement — run, not asserted —
of *when offline double-spend fraud is containable and when it runs away*, and what
the data says works and doesn't. Simulator: `crates/sim/src/containment.rs`
(`cargo run -p lifeline-sim -- containment`), reproducible (seeded RNG).

The sharpening that made it testable: the review's "transferability" axis is
really **single-agent adversary** (a non-transferable token, held by one attacker)
vs. **spreading-prey adversary** (a transferable token that is copied and
re-transferred, so the *set of over-spenders* spreads). We measured both, crossed
with two detection models (**trail-confined** = revocation rides the spend trail;
**flood** = revocation gossips to everyone) and two topologies (**well-mixed** =
complete graph; **2-D lattice** = spatial locality).

---

## The measured data (seed 42; `N_win` = total over-spend before containment)

### M1 — Non-transferable (single agent): **R2 holds. Fraud is bounded.**
`a = d = 1`; expect `E[N_win] ≈ (a/d)·ln N`.

| N | E[N_win] | ln N | ratio |
|---:|---:|---:|---:|
| 100 | 5.13 | 4.61 | 1.11 |
| 1,000 | 7.65 | 6.91 | 1.11 |
| 10,000 | 10.03 | 9.21 | 1.09 |
| 100,000 | 12.60 | 11.51 | 1.09 |

Ratio flat at ~1.05–1.11 (the offset is the harmonic constant `γ`). **`E[N_win] = H_N ≈ (a/d)·ln N` confirmed.** A single attacker double-spending one non-transferable token is inherently bounded — 13 wallets out of 100,000. The current single-device offline-CBDC threat model is right to bound it by a holding limit.

### M2 — Transferable + trail detection + **well-mixed: catastrophic (uncontainable).**
Sweep `ρ = λ_r/λ_b` (ρ<1 ⇒ detection faster). `N = 20,000`.

| ρ | N_win/N |
|---:|---:|
| 0.10 | **0.63** |
| 0.20 | 0.87 |
| 0.40 | 0.98 |
| 1.00 | 1.00 |
| 5.00 | 1.00 |

**Runaway at every ratio — even with detection 10× faster (ρ=0.1 → 63% over-spent).** A single self-seeded detector is diluted `1/N` on a well-mixed graph and cannot catch an exponentially spreading token. **Containment is not a well-mixed phenomenon.** This is the key negative result.

### M3 — Transferable + **FLOOD** detection + well-mixed: **contained iff detection is faster (ρ\* ≈ 1).**

| ρ | N_win/N |
|---:|---:|
| 0.10 | 0.0001 |
| 0.40 | 0.003 |
| 0.80 | 0.13 |
| **1.00** | **0.37** |
| 2.00 | 0.98 |

A sharp transition at **ρ\* ≈ 1** (`s_D = s_A`). Flooding the revocation (immunizing verifiers *ahead* of the token, not just along its trail) makes detection an independent epidemic that outruns the spread — **containment restored, threshold at speed-equality.** Flood gossip is a real lever.

### M4 — Transferable + trail detection + **2-D lattice: contained iff detection ≳ 2× faster (ρ\* ≈ 0.5).**
`160×160 = 25,600` sites.

| ρ | N_win/N |
|---:|---:|
| 0.30 | 0.0003 |
| **0.50** | **0.14** |
| 0.70 | 0.57 |
| 1.00 | 0.75 |
| 2.00 | 0.85 |

A genuine containment transition at **ρ\* ≈ 0.5**. This matches the known chase-escape critical point on ℤ² (`p_c ≈ 0.49`, Kumar–Grassberger–Dhar) — an *independent validation* that our process is chase-escape, exactly as the review argued. **Spatial locality is the second lever:** on a mesh, the detection front keeps local pace with the token even without flooding — but detection must be ~2× faster than the token's re-transfer rate to contain.

---

## What works and what doesn't (the honest bottom line)

| Regime | Result | Verdict |
|---|---|---|
| Non-transferable (single agent) | `N_win ~ ln N`, bounded | **Safe.** Holding limit suffices; no contagion. |
| Transferable, well-mixed, trail-only detection | runaway `→ Θ(N)` at all speeds | **Fails.** Uncontainable — the failure mode to avoid. |
| Transferable, well-mixed, flood detection | contained iff `s_D > s_A` (ρ\*≈1) | **Works** — costs gossip bandwidth. |
| Transferable, spatial mesh, trail detection | contained iff `s_D ≳ 2·s_A` (ρ\*≈0.5) | **Works** — locality does the job, at a stricter speed margin. |

**The design conclusion, in one line:** *a transferable offline token's fraud is containable, but only if the revocation either floods faster than the token spreads (ρ\*≈1) or the network's spatial locality lets a trail-confined front keep pace at a ~2× speed margin (ρ\*≈0.5); making trail-only detection merely "faster" on a well-connected network does nothing.* Non-transferable tokens need none of this.

This is a dimensioned, provisioning-relevant statement — the kind an offline-payment designer can use: *set the revocation-gossip rate to at least the token's re-transfer rate (flood) or ~2× it (rely on mesh locality); a non-transferable design is bounded by its holding limit regardless.*

---

## Reframed contribution (grounded, modest, honest)

- **Not** a new theorem: the dynamics are chase-escape (Kordzakhia; Kortchemski; Kumar–Grassberger–Dhar), and the `ln N` law is textbook epidemic take-off.
- **What we contribute:** (i) the **measurement** — the first crossing of *transferability × detection-model × topology* for offline double-spend, with the two containment thresholds located (ρ\*≈1 flood, ρ\*≈0.5 lattice); (ii) the **bridge** — naming the correspondence *offline over-spend containment ≡ chase-escape*, with revocation rate = detection front speed, and confirming it against the known ℤ² critical point; (iii) a **reusable open harness** (`sim::containment`) others can extend.

## Honest limitations & next steps

- These are **mean-field and lattice** models, not real contact traces. The load-bearing next step (per the empirical review) is **replaying real human contact traces** (MIT Reality Mining, Cambridge Haggle/Infocom, Copenhagen, Cabspotting) and measuring where real offline-payment topologies sit relative to ρ\*, and by how much heavy-tailed human mobility moves the threshold. **Pre-registered falsifier:** if real traces match the idealized ρ\*, there's no measurement contribution beyond the model.
- **Scope condition (load-bearing):** the entire danger is the *transferable* regime. A non-transferable/single-device token is bounded (M1). State transferability as the primary assumption and validate a real target (P2P offline digital-euro) has it.
- The 2-D exponent near ρ\* (finite-size scaling) is a physics companion result, not the headline.
- Claim discipline: **no theorem claims**; cite chase-escape for mechanism; cite ECB/BIS reports for the holding-limit framing.

## Reproduce

```bash
cargo run -p lifeline-sim -- containment      # prints all four measurement tables
cargo test -p lifeline-sim --lib containment  # 5 tests pin the qualitative findings
```
