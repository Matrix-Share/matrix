# Containing offline capability over-use on the Lifeline mesh — a measurement

**What this is.** Lifeline lets a partitioned mesh keep messaging when the towers
are down, but **internet egress is a scarce, node-gated resource**: a node reaches
the internet only through a gateway it is authorized for, and that authorization is
an *offline-verifiable bearer capability* (`lifeline-inet` / ServiceClass /
QuotaLedger). This note measures the security question that follows directly from
that design: **when a gateway capability is metered offline across a partition, how
badly can it be over-used before the mesh reconciles and revokes it — and what
actually stops the over-use?**

The observation that started this: metering an offline bearer capability across a
network split *is* offline double-spending [ChaumFiatNaor1988]. Under a partition
no verifier can see the full spend history [GilbertLynch2002, Rahman2017PCAP], so a
holder can present the same capability at many gateways at once. The question is not
*whether* over-use happens — it is *how much*, and *what design choice bounds it*.
We ran the experiment instead of asserting the answer: `crates/sim/src/containment.rs`
(`cargo run -p lifeline-sim -- containment`), reproducible, seeded.

The axis that makes it testable: a **single-holder capability** (bound to one
device, non-transferable) vs. a **delegatable capability** (attenuated and
re-delegated hop-to-hop, so the *set of over-users spreads* like a contagion) —
crossed with how revocation propagates (**trail** = revocation rides the spend path;
**flood** = revocation gossips mesh-wide) and topology (**well-mixed** vs.
**2-D mesh** with spatial locality). Lifeline is the 2-D-mesh, trail-revocation case
— so that row is the one that matters, and we can now say what it does.

---

## The measured data (seed 42; `N_win` = total over-use before containment)

### M1 — Single-holder capability: **over-use is bounded. This is the safe design.**
`a = d = 1`; expect `E[N_win] ≈ (a/d)·ln N` (epidemic take-off × accrual [Ganesh2005Epidemics]).

| N (verifiers/gateways) | E[N_win] | ln N | ratio |
|---:|---:|---:|---:|
| 100 | 5.13 | 4.61 | 1.11 |
| 1,000 | 7.65 | 6.91 | 1.11 |
| 10,000 | 10.03 | 9.21 | 1.09 |
| 100,000 | 12.60 | 11.51 | 1.09 |

Ratio flat at ~1.09. **`E[N_win] ≈ (a/d)·ln N` confirmed.** A non-transferable gateway
capability, even abused hard, over-uses only *logarithmically* in mesh size — 13
excess egress grants across a 100,000-node mesh. The bounded-counter / escrow
tradition [ONeil1986Escrow, Barbara1994Demarcation, Balegas2015BoundedCounter,
Roy2015Homeostasis] already keeps a numeric cap safe under partition *when the
holder can't spread it*; M1 is that regime, and it is benign.

### M2 — Delegatable + trail revocation + **well-mixed: runaway (uncontainable).**
Sweep `ρ = λ_spread / λ_revoke` (ρ<1 ⇒ revocation faster). `N = 20,000`.

| ρ | N_win/N |
|---:|---:|
| 0.10 | **0.63** |
| 0.20 | 0.87 |
| 0.40 | 0.98 |
| 1.00 | 1.00 |
| 5.00 | 1.00 |

**Runaway at every ratio — even with revocation 10× faster (ρ=0.1 → 63% of the mesh
over-uses).** A single self-seeded revocation is diluted `1/N` on a well-connected
graph and can't catch a delegatable capability spreading exponentially. **The
takeaway for Lifeline: never let a delegatable capability propagate on a
well-mixed overlay.** This is the failure mode the design must avoid.

### M3 — Delegatable + **flood** revocation + well-mixed: **contained iff revocation outruns spread (ρ\*≈1).**

| ρ | N_win/N |
|---:|---:|
| 0.10 | 0.0001 |
| 0.40 | 0.003 |
| 0.80 | 0.13 |
| **1.00** | **0.37** |
| 2.00 | 0.98 |

Sharp transition at **ρ\*≈1** (revocation speed = spread speed). Flooding the
revocation ahead of the capability — immunizing verifiers before the token reaches
them, like an anti-entropy gossip sweep [Ganesh2005Epidemics] — turns detection into
its own epidemic that outruns the abuse. **Lever 1: flood revocation at ≥ the
delegation rate.** Costs mesh bandwidth (the thing Lifeline is careful with).

### M4 — Delegatable + trail revocation + **2-D mesh: contained iff revocation ≳ 2× faster (ρ\*≈0.5).** *(This is Lifeline.)*
`160×160 = 25,600` sites.

| ρ | N_win/N |
|---:|---:|
| 0.30 | 0.0003 |
| **0.50** | **0.14** |
| 0.70 | 0.57 |
| 1.00 | 0.75 |
| 2.00 | 0.85 |

A genuine containment transition at **ρ\*≈0.5**. This matches the known
chase-escape critical point on ℤ² (`p_c ≈ 0.49` [Kumar2021chaseescape]) — an
*independent validation* that the over-use/revocation race is exactly the
chase-escape process [Kordzakhia2005escape, Kortchemski2015predator]. **On a real
mesh, spatial locality does the containment work for free:** the revocation front
keeps local pace with the spreading capability without any global flood — but the
mesh must gossip revocation ~2× faster than a capability is re-delegated.

