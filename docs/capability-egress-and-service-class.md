# Capability-based egress & service class

*How Lifeline decides "who may reach the internet, where, and at what tier"
through a mesh gateway — and why the answer is a portable token, not a server.*

This document is a design record for reviewers. It covers the motivation, a
survey of how the rest of the networking world solves the same problem, the
model we chose, the wire format, the threat model, and the known limits /
follow-ups. The implementation is [`crates/inet`](../crates/inet) (`lifeline-inet`).

---

## 1. The problem

The [internet-over-mesh gateway](reliable-transfer-and-internet.md) lets a node
with real internet act as an exit for others over the mesh. That immediately
raises an access-control question, and the requirement (from the project owner)
is precise:

> Ordinary **mesh messages are open** — any node relays any bundle. But actual
> **internet egress must be node-level authorized**: a gateway forwards to the
> internet *only* for identities it has granted, and refuses everyone else —
> while still relaying their mesh messages.

A concrete, real-world instance of the same shape is **in-flight Wi-Fi** (e.g.
KLM): free messaging to an allowlist of chat services, paid general internet,
and *no* VoIP even after paying. Three orthogonal gates:

1. **WHO** — is this session/identity entitled? (paid vs not)
2. **WHAT-destination** — which hosts may it reach? (messaging allowlist vs any)
3. **WHAT-class** — which kind of traffic? (web yes, real-time media no)

We want all three, but done in a way that works in a **disaster mesh**:
disconnected, partitioned, no central server reachable.

---

## 2. How the rest of the networking world does this

Every one of these systems answers "authorize a flow by identity, scope, class,
and cost." Each contributes one idea:

| Domain | Mechanism | Transferable idea |
|---|---|---|
| Cellular core (4G/5G) | APN + PCRF/PCC, **network slicing**, **5QI** classes | a flow carries a **service class** with a standard priority/latency budget; an isolated "life-safety slice" |
| eSIM / SIM | the SIM key *is* the entitlement | identity = credential, unspoofable |
| Zero Trust / BeyondCorp (NIST 800-207) | **PDP/PEP split**, per-request authz, identity-aware proxy | separate the decision from the enforcement; verify every request |
| Capability security (ocaps, **macaroons**, **Biscuit**) | signed, scoped, **offline-verifiable, attenuatable** tokens | put the authority *in the credential the requester carries* |
| Tor | each exit **advertises an exit policy** | gateways publish what they'll exit to; clients choose |
| SD-WAN / DiffServ | app-aware class → per-class policy; **token-bucket** policing | class-based (stateless) enforcement + quota |
| Anti-abuse / roaming | Hashcash postage, settlement | pay-to-egress so a stolen credential can't drain scarce backhaul |

**The decisive observation:** almost all of these keep the authority in a
*server you must reach* — the PCRF, the RADIUS box, the ZTNA policy engine, the
captive portal's AAA session. In a partitioned mesh there is no reachable
server, so those models fail exactly when the mesh matters most. The systems
that keep working while disconnected are the ones that put the authority **in a
portable, verifiable, attenuatable token**: capabilities, presigned URLs, Tor's
advertised policies.

So the load-bearing choice for Lifeline is **capabilities over allow-lists.**

---

## 3. The model we built (A + B)

### A. `ServiceClass` — the egress tier ("slice")

A four-level class labels *what kind* of internet egress a capability grants:

```
LifeSafety  ⊂  Messaging  ⊂  Interactive  ⊂  Bulk
(narrowest)                                  (broadest)
```

- **LifeSafety** — emergency-only egress (e.g. to official alert endpoints).
- **Messaging** — the free, zero-rated tier: egress only to an allowlist of
  messaging hosts (the in-flight-free-WhatsApp behaviour).
- **Interactive** — general web browsing.
- **Bulk** — large downloads/streaming, the most metered.

`ServiceClass` is **orthogonal** to [`Priority`](../crates/proto/src/types.rs)
(SOS/Alert/Normal/Bulk). `Priority` is a *mesh-scheduling* axis (which bundle
moves first, how much anti-spam postage it pays). `ServiceClass` is an *egress*
axis (what the exit will fetch). A high-priority mesh bundle can carry a
messaging-class egress request; life-safety mesh traffic needs no egress
capability at all, because it never leaves the mesh.

### B. Capabilities — the egress authority, as a token

A **`Capability`** is a signed, scoped grant that the requester **presents** with
each `NetRequest`. It is the [macaroon] / [Biscuit] design, built on the Ed25519
we already ship. Three properties, each essential for a mesh:

