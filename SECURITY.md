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

## Location privacy threat model

Location ("find each other," SOS coordinates, shared POIs) is sensitive, so its
handling has its own rules:

- **Consensual.** A position is transmitted only in response to an explicit user
  action (tapping *Share*, sending an SOS). Nothing is sent in the background.
  Sharing is opt-in; the choice is remembered but never enabled silently.
- **Who can see it.** A location shared to a contact or a group is sealed to that
  recipient set exactly like any other message — the relay and passers-by see
  only ciphertext. Scope is honored end to end: `LocationGroup` reaches only that
  group's members; `Location` reaches one contact; `LocationAll` reaches every
  contact.
- **Bounded in time.** Shared positions **auto-expire** (`LIFELINE_LOCATION_TTL_SECS`,
  default 30 min): a receiving node drops a contact's fix once it lapses, so a
  one-time share can never become indefinite tracking. *Stop sharing* halts
  further updates from the sender; the last fix then expires within the TTL.
- **Geocast / area-addressed messages are NOT private against relays.** A geocast
  is addressed by a region (geohash) whose recipient key is derived deterministically
  from the region, so any relay can tell *which area* a geocast targets (though not
  its plaintext without the region key). Geocast is authenticated public regional
  broadcast, not a confidential channel — do not use it to hide *where* an alert is
  about. This caveat is intentional and documented in `crates/core/src/geocast.rs`.
- **Seizure / duress.** The panic wipe (G3) destroys on-disk secrets *and* clears
  cached location state held only in memory — contacts' last-known positions,
  shared POIs, and this node's own fix — so a coerced or seized device reveals
  nothing about who was where.

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