---

## What this means for Lifeline (the honest bottom line)

| Design choice | Result | Verdict for Lifeline |
|---|---|---|
| Single-holder (non-transferable) gateway capability | `N_win ~ ln N`, bounded | ✅ **Safe by construction.** Prefer this whenever egress needn't be delegated. |
| Delegatable capability, well-mixed overlay, trail revocation | runaway `→ Θ(N)` | ❌ **Forbidden.** Never flood delegatable egress rights on a well-mixed overlay. |
| Delegatable, well-mixed, **flood** revocation | contained iff `s_revoke > s_spread` (ρ\*≈1) | ✅ **Works, at a bandwidth cost.** |
| **Delegatable, 2-D mesh, trail revocation** *(Lifeline's native case)* | contained iff `s_revoke ≳ 2·s_spread` (ρ\*≈0.5) | ✅ **Works.** Locality contains it; **provision revocation gossip at ≥ ~2× the delegation rate.** |

**The provisioning rule Lifeline can act on:** *prefer non-transferable gateway
capabilities (bounded by ln N regardless). If a capability must be delegatable,
confine revocation to the spend path and rely on mesh locality — but gossip
revocation at least ~2× as fast as capabilities are re-delegated (ρ\*≈0.5); a
well-mixed overlay breaks this and needs a full flood (ρ\*≈1) instead.* This is a
dimensioned setting for `QuotaLedger` / the revocation gossip, not a vibe.

---

## Contribution (grounded, modest, honest)

- **Not** a new theorem: the over-use/revocation race is the known chase-escape
  process [Kordzakhia2005escape, Kortchemski2015predator, Kumar2021chaseescape,
  Tang2018phase], and the `ln N` bound is textbook epidemic take-off
  [Ganesh2005Epidemics]. The single-holder case is classical offline
  detect-not-prevent [ChaumFiatNaor1988] over bounded counters under partition
  [ONeil1986Escrow, Balegas2015BoundedCounter].
- **What we contribute for Lifeline:** (i) the **measurement** — over-use of an
  offline gateway capability, resolved across *delegatability × revocation-model ×
  topology*, with the two containment thresholds located for the mesh (ρ\*≈0.5
  local, ρ\*≈1 flood); (ii) the **provisioning rule** above, wired to real knobs
  (`lifeline-inet` capability delegation, revocation gossip rate); (iii) a **reusable
  seeded harness** (`sim::containment`) that turns any future capability-egress
  design change into a measurable containment number.

## Honest limitations & next steps

- These are **mean-field and lattice** models, not real mesh traces. The load-bearing
  next step is replaying **real human contact traces** (MIT Reality Mining, Cambridge
  Haggle/Infocom, Copenhagen, Cabspotting — DTN-standard datasets [Shevade2008DTN])
  through the same harness and measuring where a real disaster-mesh sits relative to
  ρ\*, and how much heavy-tailed human contact moves the threshold. **Pre-registered
  falsifier:** if real traces sit on the idealized ρ\*, the model already answers it
  and there is no extra measurement contribution.
- **Scope condition (load-bearing):** the whole danger is *delegatable* capabilities.
  A single-holder capability is bounded (M1). Lifeline should treat delegation of
  egress rights as the feature that triggers this analysis, and default to
  non-transferable where it can.
- **Generalization (a footnote, not the frame):** the same offline-bearer-capability
  dynamics recur wherever a metered token is spent under partition — transferable
  offline payments and offline-CBDC holding limits [ChaumPedersen1992, Kempen2024offline,
  Senn2026sok] are the best-studied external instance. That is where the mechanism
  *generalizes*; it is not what Lifeline is for.
- Claim discipline: **no theorem claims** — cite chase-escape for the mechanism, the
  escrow/offline-detection tradition for the single-holder bound.

## Reproduce

```bash
cargo run -p lifeline-sim -- containment      # prints all four measurement tables
cargo test -p lifeline-sim --lib containment  # 5 tests pin the qualitative findings
```

## References

Keys resolve against [`containment.bib`](containment.bib).

- **Over-use / revocation dynamics (chase-escape, epidemics):** Kordzakhia2005escape,
  Kortchemski2015predator, Kumar2021chaseescape, Tang2018phase, Durrett2020coexistence,
  Ganesh2005Epidemics; competing-growth context HaggstromPemantle1998, Antunovic2017competing,
  Deijfen2016winner.
- **Offline capability metering under partition:** ChaumFiatNaor1988 (detect-not-prevent),
  ONeil1986Escrow, Barbara1994Demarcation, Balegas2015BoundedCounter, Roy2015Homeostasis
  (bounded counters under partition), GilbertLynch2002, Rahman2017PCAP (partition/consistency limits).
- **Mesh / DTN evaluation setting:** Shevade2008DTN (DTN routing + real contact-trace datasets).
- **External generalization (offline payments/CBDC):** ChaumPedersen1992, Kempen2024offline, Senn2026sok.
