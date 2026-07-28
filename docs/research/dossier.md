# Research dossier — offline scarcity & adversarial bearer-token containment

**Status:** working research, theory phase. Self-contained consolidation of
everything established so far, written so an *independent* reader (who has not
followed the development) can evaluate, attack, and extend it. Companion formal
paper: [`bearer-token-containment.tex`](bearer-token-containment.tex); references:
[`containment.bib`](containment.bib). Parent system: Project Lifeline, an
offline-first DTN mesh messenger (`../../ARCHITECTURE.md`).

Each result carries a **status tag**: `PROVEN` (rigorous), `EXACT-FLUID`
(exact in a deterministic fluid/mean-field model), `HEURISTIC` (independence or
scaling approximation), `CORRECTED` (an earlier error found and fixed),
`OPEN` (conjecture / unproven).

---

## 1. Origin (one paragraph)

Building an internet-egress gateway for a disaster mesh, we needed to meter a
scarce resource (bytes of egress through a node with real internet) using an
authorization token that works **offline** (no reachable policy server) because
the network is partitioned. This forced the question of what a usage *quota* can
guarantee when the token is presented offline across partitions. The answer
turned out to generalize.

---

## 2. The core problem

Escrow / demarcation / bounded-counter CRDTs already maintain a global numeric
invariant ("total spent ≤ B") under partition **without coordination**, by
pre-splitting the budget among **cooperative, known** sites. A disaster-mesh
capability violates all their assumptions:

| Escrow assumes | Mesh reality |
|---|---|
| sites known & fixed | verifiers (gateways) appear/vanish; unknown set |
| system owns & splits the budget | a **bearer token** is held & moved by the spender |
| sites cooperative | spender **adversarial**; verifiers mutually unreachable |

> **Problem.** Maintain a global numeric invariant `Σ spend ≤ B` under partition
> when the metered object is a transferable **bearer** token, the spender is
> **adversarial** and picks where to present it, and the verifier set is
> **unknown & mobile**, with verifiers deciding **offline** (token + local state
> only).

No prior mechanism (to our knowledge, and per a verified survey — §7) addresses
this regime. This dossier characterizes what is achievable.

---

## 3. Formal model

- **Verifiers** (gateways): `N` on a contact graph `G`, or a Poisson field of
  intensity `ρ_g` on ℝ/ℝ². Each admits up to `B` locally (a per-verifier escrow),
  then refuses.
- **Adversary**: a single mobile holder of a bearer capability `κ` (budget `B`).
  Reaches fresh verifiers at rate `a` (well-mixed) or with front speed
  `s_A` (spatial); `a ≍ ρ_g r v` (coverage of a range-`r` sweep at speed `v`),
  `s_A = v`. Each fresh, un-immunized verifier admits one unit of over-spend.
- **Detection (self-seeding)**: every over-spend emits a signed spend-receipt;
  receipts gossip; a verifier that sees evidence of `>B` becomes **immunized**
  (refuses `κ`, and — if issuance binds identity, à la Chaum–Fiat–Naor — attributes
  & revokes). Gossip rate `d` (well-mixed) or front speed `s_D` (spatial).
- **Quantity of interest**: `N_win` = number of verifiers the adversary over-spends
  at **before** they are immunized. Realized over-spend = `B · N_win`.

---

## 4. Results

