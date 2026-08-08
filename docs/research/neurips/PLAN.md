# ML-venue track: master plan (durable — resume from this file)

**Purpose.** Turn the Lifeline containment research into paper(s) for ML / neural /
security / networking venues. Three parallel tracks, one shared substrate. This
file is the source of truth so work survives context resets — update the **Status**
boxes as you go.

> Context in one paragraph: we have a *theory note* (`../bearer-token-containment.tex`)
> that maps **offline capability-abuse containment** to **chase-escape (predator–prey)
> percolation**, plus a Rust simulator (`crates/sim/src/containment.rs`) whose measured
> results are already in the paper: the mean-field `E[N_win] ≈ (a/d) ln N` law; a
> well-mixed→lattice **dichotomy** (trail-confined detection runs away when well-mixed,
> contains on a 2-D lattice); a critical ratio **ρ\* ≈ 0.49–0.50** (= known planar
> chase-escape p_c); order-parameter exponent **β ≈ 0.14–0.20**, consistent with 2-D
> percolation (β = 5/36 ≈ 0.139). Reviewer said: "cross-domain application is a paper."

---

## 0. Shared foundation (reused by ALL tracks) — build this FIRST

**Reference implementation (exists):** `crates/sim/src/containment.rs` — `run_single_agent`,
`run_chase_escape` (well-mixed trail), `run_chase_escape_flood`, `run_chase_escape_2d`
(torus lattice), `stats_chase_escape_2d`, `report_scaling`. CLI: `cargo run -p lifeline-sim
--release -- containment` and `-- containment-scaling`. Data: `docs/research/data/*.csv`.

**New shared artifact:** a **pure-Python + numpy** package that re-implements the same
dynamics (so ML folks can `pip install` and RL can roll out fast), *validated numerically
against the Rust reference* (must reproduce ρ\*≈0.49 and the ln N ratio ≈1.09 within noise).

- Path: `benchmarks/containment/` — package name `containment_bench`.
- Modules:
  - `dynamics.py` — the three processes (single-agent, well-mixed chase-escape (+flood),
    2-D lattice chase-escape) as numpy step-functions; seeded RNG; returns trajectories.
  - `topologies.py` — well-mixed (complete), 2-D torus lattice, **random geometric graph
    (RGG)** and a simple **Random-Waypoint mobility** graph (the "realistic mesh" the theory
    flags as the open case). RGG/mobility are NEW vs the Rust sim → novelty.
  - `env.py` — a **Gymnasium-style** env (`ContainmentEnv`) exposing observation/action for
    the RL track (see Track B). Works without gymnasium installed (duck-typed API); if
    gymnasium present, subclass `gymnasium.Env`.
  - `metrics.py` — `N_win`, over-spent fraction, cost-normalized containment, AUC over ρ,
    runaway probability (order parameter).
  - `validate.py` — asserts the numpy dynamics match the Rust CSVs (ρ\*, ln N ratio).
- **Toolchain decision (locked):** pure numpy for dynamics + metrics (present). gymnasium is
  optional. RL keeps torch **optional** — ship a numpy REINFORCE/CEM baseline so nothing
  hard-depends on torch (torch wheels for py3.13 are fragile). If torch installs cleanly,
  add a PPO baseline as a bonus.
- **Status:** [x] **DONE + validated.** `benchmarks/containment/` package built:
  `dynamics.py`, `topologies.py` (lattice + RGG + random-waypoint), `metrics.py`, `env.py`
  (`ContainmentEnv`), `validate.py`. `python -m containment_bench.validate` **passes**:
  ln N ratio 1.085 (Rust 1.09); lattice transition contained@0.30 → 0.218@0.49 → runaway@0.70
  (ρ\*≈0.49, matches Rust). README + pyproject added.
  - **Track-B note:** `ContainmentEnv` runs but its default episode granularity is coarse
    (one chunk can resolve an episode from the 1-red seed). Track B's first task: tune
    `chunk`/`horizon` and seed a small initial red patch so the agent makes many decisions.

---

## Track A — NeurIPS **Datasets & Benchmarks**: "ContainmentBench"

*The closest analog to the REAL paper the owner admires (an environment + tasks +
baselines + a measured finding). Lowest effort, highest credibility-per-effort.*

- **Contribution:** a reproducible benchmark for **containment / mitigation policies on
  competing-diffusion (chase-escape) processes** across topologies (well-mixed, lattice,
  RGG, mobility), with metrics, baseline policies, a leaderboard, and a reference empirical
  result (the universality-class measurement).
- **Target:** NeurIPS Datasets & Benchmarks (primary); backup: ICML / a graph-learning or
  ML-for-systems workshop.
- **Deliverables:**
  - `benchmarks/containment/` package (shared foundation above) + `tasks.py` (the benchmark
    suite: a fixed set of (topology, ρ, budget) scenarios with seeds).
  - `baselines.py` — trail-gossip, flood-gossip, fixed-rate provisioning (`T·m ≥ a ln N`),
    greedy-front, oracle-upper-bound.
  - `run_benchmark.py` — produces the results table + CSVs + a `RESULTS.md` leaderboard.
  - `benchmarks/containment/README.md` — install, quickstart, task spec, how to submit a
    policy (REAL-style).
  - Paper: `docs/research/neurips/dnb.tex` (NeurIPS D&B format) — reuses the ln N + dichotomy
    + universality figures (already built in the theory paper) framed as the benchmark's
    reference findings.