1. **Offline verification.** The token carries the issuer's public key and a
   signature over the grant. A gateway that trusts that issuer verifies with a
   signature check + local clock — no network, no shared per-identity state,
   works across a partition.
2. **Attenuation (narrow-only delegation).** The holder can append a `Scope`
   *caveat* that further restricts the grant (fewer hosts, tighter expiry,
   smaller byte cap) and pass it on **without contacting the issuer**. The
   verifier ANDs the root grant with *every* caveat, so an appended caveat can
   only ever *remove* authority — a maliciously "widening" caveat is a no-op.
3. **Unforgeable, unstrippable chain.** Each attenuation is signed by the key the
   previous level delegated to and binds the previous signature, so caveats can't
   be reordered, spliced, or stripped without breaking the chain.

This is what lets an authorization chain form *across the mesh*:

```
Relief authority  ── signs ──▶  "identity X may use gateways · 7-day expiry"
        │  (delegates to X's key)
        ▼
Gateway operator  ── attenuates ──▶  "…through me · only *.who.int · 50 MB/day"
        │
        ▼
Neighbour sharing uplink ── attenuates ──▶  "…GET only · today only"
```

All three AND-compose and verify with one signature check per level.

[macaroon]: https://research.google/pubs/pub41892/
[Biscuit]: https://www.biscuitsec.org/

### The two policies that ship

`AccessPolicy::decide(Authz) -> Decision` is the **Policy Decision Point (PDP)**;
`InternetGateway::handle` is the enforcement point (PEP). Two implementations:

- **`AllowList`** — the "trivial local issuer": an operator-maintained set of
  identities, each granted full `Bulk` egress; the presented capability is
  ignored. Back-compatible, good for a single-operator gateway.
- **`CapabilityPolicy`** — the real model: the gateway holds only a set of
  **trusted issuer public keys**, and authorizes each request strictly to the
  presented capability's (attenuated) scope.

---

## 4. Wire format

```
Capability {
  grant:    Grant,          // the root, signed by the issuer
  root_sig: [u8; 64],       // Ed25519(issuer, grant_signing_msg(grant))
  atts:     [Attenuation],  // zero or more narrowing steps
}

Grant {
  v: 1,
  subject:     Address,     // the identity permitted to exercise it
  issuer:      [u8; 32],    // issuer Ed25519 public key
  scope:       Scope,       // what the root authorizes
  delegate_to: [u8; 32],    // key permitted to append the FIRST attenuation
  nonce:       [u8; 16],    // uniqueness + stable id
}

Attenuation {
  caveat:      Scope,       // additional restriction (ANDed)
  delegate_to: [u8; 32],    // key permitted to append the NEXT attenuation
  sig:         [u8; 64],    // Ed25519(prev delegate, att_signing_msg(...))
}

Scope {
  hosts:              HostRule,      // Any | Only([HostPattern])
  methods:            MethodRule,    // Any | Only([String])
  class:              ServiceClass,
  max_response_bytes: Option<u64>,   // per-response ceiling; effective = min
  not_after:          Option<u64>,   // unix seconds; effective = earliest
}
```

**Signing messages** are domain-separated and injective:

- `grant_signing_msg  = "lifeline/inet/cap-grant/v1" || CBOR(grant)`
- `att_signing_msg    = "lifeline/inet/cap-attenuation/v1" || CBOR(caveat) || delegate_to[32] || prev_tag[64]`

`prev_tag` is the previous level's signature (the root signature for the first
attenuation). The two fixed-size fields are a suffix, so the encoding is
unambiguous. Verification walks the chain: verify `root_sig` under
`grant.issuer` (which must be trusted), then each `att.sig` under the previous
level's `delegate_to`.

**Evaluation** of a request `(subject, host, method, now)`: verify the chain,
require `subject == grant.subject`, then require the root scope **and every
caveat** to permit `(host, method, now)`. The effective class is the minimum
privilege across the chain; the effective byte ceiling is the tightest present.

---

## 5. Threat model & what each control stops

