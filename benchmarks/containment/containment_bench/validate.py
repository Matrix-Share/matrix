"""Validate the numpy dynamics against the Rust reference (crates/sim).

Reference facts to reproduce (see docs/research/bearer-token-containment.tex §10):
  * Single-agent: E[N_win]/ln N is flat at ~1.09 = a/d across decades of N.
  * 2-D lattice chase-escape: contained (small fraction) for rho well below ~0.5,
    runaway (macroscopic fraction) for rho above ~0.5 — i.e. rho* ~ 0.49-0.50.

Run: `python -m containment_bench.validate` from benchmarks/containment/.
Exit code 0 iff both checks pass.
"""
from __future__ import annotations
import sys
import numpy as np
from . import dynamics as dyn
from . import topologies as topo


def check_lnN() -> bool:
    print("== single-agent ln N law (a=d=1) ==")
    print(f"  {'N':>8} {'E[N_win]':>10} {'ln N':>8} {'ratio':>8}")
    ratios = []
    for n in [100, 300, 1000, 3000, 10000, 30000]:
        m = dyn.mean_single_agent(n, 1.0, 1.0, trials=400, seed=42)
        lnn = np.log(n)
        ratios.append(m / lnn)
        print(f"  {n:>8} {m:>10.2f} {lnn:>8.2f} {m/lnn:>8.3f}")
    ok = all(1.0 <= r <= 1.20 for r in ratios) and abs(np.mean(ratios) - 1.09) < 0.06
    print(f"  ratio ~ const a/d? mean={np.mean(ratios):.3f}  -> {'PASS' if ok else 'FAIL'}")
    return ok


def check_lattice_transition() -> bool:
    print("\n== 2-D lattice chase-escape transition (rho* ~ 0.49) ==")
    L = 64
    nb, meta = topo.torus_lattice(L)
    origin = (L // 2) * L + L // 2
    print(f"  L={L} (N={L*L})   {'rho':>6} {'mean_frac':>10} {'p_runaway':>10}")
    curve = {}
    for rho in [0.30, 0.40, 0.49, 0.60, 0.70]:
        mf, pr = dyn.stats_chase_escape(nb, rho, 1.0, trials=80, seed=7, origin=origin)
        curve[rho] = (mf, pr)
        print(f"  {'':>10}   {rho:>6.2f} {mf:>10.4f} {pr:>10.3f}")
    contained = curve[0.30][0] < 0.05          # well below rho*: microscopic
    runaway = curve[0.70][0] > 0.30            # well above rho*: macroscopic
    monotone = curve[0.30][0] < curve[0.49][0] < curve[0.70][0]
    ok = contained and runaway and monotone
    print(f"  contained@0.30 & runaway@0.70 & monotone? -> {'PASS' if ok else 'FAIL'}")
    return ok


def main() -> int:
    a = check_lnN()
    b = check_lattice_transition()
    ok = a and b
    print(f"\nVALIDATION: {'PASS' if ok else 'FAIL'} "
          f"(numpy dynamics match the Rust reference)")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