- **Experiments to run:** baseline sweep across all tasks; the finite-size universality
  measurement on RGG/mobility (extends the lattice result to the "realistic mesh" open case).
- **Effort:** low–medium (packaging + baselines + writing). **Status:** [ ] not started

---

## Track B — **RL / control** for containment (highest-novelty ML paper)

*Learn the mitigation policy; beat the fixed-rate SLA. A genuine ML method on a novel,
well-motivated environment.*

- **Problem:** allocate a *budget* of detection/gossip effort over time+space to minimize
  `N_win + λ·cost`, observing the spreading front — a POMDP. Baseline = the analytic
  fixed-rate `T·m ≥ a ln N`. Hypothesis: a learned policy exploits spatial/temporal
  structure (front-targeting) to contain at lower budget, especially near ρ\*.
- **Target:** NeurIPS / ICML main (if strong), AAMAS / AAAI, or a NeurIPS RL/graph workshop.
- **Deliverables:**
  - `ContainmentEnv` (in shared `env.py`): obs = coarse front features (occupied fractions,
    front perimeter, local red/blue counts on a downsampled grid); action = budget
    allocation (global rate, or per-region on a K×K coarse grid); reward = −(new over-spends)
    − λ·(gossip spent); episode = until contained or horizon.
  - `agents/` — random, fixed-rate, greedy-front (heuristic), and a **learned** policy
    (numpy REINFORCE/CEM first; PPO via torch if available).
  - `train.py` + `eval.py` — training curves + policy-vs-baseline containment/cost tables +
    a "what did it learn" visualization (allocation heatmap vs the front).
  - Paper: `docs/research/neurips/rl.tex`.
- **Experiments:** learned vs baselines across ρ (esp. near ρ\*), budget ablation,
  generalization across topologies/sizes (train lattice → test RGG/mobility).
- **Effort:** high (env + training infra + method). **Status:** [ ] not started
- **Risk/decision:** keep torch optional (numpy REINFORCE/CEM is enough for a first result);
  only pull in torch/PPO if it installs cleanly on py3.13.

---

## Track C — **Diffusion-intervention / misinformation** framing

*Recast chase-escape as fact-check-vs-rumor; measure the critical containment threshold;
add a learned intervention. ML-for-social-good / graph-learning framing.*

- **Contribution:** the misinformation-vs-fact-check race **is** chase-escape (the theory
  paper already notes this). Provide the intervention benchmark (where/when to inject
  fact-checks to contain a rumor cascade) + the measured critical threshold, and apply
  Track B's learned policy to it. Mostly a **reframing + one application experiment** on
  content-diffusion graphs (RGG / a synthetic scale-free / a small real cascade if available).
- **Target:** NeurIPS/ICML "AI for social good" or graph-learning workshop; or a full-venue
  application paper if the intervention result is strong.
- **Deliverables:** a scenario pack (`tasks.py` "misinfo" split), the threshold measurement,
  Track B's policy applied to fact-check injection, and `docs/research/neurips/diffusion.tex`.
- **Effort:** low **if built on A + B** (shares env, metrics, agents). **Status:** [ ] not started

---

## Build order & dependencies

1. **Shared foundation** (`benchmarks/containment/` numpy env + validate vs Rust). Blocks A, B, C.
2. **Track A** (benchmark + baselines + D&B paper) — first paper out; validates the env is useful.
3. **Track B** (RL env + agents + paper) — builds on A's env/metrics.
4. **Track C** (misinfo reframe + one experiment + workshop paper) — builds on A + B.

Papers share one bib: `docs/research/neurips/refs.bib`. Compile with `tectonic` (pgfplots
already verified working; reuse the theory paper's figure style).

## Reproducibility / artifact (do for every track)
- Seeded everything; CSVs committed under `docs/research/data/` (force-add: `data/` is gitignored).
- Each paper: an "Artifacts" appendix pointing at `benchmarks/containment/` + exact commands.
- Aim for the venue's artifact-evaluation badge — the open harness is a real asset.

## Open decisions / to confirm with owner
- Author line / affiliation for the ML papers (default: Archit Sharma, archit.sharma@nometria.com).
- How much compute for Track B (local numpy RL is modest; larger runs need more).
- Whether to also target a security/networking venue in parallel with the *theory* paper
  (Financial Crypto / IMC) — independent of these three ML tracks.

## Status log (append dated one-liners as work lands)
- (init) Plan written; toolchain probed (py3.13 + numpy; no torch/gym/scipy; pure-numpy env decided).
- Shared foundation built + validated against the Rust reference (ln N ratio, ρ\*≈0.49). Tracks A/B/C now unblocked. Next: Track A (benchmark suite + baselines + D&B paper).
