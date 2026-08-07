# The ln N law: why a bigger mesh is easier to cheat — and how percolation contains it

*A blog post drawn from the Project Lifeline research note, "Containing an
Adversarial Bearer Token in a Partitionable Mesh"
([`docs/research/`](../research/)). ~2,000 words. Publish on a dev blog with a
canonical URL; submit to Hacker News, r/rust, r/compsci. Swap the `<arXiv>`
placeholder once the preprint posts (#115).*

---

Lifeline is an offline mesh messenger: when the towers are down, your phone passes
end-to-end-encrypted messages to nearby phones, and they hop device to device until
they arrive. The feature people find magical is the **gateway**: the instant any one
phone in the mesh regains a sliver of connectivity — a satellite text, one bar at a
ridge line — it can drain everyone's queued messages out to the internet and pull
replies back in.

That gateway pass has to be *metered*. You don't want a single relief tent's
satellite uplink to be a free, unlimited pipe for the entire region — that's how you
melt the one working link. So a gateway carries a **budget**: this capability is good
for so much egress, then it's spent.

Here's the problem that kept me up. In a network that keeps *partitioning* — which is
the entire premise of a disaster mesh — how do you enforce "this token is worth a
budget of B, total, everywhere" when there is no "everywhere" to check against? It
turns out this innocent-looking accounting question is the same mathematics as
predator-prey percolation, it has a genuinely counter-intuitive answer, and it bumps
into an open problem in probability theory. Let me walk you through it.

## Escrow solves this — for friends

Maintaining a global numeric cap under partition, with no coordination, is a *solved*
problem when the participants cooperate. The classic **escrow method** (O'Neil, 1986)
and its descendants — the demarcation protocol, bounded-counter CRDTs — enforce
"total spent ≤ B" by pre-splitting the budget among *g* known sites, giving each a
private sub-allowance B/g. Everyone spends locally within their slice, the global sum
is conserved *by construction*, and sites only renegotiate when they meet. It's a
beautiful, partition-tolerant realization of a conserved scalar.

It rests on three assumptions:

1. the sites are **known and fixed**,
2. the **system owns the budget** and hands out the slices,
3. the sites are **cooperative**.

A disaster-mesh gateway capability violates all three. The verifiers (gateways)
appear and vanish — you don't know the set in advance. The budget isn't held by the
system; it's a **bearer token**, carried and moved by whoever holds it. And the
holder might be **adversarial** — the whole point of metering is the case where
someone tries to over-use it.

Strip the cooperative sites out of escrow and there's nothing to pre-allocate to.
The invariant has no defense against a token an adversary physically carries from one
offline verifier to the next. So the honest first question isn't "how do we prevent
over-spend" — it's "how *badly* can it go, and what actually bounds it?"

## Prevention is impossible; only detection is left

The static answer is quick and a little bleak. If the network is split into *k*
components that can't talk during spending, a holder present in *j* of them realizes
up to **jB** — B in each — and *no offline scheme can prevent it*. An offline verifier
decides admit/deny from the token and its own local state alone; two verifiers that
never communicate can't possibly coordinate a shared cap. This is exactly the
**offline double-spending** phenomenon from Chaum–Fiat–Naor's work on untraceable
digital cash back in 1988: offline, you cannot *prevent* a double-spend, you can only
*detect and attribute* it after the fact.

So prevention is off the table. Detection it is — and detection has a *speed*. Every
over-spend leaves a signed receipt; receipts spread through the mesh by gossip; a
verifier that has seen enough concludes "this token is over budget" and starts
refusing it. The real question becomes dynamic:

> **How much over-spend piles up before the gossiped revocation catches up?**

Call that quantity **N_win** — the number of verifiers the adversary spends at
*before* they get immunized. Everything interesting is about N_win.

## The ln N law (and why it's counter-intuitive)

Start with the well-mixed case: N verifiers, anyone can gossip to anyone. Two things
race. The adversary reaches fresh verifiers at rate *a*, over-spending each one. And
detection spreads like an epidemic — crucially, **self-seeded**: the adversary's own
spends are what create the receipts, so every verifier they burn becomes a new source
of the immunization epidemic that will eventually stop them. It's an arsonist who
starts a fire that chases them.

Solve the fluid limit and you get a clean closed form:

> **E[N_win] ≈ (a/d) · ln N**

where *a* is the adversary's spend rate and *d* the gossip rate. Two things fall out,
one obvious and one not.

The obvious one: over-spend scales with **a/d**, the ratio of abuse speed to
detection speed. Fast gossip is the lever. Fine.

The counter-intuitive one: over-spend **grows with N — the size of the network**.
A *larger* relief mesh is *more* abusable per token. That feels backwards; surely
more eyes means more safety? But detection is an epidemic, and an epidemic in a bigger
population takes logarithmically longer to *take off* from a single seed. During that
longer ignition window, the adversary keeps spending. More phones don't watch the
token more closely; they give the fire more room to smolder before it catches.

(If you seed detection differently — the honest DTN model, where a node only reacts
once it holds evidence of *two* different spends that had to meet via gossip first —
the constant changes from `a/d` to about `(3/2)·a/d`, but the `Θ((a/d)·ln N)` shape
is robust. The prefactor is actually an experimentally distinguishable fingerprint of
which detection model a deployment is running.)

## Space changes everything: a chase-escape race

The well-mixed model has a comforting property — detection always eventually
saturates, so N_win is always finite. Real meshes aren't well-mixed. Signals have
finite range; gossip and adversary both move at finite speed. And now the adversary
can *outrun the front*.

Model it as two growths racing from the origin: the adversary's reach at speed
`s_A`, the immunization front at speed `s_D`. A verifier is over-spent iff the
adversary gets there first. This is a **two-type competing growth** process, and it
gives a sharp dichotomy:

- If `s_A > s_D` (abuse outruns detection): **N_win = ∞**. Over-spend grows without
  bound, linearly in time. The token is uncontainable.
- If `s_D > s_A` (detection outruns abuse): the front encircles the adversary and
  **N_win is finite**. Contained.

Provable on a line and on trees; on the 2-D plane it becomes a *known open problem* in
probability — the "strong non-coexistence" question for competing first-passage
percolation. Häggström and Pemantle proved that at *equal* speeds the two types
coexist (both grow forever) with positive probability; strict strangulation at
unequal speeds is proven only in special cases. So our very practical
"can-we-contain-this-token" guarantee inherits a genuine open question in
probability. I find that delightful rather than embarrassing: a double-spend SLA that
is *equivalent to a hard percolation problem*.

And the transition is **continuous**. As `s_D` drops toward `s_A` from above, the
contained over-spend diverges like `(s_D − s_A)^{-1}` — a critical phenomenon, with
the speed ratio as the control parameter and a critical point at 1. Operationally,
that divergence is a warning: **never run a gateway network near critical.** You want
detection comfortably faster than abuse, not marginally.

## The part where I mark my own homework

Here's where I have to be honest, because the research note is, and it's the part I'm
proudest of. An independent review (I ran the draft through four adversarial
re-derivations) caught that I'd *named the wrong process*.

Because detection is self-seeded — a receipt exists only where an over-spend already
happened — in the physically honest local-gossip limit the immunization can only
advance *along the adversary's own trail*. That's not the free-front, invade-empty-
space competition of Häggström–Pemantle. It's **chase-escape** — predator riding
prey — a different and, happily, *much better understood* process: exactly solvable on
trees, a golden-ratio phase transition on the complete graph, proven transitions on
the Poisson–Gilbert graphs that actually model a mesh.

Which model you're in is set by a single ratio: **gossip range ÷ verifier spacing.**
Local gossip → chase-escape (detection trail-confined). Flood gossip → free-front
competition. Naming that fork is the real systems contribution.

The review also deflated my flashiest claim. That clean "critical exponent = 1" from
the fluid model? It's a *catch-up pole* — two straight arrival-time lines crossing —
with no fluctuations and no universality. The *genuinely* critical object is the
stochastic planar chase-escape transition, which numerically sits in the **2-D
percolation universality class**, predicting a containment exponent near **43/18 ≈
2.39**, not 1. So the headline flips from "we discovered an exponent" to "we make a
*falsifiable prediction*: measure the exponent on a real mesh and the interesting
outcome is a *departure* from percolation universality under real mobility."

I published the self-critique *inside the paper*. The `ln N` law is textbook epidemic
take-off dressed in new clothes; the honest survivor is the **dictionary** — offline-
double-spend-containment ↔ chase-escape, detection-bandwidth ↔ front-speed — and the
applied questions that dictionary makes answerable and nobody has closed: an
offline-CBDC double-spend bound (loss per compromised wallet vs. mandated sync
frequency), a DTN certificate-revocation-latency SLA, even the
misinformation-vs-fact-check race, which is chase-escape too.

## What you actually do with this

For Lifeline the payoff is a concrete provisioning rule. Bounding mean-field
over-spend to a target T costs detection bandwidth:

> **T · m ≥ a · ln N**

— gossip your revocations at a per-node rate that beats abuse by the network's log.
In simulation the delegatable-capability case is contained on a 2-D mesh right around
the chase-escape critical density (`ρ* ≈ 0.5`, matching the known `p_c ≈ 0.49`), which
turns into a rule of thumb we actually ship against: **gossip revocation at roughly
2× the delegation rate.** Below that, the token runs away; above it, the mesh strangles
the abuse on its own.

Every claim here is falsifiable against the open-source simulation harness in the
repo — well-mixed for the `ln N` law, Random-Waypoint/grid for the spatial dichotomy,
and a sweep of `s_D/s_A → 1` to actually *measure* the planar exponent. That last one
is the sharpest open experiment, and the whole reason the paper exists: not to claim a
theorem, but to hand you a prediction and the code to break it.

---

*The full note, with proofs, the honest four-lens review, and the citation trail, is
in [`docs/research/bearer-token-containment.tex`](../research/bearer-token-containment.tex)
(arXiv: `<arXiv-id — see #115>`). The system it came from is at
[github.com/matrix-share/matrix](https://github.com/matrix-share/matrix). If you can
prove or refute planar strangulation for self-seeded chase-escape, or you just want to
run the sweep and tell me I'm wrong about the exponent — open an issue.*
