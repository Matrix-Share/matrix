"""Benchmark metrics for containment policies."""
from __future__ import annotations
import numpy as np


def over_spent_fraction(n_win: int, n: int) -> float:
    return n_win / n


def runaway_probability(fractions, thresh: float = 0.05) -> float:
    """Order parameter: P(prey survives to a macroscopic fraction)."""
    return float((np.asarray(fractions) > thresh).mean())


def containment_auc(rhos, fractions) -> float:
    """Area under the over-spent-fraction curve over the swept rho range (lower =
    better containment). Trapezoidal, normalized by the rho span."""
    rhos = np.asarray(rhos, float); f = np.asarray(fractions, float)
    order = np.argsort(rhos)
    rhos, f = rhos[order], f[order]
    return float(np.trapz(f, rhos) / (rhos[-1] - rhos[0]))


def cost_normalized_containment(n_win: int, n: int, gossip_spent: float,
                                lam: float = 1.0) -> float:
    """Policy objective: over-spend plus lam * detection cost (both as fractions of
    N). Lower is better. This is what the RL agent (Track B) minimizes."""
    return n_win / n + lam * gossip_spent / n
