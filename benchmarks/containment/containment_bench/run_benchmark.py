"""Run the ContainmentBench policy leaderboard.

For each task and each baseline policy, roll out `ContainmentEnv` over graph x seed
replicas and report the cost-normalized containment objective. Writes results.csv
and a RESULTS.md leaderboard. Learned agents (Track B) drop into the same loop.

Run: `python -m containment_bench.run_benchmark` from benchmarks/containment/.
"""
from __future__ import annotations
import csv, os, time
import numpy as np
from .env import ContainmentEnv
from . import baselines as B
from .tasks import default_suite

OUT = os.path.join(os.path.dirname(__file__), "..", "results")


def run_episode(task, policy, nb, origin, seed) -> tuple[float, float]:
    env = ContainmentEnv(nb, origin=origin, lr=task.lr, lb_max=task.lb_max,
                         n_actions=task.n_actions, chunk=task.chunk, lam=task.lam,
                         horizon=task.horizon, init_red=task.init_red, seed=seed)
    obs, _ = env.reset(seed=seed)
    policy.reset()
    info = {"overspent_frac": env.n_win / env.n, "spent": 0.0}
    while True:
        obs, _, term, trunc, info = env.step(policy(obs))
        if term or trunc:
            break
    return info["overspent_frac"], info["intensity"]   # both in [0,1]


def evaluate():
    os.makedirs(OUT, exist_ok=True)
    rows = []
    for task in default_suite():
        # pre-build the graph realizations once (reused across policies + seeds)
        graphs = [task.build(g) for g in range(task.graphs)]
        # fixed-rate provisioning ~ 2x the adversary rate (the paper's Cor. rule of thumb)
        policies = B.default_policies(task.n_actions, task.lb_max, lb_target=2.0 * task.lr)
        print(f"\n### task {task.name}  ({task.topo}, lr={task.lr})")
        for pol in policies:
            os_fracs, costs = [], []
            t0 = time.time()
            for (nb, origin, meta) in graphs:
                for s in range(task.n_seeds):
                    of, c = run_episode(task, pol, nb, origin, seed=1000 * s + 7)
                    os_fracs.append(of); costs.append(c)
            of = np.array(os_fracs); c = np.array(costs)
            obj = of + task.lam * c
            rows.append(dict(task=task.name, topo=task.topo, policy=pol.name,
                             overspent=of.mean(), overspent_std=of.std(),
                             cost=c.mean(), objective=obj.mean(),
                             objective_std=obj.std(), n=len(of)))
            print(f"  {pol.name:<16} overspent={of.mean():.3f}  cost={c.mean():.3f}  "
                  f"objective={obj.mean():.3f}  ({time.time()-t0:.1f}s)")
    # write CSV
    with open(os.path.join(OUT, "leaderboard.csv"), "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        w.writeheader(); w.writerows(rows)
    _write_markdown(rows)
    print(f"\nWrote {OUT}/leaderboard.csv and RESULTS.md")
    return rows


def _write_markdown(rows):
    by_task = {}
    for r in rows:
        by_task.setdefault(r["task"], []).append(r)
    lines = ["# ContainmentBench leaderboard",
             "",
             "Objective = over-spent fraction + λ·cost (lower is better). "
             "Baselines only; a learned policy (Track B) plugs into the same loop.",
             ""]
    for task, rs in by_task.items():
        rs = sorted(rs, key=lambda x: x["objective"])
        lines += [f"## {task}", "",
                  "| rank | policy | over-spent | cost | **objective** |",
                  "|---:|---|---:|---:|---:|"]
        for i, r in enumerate(rs, 1):
            lines.append(f"| {i} | {r['policy']} | {r['overspent']:.3f} | "
                         f"{r['cost']:.3f} | **{r['objective']:.3f}** |")
        lines.append("")
    with open(os.path.join(OUT, "RESULTS.md"), "w") as f:
        f.write("\n".join(lines))


if __name__ == "__main__":
    evaluate()
