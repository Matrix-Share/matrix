# Lifeline launch & adoption plan

An honest, staged go-to-market. The core question this answers: **who can use
Lifeline, and how do we grow it without over-promising a life-safety tool that
isn't finished or audited?**

## The one-paragraph verdict

Lifeline is **not ready for a general-public launch**, and it **is ready for a
developer/community credibility launch**. The gap is concrete: the one thing the
general public would install it for — messaging when the network is down —
depends on native phone-to-phone radios that aren't shipped ([#64](https://github.com/matrix-share/matrix/issues/64)),
a one-tap installable app that doesn't exist yet ([#117](https://github.com/matrix-share/matrix/issues/117)),
and a security audit that hasn't happened ([#65](https://github.com/matrix-share/matrix/issues/65)).
Until those land, we grow among technical early adopters who reward candor, and
we keep every public claim truthful.

## Audience map

| Segment | Reachable **now**? | Why / gate |
|---|---|---|
| Developers, self-hosters | ✅ Yes | `./quickstart.sh` / Docker / cargo |
| Mesh & DTN community (Meshtastic, Reticulum, Briar) | ✅ Yes | We bridge Nostr + Meshtastic; radio-layer help wanted |
| Security researchers | ✅ Yes | Crypto/protocol implemented, testable, documented threat model |
| Academics | ✅ Yes | Chase-escape containment papers are the credibility layer |
| **Disaster-prone communities / mutual-aid / CERT** | ❌ Not yet | Needs radios (#64) + audit (#65) + installable app (#117) |
| **Protesters / censorship regions** | ❌ Not yet | Same gates; higher stakes = higher bar |
| **Travelers / hikers / festivals / maritime** | ❌ Not yet | Needs standalone app + radios |
| **General consumers** | ❌ Not yet | All of the above |

## Four gates to a consumer launch

1. **Native BLE radio shipped** on iOS + Android ([#64](https://github.com/matrix-share/matrix/issues/64)) — makes "works with no internet" a fact, not a goal.
2. **One-tap installable app** with an embedded node ([#117](https://github.com/matrix-share/matrix/issues/117)) — today the mobile app still needs a separately-run node; a civilian can't do that.
3. **Third-party security audit** ([#65](https://github.com/matrix-share/matrix/issues/65)) — gate for any life-safety claim.
4. **The demo** ([#109](https://github.com/matrix-share/matrix/issues/109)) — an airplane-mode, phone-to-phone proof. Cheapest gate, unblocks all of Stage 0.

## Stage 0 — now: earn technical credibility (no false promises)

Everything here is doable today and costs only effort. Order matters — the demo
first, because nothing else lands without it.

- [ ] **Record the demo GIF** ([#109](https://github.com/matrix-share/matrix/issues/109)). Blocker for everything below.
- [ ] **Show HN** ([#110](https://github.com/matrix-share/matrix/issues/110)) — draft ready in [`SHOW-HN.md`](SHOW-HN.md).
- [ ] **Reddit / communities** ([#111](https://github.com/matrix-share/matrix/issues/111), [#112](https://github.com/matrix-share/matrix/issues/112)) — drafts in [`reddit-and-communities.md`](reddit-and-communities.md).
- [ ] **arXiv preprint** ([#115](https://github.com/matrix-share/matrix/issues/115)) + **theory blog post** ([#114](https://github.com/matrix-share/matrix/issues/114)) — the research is marketing collateral for this audience.
- [ ] **awesome-list PRs** ([#113](https://github.com/matrix-share/matrix/issues/113)) — compounding, passive reach.
- [ ] **Project social presence** ([#116](https://github.com/matrix-share/matrix/issues/116)) + repo social-preview/pin ([#119](https://github.com/matrix-share/matrix/issues/119)).
- [ ] **Fund/schedule the audit** ([#65](https://github.com/matrix-share/matrix/issues/65)) — start now; it has a long lead time.

## Stage 1 — early-adopter beta (after gates 1, 2, 4)

- TestFlight / Play beta / F-Droid.
- Controlled density pilots where a mesh actually forms: a hackathon, a campus, a
  festival, a single mutual-aid network. Mesh adoption is **geographic, not
  viral** — win one dense community at a time.
- Recruit mutual-aid / CERT groups as design partners (not yet as life-safety
  users).

## Stage 2 — consumer launch (after all four gates)

- App-store launch with the offline claim finally true.
- Disaster-preparedness partnerships, press, the "install-and-forget" positioning
  that solves the adoption paradox (people install *before* the disaster).

## Messaging guardrails (apply everywhere, always)

- Never claim "works with no internet / no towers" as present tense without the
  qualifier "designed to; here's what runs today." (The website and README now
  do this — keep it that way.)
- Never market SOS / life-safety to consumers before the audit (#65) and radios (#64).
- Lead with candor to technical audiences; they trust the honest version more.
- One tailored post per community; identical crossposts read as spam.

## The single biggest adoption risk

The **mesh cold-start / density problem**: a mesh is useless until enough people
*near you* run it, and that network effect is geographic. This is what capped
FireChat and Bridgefy. The distribution strategy must therefore be
community-by-community seeding, not individual-viral growth — which is why the
Stage-1 pilots target dense, bounded populations.
