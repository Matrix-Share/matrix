"""Topologies for the containment benchmark, as adjacency (neighbour) lists.

  * torus_lattice(L)      — the reference 2-D grid (matches the Rust sim).
  * random_geometric(N,r) — RGG: N random points, edges within radius r. The
                            standard model of a wireless mesh.
  * random_waypoint(N,r)  — a mobility snapshot: RGG on positions drawn from the
                            Random-Waypoint stationary distribution (denser in the
                            middle). The "realistic mesh" the theory flags as open.

RGG / mobility are NEW relative to the Rust reference (which only has the lattice),
so measuring the containment transition on them is part of the benchmark's novelty.
"""
from __future__ import annotations
import numpy as np


def torus_lattice(L: int):
    n = L * L
    nb = [None] * n
    for site in range(n):
        x = site % L
        y = site // L
        nb[site] = np.array([
            y * L + (x + 1) % L,
            y * L + (x - 1) % L,
            ((y + 1) % L) * L + x,
            ((y - 1) % L) * L + x,
        ], dtype=np.int64)
    return nb, dict(kind="torus_lattice", L=L, n=n, mean_degree=4.0)


def _rgg_from_points(pts: np.ndarray, radius: float):
    n = len(pts)
    r2 = radius * radius
    nb = [None] * n
    deg = 0
    for i in range(n):
        d2 = ((pts - pts[i]) ** 2).sum(axis=1)
        idx = np.where((d2 > 0.0) & (d2 <= r2))[0]
        nb[i] = idx.astype(np.int64)
        deg += len(idx)
    return nb, deg / n


def random_geometric(n: int, radius: float, rng: np.random.Generator):
    pts = rng.random((n, 2))
    nb, mean_deg = _rgg_from_points(pts, radius)
    return nb, dict(kind="random_geometric", n=n, radius=radius, mean_degree=mean_deg, pts=pts)


def random_waypoint(n: int, radius: float, rng: np.random.Generator):
    """Random-Waypoint stationary positions: sample each node as the midpoint of two
    uniform draws (a cheap approximation to the RWP node-density, which peaks in the
    centre), then connect within `radius`."""
    a = rng.random((n, 2))
    b = rng.random((n, 2))
    pts = 0.5 * (a + b)
    nb, mean_deg = _rgg_from_points(pts, radius)
    return nb, dict(kind="random_waypoint", n=n, radius=radius, mean_degree=mean_deg, pts=pts)
