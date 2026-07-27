# Project Lifeline — white paper

An academic-style technical paper on the system. It is deliberately honest that
the machinery is a **composition of known primitives** (self-certifying identity,
content addressing, spray-and-wait, sealed-sender crypto, macaroon-style
capabilities) and that its actual contribution is **three observations that
emerged from composing them** at the DTN × security boundary:

1. **Egress metering is offline double-spending** (Prop. 1) — an offline-verifiable
   bearer capability cannot enforce a *global* usage budget across a partition;
   the achievable bound is per-component, and the right design detects and
   attributes over-spend on merge (à la Chaum–Fiat–Naor e-cash) rather than
   pretending to prevent it.
2. **Sealed-sender aligns privacy with accountability** (Prop. 2) — hiding the
   sender forces reputation to be source-local, which removes the shared state a
   Sybil would badmouth.
3. **Open relay, gated egress** (Obs. 1) — the resource asymmetry that organizes
   the design and from which (1) follows.

It also states plainly what is *not* new (a composition table), gives a
threat-model security analysis, and a reproducible comparative evaluation with
real numbers.

## Files
- `lifeline.tex` — the paper (LaTeX, `article` class; standard packages only).
- `lifeline.bib` — the bibliography. **Every entry was verified against a primary
  source** (RFC Editor, ACM DL, USENIX, IACR, Springer). Protocol/spec artifacts
  without a peer-reviewed paper (Nostr, Meshtastic, Biscuit, BLAKE3, the Signal
  specs) are cited as specifications, per accepted practice.

## Build
```bash
latexmk -pdf lifeline.tex
# or: pdflatex lifeline && bibtex lifeline && pdflatex lifeline && pdflatex lifeline
```
Or open `lifeline.tex` in Overleaf.

> Not compiled in this repo's CI (no TeX toolchain here). Uses only base packages
> (`geometry`, `amsmath`, `amssymb`, `amsthm`, `booktabs`, `array`, `enumitem`,
> `hyperref`, `titlesec`); begin/end and theorem environments balance and all
> `\cite` keys resolve against `lifeline.bib` — but run a local build before
> distributing.

## Relationship to the other docs
This paper is the scientific synthesis. The narrower design records remain the
source of truth for their subsystems:
- [`../reliable-transfer-and-internet.md`](../reliable-transfer-and-internet.md)
- [`../capability-egress-and-service-class.md`](../capability-egress-and-service-class.md)
- [`../differential-transfer.md`](../differential-transfer.md), [`../geo-and-differential.md`](../geo-and-differential.md)
- [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md)