### R1 — Static inflation `[PROVEN]`
Under a partition into `k` components (no inter-component comms during spend), an
adversary in `j ≤ k` components realizes up to `jB` **if verifiers within a
component coordinate** (cap the component at `B`); with **no coordination at all**
(pure offline, even intra-component) the bound is `(#verifiers reached) · B`. The
tight offline guarantee is a per-verifier cap of `B`; per-component `B` needs
reachable intra-component coordination; global `B` is unachievable. *This is the
CAP corner for a conserved scalar (escrow's `B/g` is the classical escape). It is
the setup, not the contribution.* The **dynamic model below is the
no-coordination regime**: `N_win` counts verifiers; detection is the only cap.

### R2 — Mean-field over-spend `[EXACT-FLUID]`
Complete graph, well-mixed, `a = O(1)` fixed, `N → ∞`:
```
E[N_win] = (a/d) · ln N + O(1).
```
The integral `∫ s dt = (1/d) ln N` is **exact** (pure-gossip logistic), so the
constant is exactly `a/d` and the closed form is a slight **over-estimate** (safe).
Predictions: over-spend scales with the speed ratio `a/d`, and **grows as ln N**
(a larger network is more abusable per token — detection takes logarithmically
longer to ignite). **Validity boundaries** (from limit-testing): `N_win ≤ N`, so
valid only when `(a/d)ln N ≪ N` (holds for `a=O(1)`; for `a ∝ N` it saturates at
`min(N,·)`); and `N_win ≥ 1`, so the operative form is
`max(1, min(N, (a/d)ln N))`.

### R3 — Collision-seeding constant `[HEURISTIC]`
If detection is **collision-seeded** (a node needs evidence of two spends, which
must first meet via gossip), a birthday argument gives ignition delay
`t0 ≈ (1/2d) ln N`, so `E[N_win] ≈ (3/2)(a/d) ln N`. Thus the **`ln N` law is
universal; the constant ∈ {1, 3/2}** (issuer- vs collision-seeded) — a measurable
signature of the detection model. **Validity**: uses an independence approximation
(heuristic), and is the gossip-limited branch, valid for `a ≫ d/ln N`; a slow
adversary (`a ≲ d/ln N`) is spend-limited, `t0 ≈ 2/a`.

### R4 — Critical divergence (the sharpest result) `[EXACT-FLUID]`
1-D fluid model with ignition delay `t0`. Verifier at `x` over-spent iff
`|x|(1/s_A − 1/s_D) < t0`. Hence:
```
N_win = ∞                              if s_A ≥ s_D
N_win = 2 ρ_g s_A s_D t0 / (s_D − s_A)  if s_D > s_A
```
As `s_D ↓ s_A`: `N_win ∼ (s_D − s_A)^(−1)`. **The containment transition is
continuous, with critical exponent γ = 1 in the mean-field/1-D class.** Passes
every limit test: `t0→0 ⇒ 0` (issuer seeding, perfect containment);
`s_D→∞ ⇒ 2ρ_g s_A t0` (the head-start region — you can't un-spend the ignition
delay); `s_A→0 ⇒ 0`; monotone in all three; dimensions consistent.
**Finite-size**: in a domain of `L` verifiers, `N_win = min(L, ·)` — the divergence
is cut off at system size (finite-size scaling; extrapolate γ as `L→∞`, never read
at criticality).

### R5 — Spatial dichotomy + geometry-dependence `[PROVEN line/tree; OPEN plane]`
As a two-type first-passage percolation (adversary vs detection front): `N_win`
a.s. finite **iff** `s_D > s_A`. Proven on the line and `b`-ary trees. **On `ℤ²` it
coincides with the known-open competing-FPP strangulation problem** (Häggström–
Pemantle: equal speeds coexist w.p. > 0; strict-gap strangulation open in general).
**New finding from re-deriving on a tree**: the over-spent region holds `~b^{ℓ*}`
vertices with `ℓ* = t0 s_A s_D/(s_D−s_A)`, so `N_win ∼ b^{c/(s_D−s_A)}` — an
**essential singularity, not a power law**. So the critical exponent is
**geometry-dependent** (line: power-law `γ=1`; tree: exponential), which is exactly
why the planar/mesh exponent is a genuine open question, not a foregone 1.

### R6 — Conservation–bandwidth lower bound `[PROVEN (mean-field); CORRECTED]`
```
mean-field:  T · m ≥ a · ln N          (m = per-node gossip rate; d = m)
spatial:     m = Ω(v/ℓ)   (pulled front, s_D ≍ ℓ m)
             m = Ω(v²/D)  (diffusive/Fisher–KPP front, s_D ≍ √(D m), D ≍ ℓ²β)
```
Over-spend × detection-bandwidth is bounded below by a network constant:
conservation costs gossip traffic (a hard constraint on a battery-limited mesh).
**Correction note**: an earlier draft wrote `T·m ≥ (a ln N)/β` and `m = Ω(v/β)`,
`Ω(v²/βD)` — all **dimensionally inconsistent**; the forms above are the corrected,
unit-checked versions. Analogue, on a spatial/dynamical axis, of Chaum–Pedersen's
"transferred cash grows in size."

---

## 5. The three doors (design-space taxonomy)

Problem §2 has exactly three escapes:

1. **Sacrifice availability** — escrow/demarcation/bounded-counter (pre-split
   `B/g`): conserves but strands a lone component to `B/g`; needs cooperative known
   sites → **fails the Problem**.
2. **Sacrifice offline / accept latency** — detect-and-attribute on merge
   (CFN e-cash; blockchain confirmations): over-spend up to `N_win`; latency = merge
   time; **this research quantifies `N_win`**.
3. **Externalize to physics** — proof-of-work/stake/location: bounds **creation**
   (Sybil), **not use** (double-spend).

**Sharp refinement:** physics conserves *creation, not use*. PoW bounds how many
tokens/identities exist (Sybil resistance) but not spending one twice — usage
conservation is irreducibly door 1 or 2. Blockchain = door 2 *using* door 3 for its
order; confirmation depth = detection latency (Pass–Seeman–Shelat: security degrades
with delay Δ). Our mesh = door 2 **without a global order**, so containment is
spatial strangulation (R4/R5), not chain depth — a regime between the e-cash and
blockchain literatures.

---

## 6. The three broader observations (the original framing)

These motivated the containment work and remain the honest "what's new" of the
parent white paper:

- **O1 (double-spend):** egress metering with offline bearer capabilities *is*
  offline double-spending → global budget unenforceable; per-component achievable;
  detect-on-merge. (Formalized here as R1–R6.)
- **O2 (privacy–accountability alignment):** sealed-sender delivery forces
  reputation to be *source-local*, which removes the shared state a Sybil would
  badmouth. Confidentiality and reputation-robustness align rather than trade off.
  Quantitative form: manipulation capacity `M(c) = 0` for source-local vs `Θ(c)` for
  gossip-based; the reputation-sharing level `α=0` is optimal iff Sybil fraction
  `f ≥ f*` (robust-estimation breakdown point). *Not yet formalized in a paper.*
- **O3 (open relay / gated egress):** relaying is an un-metered commons (gating a
  message defeats life-safety), egress is a gated scarce resource; the gate must be
  offline-verifiable, whence O1. The commons is the **only** part immune to the
  conservation impossibility.

---

## 7. Prior-art map (what's owned vs what's new)

A verified literature survey established that most of the *framing* is prior art
(all cited in `containment.bib`):

- Quantitative/continuous CAP → **PCAP** (Rahman 2017), PBS (Bailis 2012).
- Numeric invariant under partition via pre-split local budgets → **escrow**
  (O'Neil 1986), **demarcation** (Barbará–Garcia-Molina 1994), **bounded-counter
  CRDT** (Balegas 2015), **homeostasis** (Roy 2015).
- Offline ⇒ detect-not-prevent double-spend → **Chaum–Fiat–Naor** 1988.
- Transferable offline cash has unavoidable inflation → **Chaum–Pedersen 1992**
  ("Transferred Cash Grows in Size") — an uncanny structural twin (representation
  size per transfer; ours is over-spend across space).
- Security degrades with delay / confirmation depth → **Pass–Seeman–Shelat 2017**,
  Rosenfeld 2014, Garay–Kiayias–Leonardos 2015.
- Sybil needs externalized physical/economic scarcity → **Douceur 2002** + PoW/PoS/PoP.
- Competing growth / two-type FPP → **Häggström–Pemantle 1998**.
- Epidemic thresholds → **Ganesh–Massoulié–Towsley 2005**.
- Robust aggregation breakdown → **Donoho–Huber 1983**.

**Surviving contribution (narrow, honest):** (i) the **adversarial bearer-token
regime** (outside escrow's cooperative-site model) as a stated problem; (ii) its
**competing-growth dynamics** — the mean-field `ln N` law (R2), the critical
divergence + geometry-dependence (R4/R5), and the conservation–bandwidth bound
(R6) — which the survey did not find in the literature.

---

## 8. Falsification ledger (inverse-method pass)

| Equation | Killer test applied | Verdict |
|---|---|---|
| R1 `≤ kB` | count the quantity (verifiers vs components) | holds, disambiguated (2 regimes) |
| R2 `(a/d)ln N` | exact integration; `d→0`, `N=1` hard caps | holds, exact constant; regime `a=O(1)`, saturates for `a∝N` |
| R3 `3/2` | `a→0` (a-independence suspicious) | holds only for `a ≫ d/ln N`; heuristic |
| R4 divergence | `t0→0`, `s_D→∞`, `s_A→0`, dims, monotonicity | holds — passes all; finite-size cutoff added |
| R5 dichotomy | re-derive on a tree | holds — **produced** the tree essential-singularity result |
| R6 bandwidth | **dimensional analysis** | **two dimensional errors found & fixed** |

The dimensional check on R6 caught genuine errors (`d ≍ βm` is time⁻²; `v/β` is a
length) — the cheapest and most decisive falsifier.

---

## 9. Open questions

1. **The planar critical exponent `γ`** (headline). `N_win ∼ (s_D−s_A)^{−γ}` on
   `ℤ²` / random-geometric-mesh: `γ=1` on the line (R4), exponential on trees (R5),
   **unknown on the plane** — a genuine critical-phenomena / competing-FPP question,
   and **directly measurable** in our simulation harness. This is the sharpest
   quantitative target.
2. **Planar strangulation** (Conjecture): does `s_D > s_A` ⇒ `N_win < ∞` a.s. on
   `ℤ²`? (Prove/refute; it is the competing-FPP non-coexistence problem specialized
   to self-seeding.)
3. **Optimal escrow under a partition-size law** `π`: the pre-split `g*` minimizing
   `E[unavailability + over-spend risk]` is a newsvendor problem; characterize
   `g* = π^{-1}(critical ratio)` from real outage traces.
4. **Adversarial seeding suppression:** here detection is seeded by the adversary's
   own spends; when can an adversary suppress its seeds (spend only at
   poorly-connected leaf verifiers)? What verifier placement resists it?
5. **Tightness of R6:** pin the constant and the gossip model under which the
   product bound is tight.
6. **Formalize O2** (privacy–accountability): the `α*`/`f*` reputation frontier as
   a Byzantine-robust estimation result.

---

## 10. Validation / simulation plan

Falsifiable against the project's open harness (`sim::bench`, `mobility`), which has
verifiers, mobility models (Random Waypoint, replayed traces), and a gossip channel.
Build a **spend-gossip detector** + a **mobile double-spender**, then:
1. **Mean-field law (R2):** well-mixed topology; sweep `N`, `a/d`; fit `(a/d)ln N`;
   confirm `ln N` growth and the `a/d` slope; check the issuer-vs-collision constant.
2. **Spatial dichotomy + exponent (R4/R5):** sweep `v/(βm)` across 1; verify the
   linear→bounded transition; **fit `N_win ∼ (s_D−s_A)^{−γ}` to measure the 2-D
   exponent** (predicted 1 in 1-D; unknown on the mesh — the point). Use finite-size
   scaling (extrapolate `L→∞`).
3. **Bandwidth bound (R6):** sweep per-node gossip rate `m`; verify `T·m ≳ a ln N`
   and the `Ω(v/ℓ)` / `Ω(v²/D)` thresholds.
4. **Partition-prolonging adversary:** give the adversary the option to jam
   detection links; confirm its payoff-maximizing strategy suppresses gossip.

---

## 11. Honest self-assessment of novelty

- **Not new (cited):** the quantitative-CAP framing, escrow/pre-split, offline
  detect-not-prevent, transferable-cash inflation, security-vs-delay, Sybil-needs-
  physical-scarcity, competing-FPP as a model, epidemic thresholds, robust-
  aggregation breakdown. Much of the "grand unification" is *systematization* of
  known results, valuable for exposition but not a theorem.
- **Plausibly new (the target of the independent review):** the adversarial
  bearer-token problem statement; the mean-field `ln N` over-spend law; the
  critical divergence with geometry-dependent exponent; the conservation–bandwidth
  lower bound. Whether these are genuinely unclaimed is exactly what an independent
  study should attack.

---

## For an independent reviewer

Treat every claim above as **suspect**. Specifically worth attacking: (a) is the
"bearer-token regime" actually uncovered by prior work, or does some escrow /
mobile-agent / DRM / streaming-quota / rate-limiting / e-cash paper already do it?
(b) Is the mean-field `ln N` law or the critical divergence already a known result
under a different name (epidemic containment, rumor-scotching, competing contagions,
information-vs-infection races)? (c) Are the model assumptions (ballistic sweeping
adversary, self-seeding detection, per-verifier escrow) the right ones, or do they
smuggle in the conclusion? (d) Is there a sharper or more realistic model that
changes the answer? Independent verification and disconfirmation are the goal.
