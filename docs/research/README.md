# Research notes

Formal, in-progress research arising from Project Lifeline. Unlike the design
records (which document what is *built*), these are theory: problem statements,
models, theorems, and open questions. Sources are verified against primary records.

## Papers
- **`bearer-token-containment.tex`** — *Containing an Adversarial Bearer Token in a
  Partitionable Mesh.* The genuinely-open problem left by escrow/demarcation:
  maintaining a numeric invariant when the metered token is a transferable bearer
  instrument, the spender is adversarial, and the verifiers are unknown and mobile.
  Results: the static per-component inflation bound (Prop. 1); an exact mean-field
  over-spend law `E[N_win] ≈ (a/d)·ln N` (Thm. 1) with the counter-intuitive
  prediction that over-spend *grows* with network size; a spatial containment
  dichotomy as two-type first-passage percolation (Thm. 2, proven on line/tree;
  Conj. 1 on the plane — a known-hard percolation question); and a
  conservation–bandwidth lower bound `T·m ≥ (a ln N)/β` (Cor. 1). Includes an
  honest three-door taxonomy positioning against escrow, offline e-cash, and
  blockchain-under-delay, and a simulation protocol against `sim::bench`.
- `containment.bib` — verified bibliography (DOI / arXiv / IACR / DBLP).

## Provenance / honesty
A verified literature sweep established that most of the *framing* (quantitative
CAP, escrow/demarcation, offline-detect-not-prevent, Sybil-needs-physical-scarcity,
security-degrades-with-delay) is prior art — cited as such. The surviving
contribution is narrow and stated as such: the **adversarial bearer-token regime**
(outside escrow's cooperative-site model) and its **competing-growth dynamics**
(the `ln N` mean-field law and the spatial dichotomy + bandwidth bound), which the
survey did not find in the literature.

## Build
```bash
latexmk -pdf bearer-token-containment.tex   # or Overleaf
```
Not compiled in CI (no TeX toolchain here); begin/end, tabular, and theorem
environments balance and all `\cite` keys resolve against `containment.bib`.

## Relationship to the white paper
The white paper (`../whitepaper/`) states the egress-quota = double-spending
observation as one of three; this note is the full formal development of that one,
turned from an observation into a problem, two theorems, and an open question.