| Threat | Control | Test |
|---|---|---|
| Unauthorized user egresses | policy `Deny` → refused **without fetch** | `unauthorized_request_is_refused_without_fetching` (`calls == 0`) |
| Forged / tampered capability | Ed25519 over the grant; strict verify | `forged_signature_is_rejected` |
| Capability from an unknown issuer | issuer must be in the trusted set | `untrusted_issuer_is_rejected`, `capability_policy_refuses_missing_or_untrusted_capability` |
| Stolen capability replayed by another identity | `subject` binding | `capability_is_bound_to_its_subject` |
| Holder tries to *widen* scope via a caveat | verifier ANDs all levels (monotone) | `attenuation_narrows_and_cannot_widen` |
| Attacker forges an attenuation | each caveat signed by the delegated key | `attenuation_by_wrong_key_is_refused` |
| Attacker splices/strips caveats | signature chain binds `prev_tag` | `stripped_attenuation_breaks_the_chain` |
| Stale grant | `not_after` expiry | `expired_capability_is_refused` |
| Egress to internal infra (SSRF) | `is_safe_url` (http/https only; no localhost/private/link-local/CGNAT/cloud-metadata) | `ssrf_targets_are_refused_even_for_authorized_requesters` |
| Oversized response drains backhaul | per-response `max_response_bytes` ceiling | `capability_byte_ceiling_is_enforced_after_fetch` |
| Real-time calling (VoIP) | **structural** — no representation in a request/response DTN; schemes limited to http/https | `voip_style_schemes_have_no_representation` |
| Works while partitioned | offline signature verification, no server | `verification_is_offline_and_survives_a_partition` |

**Trust boundary the model does *not* remove:** the gateway is the exit, so it
necessarily sees the plaintext request it fetches (URL, headers) — exactly like a
VPN/Tor exit or your ISP. Users choose which gateway to trust; relays in between
see only a sealed bundle bound for the gateway, never the URL. This is stated
plainly rather than implied.

**Now implemented (was a follow-up):**

- **Live cross-request quota (token bucket).** A `Scope` can carry a
  `max_total_bytes` cumulative cap (effective = min across the chain). The
  gateway keeps a `QuotaLedger` keyed by `Capability::id`; it refuses a request
  **before fetching** once the capability is at/over its cap, and meters served
  bytes after each response. Total spend is bounded to roughly `quota + one
  response`. This is the token-bucket "data pass" that stops a stolen or
  over-eager credential draining a gateway's scarce backhaul. Test:
  `cumulative_quota_is_metered_and_then_exhausts`.
- **Revocation before expiry.** `CapabilityPolicy::revoke(cap_id)` is the
  break-glass control: a revoked capability is refused (without fetching) even if
  it otherwise verifies and is unexpired. Short expiry remains the primary
  mechanism; this handles "revoke *now*." Test:
  `revoked_capability_is_refused_without_fetching`.

**Still not in scope (documented, not silently missing):**

- **Third-party caveats** (macaroon-style "valid only if service Y also
  authorizes") are not implemented; only first-party (locally-checkable) caveats.
- **Format review.** The token is a composition of audited primitives (Ed25519,
  BLAKE3/CBOR canonicalisation), not a new primitive — but the *composition*
  should get the same independent security review as the rest of the protocol
  before launch (project NFR-1).

---

## 6. How it plugs into the gateway

```
requester ─ NetRequest + Capability ─▶ InternetGateway::handle(req, cap, now)
                                          │  policy.decide(Authz)  ← PDP
                                          │      Deny → refuse (no fetch)
                                          │      Allow{class, max_bytes}
                                          │  is_safe_url(req.url)?  ← SSRF guard
                                          │  fetcher.fetch(req)     ← real HTTP (feature)
                                          │  body > max_bytes? → refuse
                                          ▼
requester ◀──────────── NetResponse ─────┘
```

The `Fetcher` is a trait; tests use a mock that records call counts (so "refused
without fetching" is *proven*, not asserted). The real `reqwest`/`ureq` fetcher
and the client-side local proxy are the integration follow-up.

---

## 7. Follow-ups (the C/D/E of the broader design)

- **C — richer PDP:** fold node reputation/posture into `CapabilityPolicy`
  (a gateway can refuse egress to an identity our black-hole attribution has
  demoted) without touching the PEP.
- **D — advertised exit policies + discovery:** each gateway publishes a signed
  exit-policy descriptor into the [DHT](../crates/dht); clients select a gateway
  whose advertised class/host policy matches their need (Tor's model,
  decentralised onto infrastructure we already have).
- **E — postage / token-bucket:** spend against the capability's quota (and
  optionally a small proof-of-work, reusing [`proto::pow`](../crates/proto/src/pow.rs))
  so a compromised credential can't drain a gateway's scarce backhaul.

See [`reliable-transfer-and-internet.md`](reliable-transfer-and-internet.md) for
the transport side (multi-carrier reliability and the swarm-fetch gap).
