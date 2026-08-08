"""ContainmentEnv — a Gymnasium-style control environment for Track B (RL).

The agent watches a chase-escape (over-spend) front spread on a graph and, each
step, chooses how much detection/gossip budget to spend for the next chunk of
micro-events. Objective: contain the fraud (few over-spends) at low detection cost.

  observation : [red_frac, blue_frac, overspent_frac, frontier_frac, t/horizon]
  action      : discrete detection level in {0..A-1} -> lb in [0, lb_max]
  reward      : -(new over-spends)/N  -  lam * (detection cost)/N   (per step)
  done        : prey extinct (contained) OR horizon reached OR budget exhausted

Pure numpy; works with or without `gymnasium` installed (duck-typed). Baselines
(fixed-rate, greedy-front) and learned agents live in Track B's `agents/`.
"""
from __future__ import annotations
import numpy as np

try:                                   # optional dependency
    import gymnasium as gym
    _Base = gym.Env
except Exception:                      # pragma: no cover
    gym = None
    class _Base:                       # minimal shim
        pass


class ContainmentEnv(_Base):
    metadata = {"render_modes": []}

    def __init__(self, neighbors, origin: int | None = None, lr: float = 1.0,
                 lb_max: float = 2.0, n_actions: int = 5, chunk: int | None = None,
                 lam: float = 1.0, budget: float = np.inf, horizon: int = 400,
                 seed: int = 0):
        self.nb = neighbors
        self.n = len(neighbors)
        self.origin = self.n // 2 if origin is None else origin
        self.lr = lr
        self.lb_max = lb_max
        self.n_actions = n_actions
        self.chunk = chunk if chunk is not None else max(1, self.n // 20)
        self.lam = lam
        self.budget = budget
        self.horizon = horizon
        self.rng = np.random.default_rng(seed)
        if gym is not None:
            self.action_space = gym.spaces.Discrete(n_actions)
            self.observation_space = gym.spaces.Box(0.0, 1.0, (5,), np.float32)

    # ---- core state ----------------------------------------------------------
    def reset(self, *, seed=None, options=None):
        if seed is not None:
            self.rng = np.random.default_rng(seed)
        self.state = np.zeros(self.n, dtype=np.int8)     # 0 W, 1 R, 2 B
        self.red_pos = np.full(self.n, -1, dtype=np.int64)
        self.reds = [int(self.origin)]
        self.state[self.origin] = 1
        self.red_pos[self.origin] = 0
        self.n_win = 1
        nb0 = self.nb[self.origin]
        self.blues = []
        if len(nb0):
            b = int(nb0[0]); self.state[b] = 2; self.blues.append(b)
        self.t = 0
        self.spent = 0.0
        return self._obs(), {}

    def _obs(self):
        n = self.n
        # frontier: reds with >=1 white neighbour (cheap estimate on a sample)
        front = 0
        sample = self.reds if len(self.reds) <= 256 else \
            [self.reds[i] for i in self.rng.integers(0, len(self.reds), 256)]
        for s in sample:
            if any(self.state[int(x)] == 0 for x in self.nb[s]):
                front += 1
        front_frac = front / max(1, len(sample))
        return np.array([len(self.reds)/n, len(self.blues)/n, self.n_win/n,
                         front_frac, self.t/self.horizon], dtype=np.float32)

    def step(self, action):
        lb = (float(action) / max(1, self.n_actions - 1)) * self.lb_max
        new_overspend = 0
        cost = 0.0
        for _ in range(self.chunk):
            if not self.reds:
                break
            tr = self.lr * len(self.reds)
            tb = lb * len(self.blues)
            total = tr + tb
            if total <= 0.0:
                break
            cost += lb                        # detection effort spent this micro-step
            if self.rng.random() * total < tr:
                site = self.reds[self.rng.integers(len(self.reds))]
                nbrs = self.nb[site]
                x = int(nbrs[self.rng.integers(len(nbrs))])
                if self.state[x] == 0:
                    self.state[x] = 1; self.red_pos[x] = len(self.reds)
                    self.reds.append(x); self.n_win += 1; new_overspend += 1
            elif self.blues:
                site = self.blues[self.rng.integers(len(self.blues))]
                nbrs = self.nb[site]
                x = int(nbrs[self.rng.integers(len(nbrs))])
                if self.state[x] == 1:
                    p = int(self.red_pos[x]); last = self.reds[-1]
                    self.reds[p] = last; self.red_pos[last] = p; self.reds.pop()
                    self.red_pos[x] = -1; self.state[x] = 2; self.blues.append(x)
        self.t += 1
        self.spent += cost
        reward = -(new_overspend / self.n) - self.lam * (cost / self.n)
        terminated = len(self.reds) == 0
        truncated = self.t >= self.horizon or self.spent >= self.budget
        info = {"n_win": self.n_win, "overspent_frac": self.n_win / self.n,
                "spent": self.spent}
        return self._obs(), reward, terminated, truncated, info
