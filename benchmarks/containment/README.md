# ContainmentBench

A benchmark for **containment / mitigation policies on competing-diffusion
(chase-escape) processes** — the offline capability-abuse containment problem from
[`docs/research/bearer-token-containment.tex`](../../docs/research/bearer-token-containment.tex),
recast as a reusable, ML-friendly environment.

Pure Python + numpy. The Rust simulator in [`crates/sim`](../../crates/sim) is the
**reference implementation**; the numpy port here is validated against it.

## Install & validate
```bash
cd benchmarks/containment
pip install -e .            # numpy only; gymnasium/torch optional
python -m containment_bench.validate
```
`validate` reproduces the two reference facts (exit 0 on success):
- single-agent `E[N_win]/ln N` flat at ≈1.09 = a/d across decades of N;
- 2-D lattice chase-escape transition at **ρ\* ≈ 0.49** (contained below, runaway above).

## Package layout
| Module | What |
|---|---|
| `dynamics.py` | the three processes (single-agent, well-mixed, graph chase-escape) in numpy |
| `topologies.py` | adjacency lists: torus lattice (reference), random-geometric, random-waypoint mobility |
| `metrics.py` | `N_win`, over-spent fraction, runaway probability (order parameter), containment AUC, cost-normalized objective |
| `env.py` | `ContainmentEnv` — Gymnasium-style control env for the RL track |
| `validate.py` | numpy-vs-Rust reference checks |

## Research tracks (see [`docs/research/neurips/PLAN.md`](../../docs/research/neurips/PLAN.md))
- **A · NeurIPS D&B** — this benchmark: task suite + baseline policies + leaderboard.
- **B · RL** — learn a mitigation policy in `ContainmentEnv` that beats the analytic
  fixed-rate `T·m ≥ a·ln N` baseline.
- **C · Diffusion-intervention** — the misinformation-vs-fact-check reframing.

## Status
Shared foundation: **validated** (dynamics, topologies, metrics, env). Tracks A/B/C
build on it. RGG / random-waypoint topologies extend the reference (lattice-only)
to the "realistic mesh" case the theory flags as open.
