# Blog post outline: "When does a phone mesh actually deliver?" (#114)

Lifeline's unfair advantage is that it has *theory*, not just vibes. This post
turns `WHITEPAPER.md` + `docs/research/` into something a general technical
audience will read and share. Aim: top of HN / r/rust on merit, funnel back to repo.

**Working title:** *What percolation theory says about when a phone mesh delivers*
**Length:** 1,500–2,500 words. **Tone:** rigorous but accessible; one good diagram
beats three equations.

## Outline

1. **The hook** — "If everyone in a stadium has the app and the towers are down,
   can a message get across the room? The answer is a phase transition."

2. **The setup** — model phones as nodes placed at density λ; two nodes link if
   within radio range r. A message can traverse the mesh only if a connected path
   exists. This is continuum percolation.

3. **The critical density** — below a critical λc, the network is a scatter of
   small islands; above it, a giant connected component appears suddenly. Show the
   S-curve. This is *the* number that decides whether Lifeline works in a given crowd.

4. **The delivery law** — summarize the mean-field result (average hop count / delay
   scaling ~ ln N for a connected mesh). Explain intuitively why "carry" (mobility)
   rescues you below λc: time substitutes for density.

5. **The open question** — the planar critical exponent γ: how delivery probability
   ramps as you approach λc from below. We can *measure* it in-sim. Show the plot
   and invite others to reproduce/beat it.

6. **Why it matters for the app** — density → range → battery → hop budget are the
   real design knobs. Tie each back to a concrete Lifeline decision.

7. **Reproduce it** — point at the eval harness in the repo; "here's how to run the
   simulation yourself." A runnable artifact is what makes this credible.

## Assets to make
- [ ] The percolation S-curve (delivery vs density) — from the eval harness.
- [ ] A hop-count vs N plot showing the ln N law.
- [ ] One clean schematic of islands → giant component.

## Distribution
- Publish on a personal/dev blog with a canonical URL.
- Submit to This Week in Rust, HN (as a regular link), r/rust, r/compsci.
- Link from README ("The theory") and the site's Open-source section.
- Later: fold into the arXiv preprint (#115) with a proper references section.
