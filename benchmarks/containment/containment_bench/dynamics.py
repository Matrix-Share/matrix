"""Chase-escape / containment dynamics in pure numpy.

Mirrors the Rust reference (`crates/sim/src/containment.rs`) so the benchmark is
accessible to the ML community while staying numerically faithful. `validate.py`
checks that these reproduce the reference results (the ln N law and rho* ~ 0.49).

Processes:
  * single_agent          — non-transferable, mean-field; over-spend ~ (a/d) ln N.
  * chase_escape_wellmixed — transferable, complete graph (fast mean-field form).
  * chase_escape          — transferable on an arbitrary graph (adjacency lists);
                            this is the real predator-prey process with a spatial
                            containment transition on lattices / RGG / mobility.
"""
from __future__ import annotations
import numpy as np


def single_agent(n: int, a: float, d: float, rng: np.random.Generator) -> int:
    """One realization of the non-transferable single-agent model on the complete
    graph. Returns total over-spend N_win. Analytically E[N_win]=sum_j a/(a+d j)
    ~ (a/d) ln N."""
    s = n           # susceptible verifiers
    imm = 0         # immunized (revocation reached)
    n_win = 0
    while True:
        r_over = a * s / n
        r_gossip = d * imm * s / n
        total = r_over + r_gossip
        if total <= 0.0:
            break
        s -= 1
        imm += 1
        if rng.random() * total < r_over:
            n_win += 1
    return n_win


def chase_escape_wellmixed(n: int, lr: float, lb: float, rng: np.random.Generator) -> int:
    """Transferable token, trail-confined detection, complete graph (mean-field).
    Prey (over-spending holders) spread at rate lr; predator converts prey at rate
    lb. Returns N_win = verifiers ever reached by prey."""
    if n < 2:
        return n
    s = n - 2
    i = 1     # prey (over-spent, not yet caught)
    r = 1     # predator (immunized)
    n_win = 1
    while True:
        r_inf = lr * i * s / n
        r_conv = lb * r * i / n
        total = r_inf + r_conv
        if total <= 0.0 or i == 0:
            break
        if rng.random() * total < r_inf:
            s -= 1; i += 1; n_win += 1
        else:
            i -= 1; r += 1
    return n_win


def chase_escape(neighbors, lr: float, lb: float, rng: np.random.Generator,
                 origin: int | None = None) -> int:
    """Transferable token, trail-confined detection, on an arbitrary graph given by
    `neighbors` (a list where neighbors[i] is an int array of i's neighbours).

    Prey (Red) fire at rate lr, converting a random White neighbour to Red
    (over-spend). Predators (Blue) fire at rate lb, converting a random Red
    neighbour to Blue (caught). Returns N_win = sites ever Red. This is the process
    whose 2-D containment transition sits at rho* = lr/lb ~ 0.49.
    """
    n = len(neighbors)
    if n < 2:
        return n
    state = np.zeros(n, dtype=np.int8)          # 0 White, 1 Red, 2 Blue
    red_pos = np.full(n, -1, dtype=np.int64)
    reds: list[int] = []
    blues: list[int] = []

    if origin is None:
        origin = n // 2
    state[origin] = 1
    red_pos[origin] = 0
    reds.append(int(origin))
    n_win = 1

    nb0 = neighbors[origin]
    if len(nb0):
        bseed = int(nb0[0])
        state[bseed] = 2
        blues.append(bseed)

    max_iters = 200 * n
    it = 0
    rand = rng.random
    randint = rng.integers
    while reds and it < max_iters:
        it += 1
        tr = lr * len(reds)
        tb = lb * len(blues)
        total = tr + tb
        if total <= 0.0:
            break
        if rand() * total < tr:
            site = reds[randint(len(reds))]
            nbrs = neighbors[site]
            nb = int(nbrs[randint(len(nbrs))])
            if state[nb] == 0:
                state[nb] = 1
                red_pos[nb] = len(reds)
                reds.append(nb)
                n_win += 1
        else:
            if not blues:
                continue
            site = blues[randint(len(blues))]
            nbrs = neighbors[site]
            nb = int(nbrs[randint(len(nbrs))])
            if state[nb] == 1:
                p = int(red_pos[nb])
                last = reds[-1]
                reds[p] = last
                red_pos[last] = p
                reds.pop()
                red_pos[nb] = -1
                state[nb] = 2
                blues.append(nb)
    return n_win


# ---- trial means (deterministic given seed) ----------------------------------

def _rng(seed: int, key: int) -> np.random.Generator:
    return np.random.default_rng((seed * 0x9E3779B1) ^ key)


def mean_single_agent(n, a, d, trials, seed=42):
    return float(np.mean([single_agent(n, a, d, _rng(seed, (n << 8) ^ t)) for t in range(trials)]))


def mean_chase_escape_wellmixed(n, lr, lb, trials, seed=42):
    return float(np.mean([chase_escape_wellmixed(n, lr, lb, _rng(seed, (n << 8) ^ t ^ 0xABCD))
                          for t in range(trials)]))


def stats_chase_escape(neighbors, lr, lb, trials, seed=42, origin=None, runaway_frac=0.05):
    """Return (mean_over_spent_fraction, runaway_probability) on a fixed graph."""
    n = len(neighbors)
    fr = np.empty(trials)
    for t in range(trials):
        fr[t] = chase_escape(neighbors, lr, lb, _rng(seed, t ^ 0x5CA1E), origin) / n
    return float(fr.mean()), float((fr > runaway_frac).mean())
