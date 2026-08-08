"""Baseline containment policies for the ContainmentBench leaderboard.

A *policy* maps an observation (the 5-vector from `ContainmentEnv`) to a discrete
detection level in {0..n_actions-1}. Learned agents (Track B) implement the same
interface and plug into the same leaderboard.

obs = [red_frac, blue_frac, overspent_frac, front_frac, t/horizon]
"""
from __future__ import annotations
import numpy as np


class Policy:
    name = "policy"
    def __init__(self, n_actions: int):
        self.A = n_actions
    def __call__(self, obs) -> int:
        raise NotImplementedError
    def reset(self):
        pass


class NoDetect(Policy):
    name = "no-detect"
    def __call__(self, obs): return 0


class MaxDetect(Policy):
    name = "max-detect"
    def __call__(self, obs): return self.A - 1


class Constant(Policy):
    def __init__(self, n_actions, level: float):
        super().__init__(n_actions); self.a = int(round(level * (n_actions - 1)))
        self.name = f"constant-{level:.2f}"
    def __call__(self, obs): return self.a


class FixedRate(Policy):
    """Analytic provisioning: spend a constant detection rate set to meet the
    threshold T·m ≥ a·ln N (i.e. m ≥ a·ln N / T for over-spend target T). We map
    that target rate onto the action grid via `lb_target / lb_max`."""
    name = "fixed-rate"
    def __init__(self, n_actions, lb_target: float, lb_max: float):
        super().__init__(n_actions)
        self.a = int(round(np.clip(lb_target / lb_max, 0, 1) * (n_actions - 1)))
    def __call__(self, obs): return self.a


class Reactive(Policy):
    """Spend in proportion to the *active front* — ramp detection up while the
    over-spend is still spreading, stand down once it stops. A cheap, sensible
    heuristic and the main target for a learned policy to beat."""
    name = "reactive-front"
    def __init__(self, n_actions, gain: float = 2.0):
        super().__init__(n_actions); self.gain = gain
    def __call__(self, obs):
        front = obs[3]                       # frontier fraction of the red set
        red = obs[0]                         # red fraction of the graph
        drive = self.gain * front * (1.0 + 4.0 * red)
        return int(np.clip(round(drive * (self.A - 1)), 0, self.A - 1))


class Random(Policy):
    name = "random"
    def __init__(self, n_actions, seed: int = 0):
        super().__init__(n_actions); self.rng = np.random.default_rng(seed)
    def __call__(self, obs): return int(self.rng.integers(self.A))
    def reset(self): pass


def default_policies(n_actions: int, lb_max: float, lb_target: float, seed: int = 0):
    """The baseline roster reported on the leaderboard."""
    return [
        NoDetect(n_actions),
        Constant(n_actions, 0.5),
        MaxDetect(n_actions),
        FixedRate(n_actions, lb_target, lb_max),
        Reactive(n_actions, gain=2.0),
        Random(n_actions, seed=seed),
    ]
