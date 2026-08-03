# Secure Software Development Lifecycle (SSDLC)

Lifeline is safety- and security-critical software. This document describes how
security is built into the development process — not bolted on — using
**open-source tooling that anyone can run and verify**. It is the internal
counterpart to the externally-facing [`SECURITY.md`](../SECURITY.md) (how to
report a vulnerability) and [`docs/RELEASE-READINESS.md`](RELEASE-READINESS.md)
(what "alpha" means).

## Principles (enforced in review — see [`CONTRIBUTING.md`](../CONTRIBUTING.md))

- **No custom crypto.** Compose audited primitives only.
- **Harden the framing layer.** Every parser of untrusted bytes is strict and
  bounded (the Bridgefy lesson); add fuzz targets for new parsers.
- **Offline-first, least-privilege.** No feature hard-depends on a server; the
  relay is optional and zero-knowledge.
- **Honesty about limits.** Security properties are documented with their exact
  boundaries; we never overstate what a mechanism buys.

## The lifecycle and its gates

| Phase | Practice | Tooling (all open source) |
|---|---|---|
| **Design** | Threat model each protocol change; document the security boundary in the PR. | Technical white paper, `SECURITY.md` threat model, PR template's "threat-model impact" section |
| **Code** | Secure-coding standards; no custom crypto; bounded parsers. | `cargo clippy -D warnings`, `rustfmt`, code review by a maintainer |
| **Static analysis (SAST)** | Scan every push/PR for vulnerable patterns. | **Semgrep** (Rust + JS/TS + Next.js rulesets), **CodeQL** (JS/TS → Security tab), Clippy |
| **Supply chain** | No known-vulnerable, yanked, unmaintained, or wrongly-licensed dependencies; only the official registry. | **cargo-audit** (RustSec), **cargo-deny** (`deny.toml`), **npm audit** (`--audit-level=high`), **Dependabot** |
| **Secrets** | No credentials in the repo or history. | **gitleaks** + GitHub native secret scanning + push protection |
| **Dynamic analysis (DAST)** | Scan the running app for header/cookie/info-leak/misconfig issues. | **OWASP ZAP** baseline (`dast-zap.yml`, scheduled + on demand) |
| **Fuzzing** | Fuzz the wire parsers (planned). | `cargo fuzz` (tracked in the issue tracker) |
| **Posture** | Continuously score the repo's OSS security posture. | **OpenSSF Scorecard** (`scorecard.yml` → Security tab + badge) |
| **Release** | Verify build + full test suite + a clean security run; tag; publish honest notes. | CI (`ci.yml`, `security.yml`), `CHANGELOG.md`, `RELEASE-READINESS.md` |
| **Respond** | Private vulnerability intake, triage, fix, disclose. | GitHub private advisories, `SECURITY.md` |

## Where it runs

- [`.github/workflows/security.yml`](../.github/workflows/security.yml) — cargo-audit, cargo-deny, Semgrep, npm audit, gitleaks. On push/PR + weekly.
- [`.github/workflows/codeql.yml`](../.github/workflows/codeql.yml) — CodeQL SAST for JS/TS.
- [`.github/workflows/scorecard.yml`](../.github/workflows/scorecard.yml) — OpenSSF Scorecard.
- [`.github/workflows/dast-zap.yml`](../.github/workflows/dast-zap.yml) — OWASP ZAP DAST (scheduled/manual).
- [`.github/dependabot.yml`](../.github/dependabot.yml) — automated dependency + action updates.
- [`deny.toml`](../deny.toml) — the supply-chain policy.

## Run it yourself

```bash
# SAST + supply chain (Rust)
cargo install cargo-audit cargo-deny
cargo audit
cargo deny check

# Dependencies (JS)
( cd saas   && npm audit --audit-level=high )
( cd mobile && npm audit --audit-level=high )

# SAST (multi-language) — needs semgrep (pip install semgrep)
semgrep scan --config p/security-audit --config p/rust --config p/javascript
```

## Dependency policy

- **Advisories are blocking.** A known RustSec/npm advisory at *high* or above
  fails CI and must be fixed (upgrade, override, or — with written justification
  and a tracking issue — an explicit, auditable ignore).
- **Prefer removing attack surface** over ignoring. Example: the optional
  Meshtastic MQTT bridge disables `rumqttc`'s bundled TLS (which pinned a
  vulnerable `rustls-webpki`); Lifeline payloads are already end-to-end sealed,
  so the MQTT hop is not a confidentiality boundary. See
  [`crates/bridge/Cargo.toml`](../crates/bridge/Cargo.toml).
- **Licenses** are restricted to permissive, Apache-2.0-compatible ones
  (`deny.toml`).

## Current state

At the last run: `cargo audit` reports **no vulnerabilities** (one transitive
*unmaintained* warning), and the `saas` and hardened `mobile` trees have **no
high/critical** npm advisories. The remaining pre-1.0 security work — a
**third-party audit** and **fuzzing in CI** — is tracked as issues.
