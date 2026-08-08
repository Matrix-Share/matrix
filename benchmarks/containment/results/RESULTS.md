# ContainmentBench leaderboard

Objective = over-spent fraction + λ·cost (lower is better). Baselines only; a learned policy (Track B) plugs into the same loop.

## lattice-64

| rank | policy | over-spent | cost | **objective** |
|---:|---|---:|---:|---:|
| 1 | reactive-front | 0.012 | 0.921 | **0.289** |
| 2 | max-detect | 0.012 | 1.000 | **0.312** |
| 3 | constant-0.50 | 0.262 | 0.500 | **0.412** |
| 4 | fixed-rate | 0.262 | 0.500 | **0.412** |
| 5 | no-detect | 0.418 | 0.000 | **0.418** |
| 6 | random | 0.382 | 0.539 | **0.543** |

## rgg-2500

| rank | policy | over-spent | cost | **objective** |
|---:|---|---:|---:|---:|
| 1 | constant-0.50 | 0.087 | 0.500 | **0.237** |
| 2 | fixed-rate | 0.087 | 0.500 | **0.237** |
| 3 | reactive-front | 0.011 | 0.808 | **0.253** |
| 4 | max-detect | 0.010 | 1.000 | **0.310** |
| 5 | no-detect | 0.368 | 0.000 | **0.368** |
| 6 | random | 0.240 | 0.568 | **0.410** |

## rwp-2500

| rank | policy | over-spent | cost | **objective** |
|---:|---|---:|---:|---:|
| 1 | max-detect | 0.159 | 1.000 | **0.459** |
| 2 | reactive-front | 0.159 | 1.000 | **0.459** |
| 3 | no-detect | 0.573 | 0.000 | **0.573** |
| 4 | constant-0.50 | 0.431 | 0.500 | **0.581** |
| 5 | fixed-rate | 0.431 | 0.500 | **0.581** |
| 6 | random | 0.571 | 0.501 | **0.721** |
