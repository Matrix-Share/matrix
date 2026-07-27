# Project Lifeline — white paper

An academic-style technical white paper covering the system architecture and its
four principal contributions (residual differential transfer, manifest-scoped
HAVE swarm fetch, source-attributed custody reputation, and attenuable egress
capabilities), with a security analysis and an evaluation-methodology section.

## Files
- `lifeline.tex` — the paper (LaTeX, `article` class; only standard packages).
- `lifeline.bib` — the bibliography. **Every entry was verified against a
  primary source** (RFC Editor, ACM DL, USENIX, IACR, Springer, publisher pages);
  protocol/spec artifacts without a peer-reviewed paper (Nostr, Meshtastic,
  Biscuit, BLAKE3, the Signal specs) are cited as specifications, per accepted
  practice.

## Build
```bash
# with latexmk (recommended)
latexmk -pdf lifeline.tex

# or the classic four-pass sequence
pdflatex lifeline && bibtex lifeline && pdflatex lifeline && pdflatex lifeline
```
Or open `lifeline.tex` in Overleaf and compile there.

> Note: the paper has **not** been compiled in this repo's CI (no TeX toolchain
> is installed here). It uses only base packages (`geometry`, `amsmath`,
> `booktabs`, `hyperref`, `titlesec`, `enumitem`, `xcolor`) and all `\cite` keys
> resolve against `lifeline.bib`, but please run a local build before
> distributing.

## Relationship to the other docs
This paper is the scientific synthesis. The narrower design records remain the
source of truth for their subsystems and go into more implementation detail:
- [`../reliable-transfer-and-internet.md`](../reliable-transfer-and-internet.md)
- [`../capability-egress-and-service-class.md`](../capability-egress-and-service-class.md)
- [`../differential-transfer.md`](../differential-transfer.md),
  [`../geo-and-differential.md`](../geo-and-differential.md)
- [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md)
</content>
