# Release readiness — is Lifeline ready to be open-sourced?

**Short answer: yes, as a clearly-labeled `0.1.0-alpha` developer preview — and
no, not for production or life-safety use.** Both halves matter; this document is
the honest cross-check behind that verdict.

## Verdict

| Question | Answer |
|---|---|
| Can a newcomer clone it and run it? | **Yes.** `docker compose up --build`, or `cargo run`. The web app, node, simulator, mobile, and SaaS all run from documented steps. |
| Does it build and is it tested? | **Yes.** `cargo build --workspace` is clean; **292 tests** pass; `cargo clippy -- -D warnings` and `cargo fmt` are clean; the acceptance simulator hits its ≥95%-delivery criterion (achieves 100%). |
| Is it honest about what it is? | **Yes.** Alpha status, "not audited", and "native radio not shipped" are stated in the README, white paper, and marketing. |
| Is it safe to rely on for real emergencies today? | **No.** Not third-party audited; native phone-to-phone radios aren't shipped yet. |
| Is it a legitimate open-source release? | **Yes — as an alpha.** Releasing an unfinished-but-real project openly, clearly labeled, is normal and healthy. |

## What's ready ✓

- **Runnable across all surfaces**: mesh node + web app, mobile (Expo), SaaS
  (Next.js), static site, acceptance simulator.
- **Correctness**: 292 tests across crypto, DTN routing, CRDT sync, transport,
  geo, engine; deterministic acceptance simulator with a headline delivery AC.
- **Real cryptography from audited primitives** (no custom crypto): Ed25519,
  X25519, HKDF-SHA256, XChaCha20-Poly1305, BLAKE3, Argon2id.
- **Hardened framing** (strict, bounded parsers — the Bridgefy lesson).
- **Documentation**: [README](../README.md), plain + technical white papers,
  [ARCHITECTURE](../ARCHITECTURE.md), [USE-CASES](USE-CASES.md), [STATUS](../STATUS.md),
  [GAPS](../GAPS.md), roadmaps, per-crate/app READMEs.
- **OSS hygiene**: Apache-2.0 LICENSE, CONTRIBUTING, CODE_OF_CONDUCT, SECURITY
  (private vuln reporting), MAINTAINERS, issue/PR templates, CHANGELOG, CI
  (fmt/clippy/test/sim/docker + saas + mobile).
- **No secrets, personal paths, or build artifacts tracked.**

## What's not ready (be honest) ✗

- **No third-party security audit.** The threat model is documented and the code
  is unit-tested, but it has not been independently reviewed. This is the single
  biggest caveat for a security tool.
- **Native radio bearers not shipped.** BLE / Wi-Fi Aware / ultrasound are
  designed and (for BLE) have a tested platform-independent core, but the
  on-device radio backends aren't built — so "works phone-to-phone with no
  internet at all" is the goal, not yet the out-of-the-box reality. Today nodes
  mesh over a local relay or LAN that stands in for those bearers.
- **Mobile app is a client, not yet a full mesh node** (no on-device engine /
  BLE).
- **No fuzzing in CI yet** for the wire parsers (recommended for a framing-heavy
  protocol).
- **Group messaging is Signal/Megolm sender-keys**, not MLS — no post-compromise
  security or rekey-on-membership-change yet (tracked; the flagship next build).

## Recommended release posture

- Tag **`v0.1.0-alpha`** and publish it as a **GitHub pre-release**.
- Keep every "alpha / not audited / native radio not shipped" disclaimer in
  place across all surfaces.
- Invite **review and contribution**, not production deployment — the whole point
  of releasing now is to get the crypto and protocol looked at in the open, and
  to attract help finishing the radio layer.

## Open-source setup checklist

- [x] Apache-2.0 `LICENSE`
- [x] `README` with quickstart, honest status, and repo map
- [x] `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`
- [x] `SECURITY.md` with private vulnerability reporting
- [x] `MAINTAINERS.md`
- [x] `CHANGELOG.md` (Keep a Changelog) + first tagged version
- [x] Issue templates (bug, feature) + `config.yml` routing to Discussions/Security
- [x] Pull-request template
- [x] CI (fmt, clippy `-D warnings`, tests, acceptance sim, docker, saas, mobile)
- [x] Repository description + topics
- [x] Label taxonomy (area / type / priority / effort / status)
- [x] Seeded issues (roadmap + good-first-issues) and GitHub Discussions
- [ ] Third-party security audit *(pre-1.0 blocker)*
- [ ] Fuzzing in CI for wire parsers
- [ ] Native BLE radio backend on real hardware
