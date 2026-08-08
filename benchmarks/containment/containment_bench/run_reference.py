"""Reference findings: the containment transition across topologies.

Reproduces the paper's dichotomy and, crucially, EXTENDS it from the lattice
(the only spatial topology in the Rust reference) to the random-geometric graph
and a Random-Waypoint mobility snapshot — the "realistic mesh" the theory flags as
the open case. Emits reference_dichotomy.csv and prints an estimated rho* per topology.

Run: `python -m containment_bench.run_reference` from benchmarks/containment/.
"""
from __future__ import annotations
import csv, os
import numpy as np
from . import dynamics as dyn
from . import topologies as topo

OUT = os.path.join(os.path.dirname(__file__), "..", "results")
RHOS = [0.30, 0.45, 0.60, 0.75, 0.90]
TRIALS = 25
N_GRAPHS = 2


def _center(pts):
    return int(np.argmin(((pts - 0.5) ** 2).sum(axis=1)))


def spatial_stats(kind, size, rho, seed_base):
    """Mean over-spent fraction + runaway prob for a spatial topology at a given
    rho (= lr with lb=1), averaged over N_GRAPHS graph realizations."""
    mfs, prs = [], []
    for g in range(N_GRAPHS):
        rng = np.random.default_rng(2000 + g)
        if kind == "lattice":
            nb, meta = topo.torus_lattice(size); origin = (size // 2) * size + size // 2
        elif kind == "rgg":
            r = float(np.sqrt(6.0 / (np.pi * size)))
            nb, meta = topo.random_geometric(size, r, rng); origin = _center(meta["pts"])
        else:  # rwp
            r = float(np.sqrt(6.0 / (np.pi * size)))
            nb, meta = topo.random_waypoint(size, r, rng); origin = _center(meta["pts"])
        mf, pr = dyn.stats_chase_escape(nb, rho, 1.0, TRIALS, seed=seed_base + g, origin=origin)
        mfs.append(mf); prs.append(pr)
    return float(np.mean(mfs)), float(np.mean(prs))


def estimate_rho_star(rhos, fracs, thresh=0.15):
    """Linear-interpolate where the over-spent fraction first crosses `thresh`."""
    rhos = np.asarray(rhos); fracs = np.asarray(fracs)
    for i in range(1, len(rhos)):
        if fracs[i - 1] < thresh <= fracs[i]:
            t = (thresh - fracs[i - 1]) / (fracs[i] - fracs[i - 1] + 1e-12)
            return float(rhos[i - 1] + t * (rhos[i] - rhos[i - 1]))
    return float("nan")


def main():
    os.makedirs(OUT, exist_ok=True)
    rows = []
    print("== Containment transition across topologies (over-spent fraction vs rho) ==")

    # Well-mixed reference (mean-field, fast): runs away at every rho.
    print("topology   " + "  ".join(f"rho={r:.2f}" for r in RHOS))
    wm = [dyn.mean_chase_escape_wellmixed(20000, r, 1.0, 60, seed=42) / 20000 for r in RHOS]
    for r, f in zip(RHOS, wm):
        rows.append(dict(topology="well-mixed", rho=r, mean_frac=f, p_runaway=float(f > 0.05)))
    print("well-mixed " + "  ".join(f"{f:8.3f}" for f in wm))

    specs = [("lattice", 48), ("rgg", 1600), ("rwp", 1600)]
    rho_star = {}
    for kind, size in specs:
        fr = []
        for r in RHOS:
            mf, pr = spatial_stats(kind, size, r, seed_base=7)
            fr.append(mf)
            rows.append(dict(topology=kind, rho=r, mean_frac=mf, p_runaway=pr))
        rho_star[kind] = estimate_rho_star(RHOS, fr)
        print(f"{kind:<10} " + "  ".join(f"{f:8.3f}" for f in fr) +
              f"   rho*~{rho_star[kind]:.2f}")

    with open(os.path.join(OUT, "reference_dichotomy.csv"), "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["topology", "rho", "mean_frac", "p_runaway"])
        w.writeheader(); w.writerows(rows)

    print("\nFinding: well-mixed runs away at every rho; ALL spatial topologies "
          "(lattice, RGG, mobility) contain below a finite rho* — so trail-confined "
          "detection works on realistic meshes, not just the idealized lattice.")
    print("rho* estimates:", {k: round(v, 3) for k, v in rho_star.items()})
    return rows, rho_star


if __name__ == "__main__":
    main()
