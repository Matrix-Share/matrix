# arXiv submission guide (#115)

Two submission-ready LaTeX papers live in this repo. Both compile cleanly to PDF
(verified with `tectonic`; base packages only) and each has its verified `.bib`. The
`.bbl`, `.pdf`, and other build outputs are treated as artifacts (git-ignored) —
regenerate them with one local compile, per below, before you submit.

| Paper | Source | What it is |
|---|---|---|
| **System paper** | [`whitepaper/lifeline.tex`](whitepaper/lifeline.tex) | *Project Lifeline* — the DTN × security synthesis: open relay / gated egress, sealed-sender, capability egress, threat model, comparative evaluation. Three named observations (egress metering = offline double-spend; sealed-sender aligns privacy with accountability; open-relay/gated-egress). |
| **Theory note** | [`research/bearer-token-containment.tex`](research/bearer-token-containment.tex) | *Containing an Adversarial Bearer Token in a Partitionable Mesh* — the `E[N_win] ≈ (a/d)·ln N` over-spend law, the containment dichotomy as chase-escape / competing-FPP percolation, the continuous transition with critical exponent, and the conservation–bandwidth provisioning bound. |

## Build locally before submitting
```bash
# from docs/
cd whitepaper && tectonic lifeline.tex          # -> lifeline.pdf
cd ../research && tectonic bearer-token-containment.tex   # -> *.pdf
# classic toolchain equivalent:
#   pdflatex X && bibtex X && pdflatex X && pdflatex X
```
Only cosmetic `Underfull \hbox` warnings; no errors.

## What to upload to arXiv
arXiv wants the **source**, not the PDF. For each paper, upload a folder containing:

- the `.tex` file,
- its `.bib` (`lifeline.bib` / `containment.bib`),
- its `.bbl` (`lifeline.bbl` / `bearer-token-containment.bbl`) — produced by the local
  compile above; including it is belt-and-suspenders against BibTeX surprises on
  arXiv's build farm.

Do **not** upload the `.pdf`, `.aux`, `.log`, `.out`, or `.toc`. (Submit the two
papers as **separate** arXiv submissions — they are independent.)

## Suggested classification

**System paper (`lifeline.tex`)**
- Primary: **cs.NI** (Networking and Internet Architecture)
- Cross-list: **cs.CR** (Cryptography and Security), **cs.DC** (Distributed Computing)

**Theory note (`bearer-token-containment.tex`)**
- Primary: **cs.NI** — reaches the DTN/systems audience the framing targets
- Cross-list: **math.PR** (Probability), **cs.DC**, **cs.CR**
- *(If you'd rather foreground the probability contribution, flip primary to
  `math.PR` and keep `cs.NI` as a cross-list. MSC 60K35 / 82C22; ACM C.2.1.)*

## Before you click submit
- [ ] Confirm the author line / affiliation (`\author{...}\thanks{...}`) is how you
      want to be credited — currently **Archit Sharma**, `archit.sharma@nometria.com`.
      arXiv requires named authors; add co-authors here if any.
- [ ] The theory note is candid that its headline results are *reframed* by an
      internal review section (chase-escape, not Häggström–Pemantle; the fluid
      `γ=1` is a catch-up pole, with the real planar exponent a falsifiable
      prediction near 2-D-percolation universality). That honesty is a strength for
      a note, but read §7 and decide it reads the way you want under your name.
- [ ] Pick a license at submission (arXiv's default non-exclusive, or CC BY 4.0 to
      match the repo's openness).
- [ ] After it posts, add the arXiv ID + BibTeX to `README.md` and `CITATION.cff`,
      and cite it from the blog post (`docs/launch/blog-percolation.md`).

## Status
Both papers are **submission-ready** as source. What remains is your account action:
create the submission, upload the folder, set category + license, and post. This is
the maintainer step tracked in #115.
