"""ContainmentBench task suite.

A Task fixes a topology + adversary rate + detection budget grid + seeds. Policies
are scored on the cost-normalized containment objective (over-spend + lam * cost).
"""
from __future__ import annotations
from dataclasses import dataclass, field
import numpy as np
from . import topologies as topo


def _radius_for_degree(n: int, mean_degree: float) -> float:
    # RGG on the unit square: E[deg] = n * pi * r^2  ->  r = sqrt(deg/(pi n))
    return float(np.sqrt(mean_degree / (np.pi * n)))


def _center_node(pts: np.ndarray) -> int:
    return int(np.argmin(((pts - 0.5) ** 2).sum(axis=1)))


@dataclass
class Task:
    name: str
    topo: str                       # 'lattice' | 'rgg' | 'rwp'
    size: int                       # L for lattice; N for rgg/rwp
    lr: float                       # adversary (prey) rate; rho = lr / lb_max-ish
    lb_max: float = 2.0
    n_actions: int = 5
    mean_degree: float = 6.0        # rgg/rwp
    init_red: int = 9
    horizon: int = 300
    lam: float = 1.0                # cost weight in the objective
    chunk: int | None = None
    n_graphs: int = 3               # graph realizations (rgg/rwp); 1 for lattice
    n_seeds: int = 12               # episode seeds per graph

    def build(self, graph_seed: int):
        rng = np.random.default_rng(1000 + graph_seed)
        if self.topo == "lattice":
            nb, meta = topo.torus_lattice(self.size)
            origin = (self.size // 2) * self.size + self.size // 2
        elif self.topo == "rgg":
            r = _radius_for_degree(self.size, self.mean_degree)
            nb, meta = topo.random_geometric(self.size, r, rng)
            origin = _center_node(meta["pts"])
        elif self.topo == "rwp":
            r = _radius_for_degree(self.size, self.mean_degree)
            nb, meta = topo.random_waypoint(self.size, r, rng)
            origin = _center_node(meta["pts"])
        else:
            raise ValueError(self.topo)
        return nb, origin, meta

    @property
    def graphs(self):
        return 1 if self.topo == "lattice" else self.n_graphs


def default_suite() -> list[Task]:
    """A compact but representative suite: three topologies, adversary rate set
    above the lattice critical point (rho ~ 0.7) so containment is non-trivial and
    a good policy is rewarded. Sizes kept modest for a pure-Python reference run."""
    # lr in the *containable* regime: with lb up to lb_max, a policy CAN push the
    # effective ratio below rho* if it spends enough — so spending well matters.
    # lam=0.3: detection is cheaper than fraud, so containing is worth its cost —
    # this makes the objective discriminating (do-nothing is NOT optimal).
    return [
        Task("lattice-64", "lattice", 64, lr=0.6, lb_max=2.0, init_red=9, horizon=300, lam=0.3),
        Task("rgg-2500",   "rgg",   2500, lr=0.6, lb_max=2.0, init_red=9, horizon=300, lam=0.3),
        Task("rwp-2500",   "rwp",   2500, lr=0.6, lb_max=2.0, init_red=9, horizon=300, lam=0.3),
    ]
