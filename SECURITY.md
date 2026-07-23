# Security Policy

Project Lifeline is intended for use in emergencies, where a security flaw can
put people at real risk. We take vulnerabilities seriously.

## Reporting a vulnerability

**Please do not open a public GitHub issue for security vulnerabilities.**

Instead, use GitHub's private vulnerability reporting ("Report a vulnerability"
under the repository's *Security* tab), or email the maintainers listed in
`Cargo.toml`/`MAINTAINERS`. Include:

- a description and impact assessment,
- steps to reproduce or a proof of concept,
- affected crates/versions.

We aim to acknowledge within 72 hours and to provide a remediation timeline
after triage. We will credit reporters who wish to be named once a fix ships.

## Scope and threat model

The design threat model is documented in the PRD (`docs/PRD-*.md`, §15) and
`GAPS.md`. Areas of particular interest:

- **Framing/parsing** (`proto`, `transport::frame`) — untrusted-input parsers.
  This is where comparable mesh messengers (Bridgefy) were broken; see the
  CT-RSA 2021 / USENIX 2022 analyses referenced in `GAPS.md`.
- **Mesh handling** — impersonation, MITM, replay, custody, DoS/flooding.
- **The relay** — it must never be able to read or forge message content; it
  only forwards opaque ciphertext frames. Report anything that breaks this.

## What is explicitly out of scope

- Denial of service that requires physically jamming radios or overwhelming a
  device is inherent to the medium, not a software vulnerability.
- The system cannot deliver to a permanently unreachable recipient; that is
  physics, not a bug.

## Cryptography

We use only audited primitives (Ed25519, X25519, HKDF-SHA256,
XChaCha20-Poly1305, BLAKE3, Argon2id). Reports of primitive misuse, nonce reuse,
domain-separation gaps, or downgrade paths are especially welcome. An
independent audit is a pre-launch gate (PRD NFR-1).
