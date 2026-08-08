<p align="center">
  <img src="website/logo-wordmark.svg" alt="Lifeline" width="340">
</p>

<p align="center"><b>Messaging that works when the network doesn't.</b></p>

<p align="center">
  <a href="https://matrix-alpha-ashen.vercel.app">Website</a> ·
  <a href="WHITEPAPER.md">Whitepaper</a> ·
  <a href="#how-lifeline-compares">How it compares</a> ·
  <a href="docs/USE-CASES.md">Use cases</a> ·
  <a href="https://github.com/matrix-share/matrix/discussions">Discussions</a>
</p>

<p align="center">
  <a href="https://github.com/matrix-share/matrix/actions/workflows/ci.yml"><img src="https://github.com/matrix-share/matrix/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/matrix-share/matrix/actions/workflows/security.yml"><img src="https://github.com/matrix-share/matrix/actions/workflows/security.yml/badge.svg" alt="Security"></a>
  <a href="https://scorecard.dev/viewer/?uri=github.com/matrix-share/matrix"><img src="https://api.securityscorecards.dev/projects/github.com/matrix-share/matrix/badge" alt="OpenSSF Scorecard"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-Apache_2.0-blue.svg" alt="License: Apache-2.0"></a>
</p>

<!-- 🎬 Demo GIF goes here once recorded (#109) -->
<p align="center"><sub>🎬 A short phone-to-phone demo is coming — track it in <a href="https://github.com/matrix-share/matrix/issues/109">#109</a>.</sub></p>

An open-source, self-hostable, end-to-end-encrypted mesh messenger that keeps
people connected when the internet and cellular networks fail — and "comes
alive" the moment any single node touches connectivity. Every phone is a node;
messages are carried, replicated, and relayed opportunistically (even by
physically moving devices) until they reach the recipient, with **cryptographic
proof of delivery and no blockchain**. Built from the design docs in
[`docs/`](docs/).

> "Kill the towers, keep one phone on data, and the whole room still messages
> out — with cryptographic proof of delivery."

> [!IMPORTANT]
> **Status: alpha, and not yet independently security-audited.** The cryptography
> and protocol are implemented and unit-tested, but they have not had a
> third-party review — **don't rely on Lifeline for high-risk or life-safety
> communication yet.** Also note **what runs today**: nodes mesh over a local
> WebSocket **relay** (or LAN/UDP), which stands in for the internet transport so
> browsers and servers can connect. The native phone-to-phone radio bearers
> (Bluetooth LE / Wi-Fi Aware / ultrasound) are designed but **not shipped** — so
> "works with no internet at all, phone-to-phone" is the goal, not yet the
> out-of-the-box reality. Everything below runs and is real.

## Quickstart — chat with someone in 60 seconds

**Docker (self-hosted, recommended):**

```bash
docker compose up --build
# open two tabs:  http://localhost:8080 (Asha)   http://localhost:8081 (Ravi)
# they auto-discover each other — type a message; it's E2E-encrypted with a delivery receipt
```

**From source:**

```bash
make relay &                                             # zero-knowledge hub on :7000
LIFELINE_NODE_ADDR=127.0.0.1:8080 LIFELINE_NAME=Asha cargo run -p lifeline-node &
LIFELINE_NODE_ADDR=127.0.0.1:8081 LIFELINE_NAME=Ravi cargo run -p lifeline-node &
# open http://127.0.0.1:8080 and :8081
```

The **relay is optional infrastructure and zero-knowledge** — it only forwards
opaque ciphertext frames. On real devices, nodes mesh directly over
BLE/Wi-Fi Aware/ultrasound; the relay just stands in for the internet transport
so browsers can connect.

## How Lifeline compares

Other tools solve pieces of this. Lifeline's niche is **carrying a message across
people and gaps with no infrastructure**, then **bridging to the internet the
moment any node can**.

| Capability | **Lifeline** | Briar | bitchat | Meshtastic | Nostr |
|---|:--:|:--:|:--:|:--:|:--:|
| Works with no internet or cell | ✅ | ✅ | ✅ | ✅ | ❌ |
| Phone-to-phone, no extra hardware | ✅ | ✅ | ✅ | ❌ *LoRa* | ❌ |
| Carries across gaps (delay-tolerant) | ✅ | ⚠️ | ⚠️ | ⚠️ | ❌ |
| End-to-end encrypted | ✅ | ✅ | ✅ | ⚠️ *channel* | ⚠️ *DMs* |
| No account or phone number | ✅ | ✅ | ✅ | ✅ | ✅ |
| Group messaging | ✅ | ✅ | ✅ | ✅ | ⚠️ |
| SOS + live location | ✅ | ❌ | ❌ | ✅ | ❌ |
| Opportunistic internet bridge | ✅ | ⚠️ *Tor* | ⚠️ *Nostr* | ✅ *MQTT* | *is the internet* |
| Open source | ✅ | ✅ | ✅ | ✅ | ✅ |

✅ full · ⚠️ partial / conditional · ❌ not supported. Reflects each project's
typical capabilities as of 2026; meant to be fair, not exhaustive — all of these
are worthwhile projects. Corrections welcome via
[an issue](https://github.com/matrix-share/matrix/issues).

## Apps in this repo

Lifeline is one product across several surfaces. The **mesh + messenger is the
core** (Rust, above); the rest are optional layers that share one design system.

| Surface | Path | What it is | Run it |
|---|---|---|---|
| **Mesh node + web app** | `crates/`, `crates/node/web/` | The Rust engine + a browser messenger it serves. This is the product. | `docker compose up --build`, or the "From source" steps above |
| **Mobile app** | [`mobile/`](mobile/) | Native iOS/Android client (Expo + React Native) that talks to a node's API. | `cd mobile && npm install && npx expo start` |
| **SaaS** | [`saas/`](saas/) | Hosted layer — marketing site, accounts, dashboard, teams, Stripe billing (Next.js). The mesh stays accountless; this sits on top. | `cd saas && npm install && npm run dev` → http://localhost:3000 |
| **Marketing site** | [`website/`](website/) | Static landing page. | open `website/index.html`, or serve the folder with any static server |

The shared visual language lives in [`docs/design/design-system.md`](docs/design/design-system.md).
Each app has its own README with details.

## Testing / the acceptance simulator

```
cargo test                            # 280 tests across all crates
cargo run -p lifeline-sim --release   # runs the PRD acceptance scenarios + report
```

The GUI also exposes a **"Run network self-test"** button that executes the
3-cluster + data-mule acceptance scenario live and shows ≥95%-delivery results.

## Repository docs

| Doc | What |
|---|---|
| [`WHITEPAPER.md`](WHITEPAPER.md) | **Plain-English white paper** — what Lifeline is, who it's for, and how it works, with no jargon. Start here if you're new. |
| [`docs/USE-CASES.md`](docs/USE-CASES.md) | **Where Lifeline helps** — the real-world situations it's built for (disasters, shutdowns, crowds, the backcountry, privacy, and more), each mapped to the feature that answers it. |
| [`docs/ROADMAP-location.md`](docs/ROADMAP-location.md) | **Roadmap** — the checklist to make the location / "find each other" story production-real and keep docs + messaging consistent across every surface. |
| [`docs/ble-transport.md`](docs/ble-transport.md) | **BLE transport** — how the phone-to-phone Bluetooth bearer is built (ATT segmentation + the radio seam), the platform matrix, and what's tested vs. needs hardware. |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | **How the system fits together** — layers, crates, the extension seams, message flow, threading, and the roadmap. Start here to understand the codebase. |
| [`docs/`](docs/) | The original PRD + design docs (spectrum, network layer, gateway, OSS/papers). |
| [`STATUS.md`](STATUS.md) | Requirement-by-requirement (`FR-*`) implementation status. |
| [`GAPS.md`](GAPS.md) | Design-doc gap analysis + research-paper "what-to-improve" agenda. |
| [`INTEROP.md`](INTEROP.md) | How each listed OSS project migrates onto our seams (Reticulum, ggwave, BP7, Automerge, …). |
| [`ROADMAP.md`](ROADMAP.md) | **Where the project is headed** — the big rocks (making the offline promise real, hardening, growth), grouped from the issue tracker. |
| [`CONTRIBUTING.md`](CONTRIBUTING.md) · [`SECURITY.md`](SECURITY.md) · [`MAINTAINERS.md`](MAINTAINERS.md) | How to contribute; how to report vulnerabilities; who maintains the project. |
| [`docs/RELEASE-READINESS.md`](docs/RELEASE-READINESS.md) | **Is it ready to use?** An honest cross-check of what works, what doesn't, and what "alpha" means here. Read before relying on it. |
| [`docs/SSDLC.md`](docs/SSDLC.md) | **Secure development lifecycle** — how security is built into the process: SAST (Semgrep/CodeQL), supply-chain (cargo-audit/deny, npm audit, Dependabot), secret scanning, DAST (OWASP ZAP), and OpenSSF Scorecard, all with open-source tools. |

**Modular across networks:** every physical channel (BLE, Wi-Fi Aware,
ultrasound, optical, LoRa, internet) is a plug-in implementing one [`Interface`](crates/transport)
contract. The [`NodeEngine`](crates/transport) runs any number of them
concurrently and carries the *same* encrypted bundle over each, fragmenting to
each link's MTU — so one message can hop BLE → ultrasound → LoRa → internet
without the app knowing.

## What this is (and isn't) yet

This is **Phase 1 — the decentralized core** (PRD §7 layers L2 + L4 + L5, and the
schemas of §11 / protocols of §12). It is the part that must be correct before
any radio or UI is worth building, and it is validated by a network simulator
that proves the PRD's headline acceptance criterion:

> **NFR-3 / FR-29 AC** — under a simulated 3-cluster partition bridged only by
> one moving data mule, **≥95% of messages eventually deliver**. This build
> achieves **100% delivery and 100% verified delivery** on the reference
> scenario (see `cargo run`).

On top of that core there is now a **runnable app** (web GUI + node daemon +
zero-knowledge relay), a **real UDP/LAN transport** that meshes with no relay,
PoW anti-abuse, CRDT sync, reputation-based black-hole avoidance,
**erasure/fountain coding** (any k-of-n fragments reconstruct — survives partial
carrier escape), and hardened parsers. What's **not** here yet: the native **radio backends** (BLE / Wi-Fi
Aware / ggwave ultrasound / RNode LoRa) behind the finished `Interface` seam, a
native mobile app, and an independent security audit. See [`STATUS.md`](STATUS.md)
for a requirement-by-requirement traceability table and [`GAPS.md`](GAPS.md) for
what's next.

## Workspace layout

Mirrors the PRD §13.1 monorepo, as Rust crates:

| Crate | PRD layer | Responsibility |
|---|---|---|
| [`crates/proto`](crates/proto) | wire format | Versioned §11 schemas + canonical CBOR + base64url. No crypto, no I/O. |
| [`crates/core`](crates/core) | **L2 + L5** | Identity (Ed25519/X25519), sealed-sender E2E crypto, hash-linked logs, signed delivery receipts, **offline** delivery verification. |
| [`crates/router`](crates/router) | **L4** | DTN store-carry-forward, binary spray-and-wait, TTL/hop/dedup, strict priority (SOS first), gateway announce + gradient, store cap + eviction, **PoW-postage admission gating** (FR-46). |
| [`crates/sync`](crates/sync) | **L5** | CRDTs — version vectors, OR-Set (ORSWOT, add-wins), LWW registers — composed into shared state (group membership, blocklists, presence, delivery status) that merges conflict-free after partitions (FR-33), with causal-stability GC (Shapiro). |
| [`crates/transport`](crates/transport) | **L0 + L1** | The `Interface` transport contract, MTU-aware fragmentation/reassembly, in-process medium + BLE/Wi-Fi Aware/ultrasound/optical/LoRa/internet caps, a **real `UdpInterface`** (multicast/LAN, no relay) and `ChannelInterface` (relay), and the multi-interface `NodeEngine` runtime (FR-16..22). |
| [`crates/relay`](crates/relay) | app | Zero-knowledge TCP hub that forwards opaque ciphertext frames between nodes (the internet-gateway fabric). |
| [`crates/node`](crates/node) | app | The node daemon: runs the engine, connects to a relay, and serves the web GUI + HTTP/WS API. |
| [`crates/sim`](crates/sim) | `/sim` | Deterministic simulator: clusters, data mules, internet fabric, CRDT anti-entropy; runs the acceptance scenarios and measures delivery + proof + convergence. |

Dependency direction: `{node} → {transport, sim, relay} → {core, router, sync} → proto`. The router is
transport-agnostic and treats ciphertext as opaque (FR-38); the core is the only
place plaintext ever exists (endpoints only).

## The message lifecycle (PRD §12.1), as code

1. **Seal** — `core::message::seal_bundle`: CBOR-encode the payload, encrypt it
   to the recipient's X25519 key (ephemeral-static sealed box), seal the sender
   identity to the recipient (**sealed sender**, FR-45), and sign the immutable
   header (Ed25519). Mutable fields (`hops`, `copies_left`) are excluded from the
   signature so relays can update them.
2. **Spray & carry** — `router::DtnRouter`: binary spray-and-wait bounds the copy
   budget; nodes hold bundles until a contact appears; a moving node physically
   carries them (data mule).
3. **Bridge** — an internet gateway drains non-local bundles onto its uplink; the
   fabric delivers them into every other gateway, which meshes them onward
   (FR-37, "one gateway lights the mesh").
4. **Open** — `core::message::open_bundle`: unseal the sender, **verify the
   header signature with the real sender key** (defeats impersonation/MITM,
   FR-6 AC), then decrypt the payload.
5. **Receipt** — `core::receipt::make_delivery_receipt`: the recipient signs
   `bundle_id || delivered_at`; the receipt diffuses back as its own bundle.
6. **Verify** — `core::receipt::verify_delivery`: a pure, **fully offline**
   function of `(bundle, receipt, sender_pub, recipient_pub)` → the sender
   reaches `delivered(verified)` with no server and no ledger (§12.4).

## Cryptography

Composed entirely from audited primitives (PRD §15 gate — *no custom crypto*):

- **Ed25519** (`ed25519-dalek`) — identity signatures, headers, receipts, logs.
- **X25519** (`x25519-dalek`) — key agreement / sealed box.
- **HKDF-SHA256** + **XChaCha20-Poly1305** — key derivation + AEAD.
- **BLAKE3** — addresses (`blake3(sign_pub)[..16]`), bundle ids, log links.
- **Argon2id** — passphrase-wrapped encrypted key backup (FR-5).

**Engineering note (PRD OQ3):** the PRD names "X3DH + Double Ratchet". The Double
Ratchet's security proof assumes roughly in-order delivery, which a delay-tolerant
network with multi-day one-way delays and reordering violates. Phase 1 therefore
ships a **stateless ephemeral-static sealed box** behind the
`core::crypto::SecureChannel` seam — forward-secret against later ephemeral-key
compromise, authenticated, sealed-sender, and tolerant of arbitrary reordering. A
DTN-tuned ratchet can implement the same trait later without touching callers.

## Running the acceptance simulator

```
cargo run -p lifeline-sim --release
```

Prints, for each scenario, `sent / delivered / verified` with the ≥95% target
highlighted, plus spray overhead and a whole-world hash-linked-log integrity
check. Scenarios: 3-cluster + mule (UC2/UC3/NFR-3), one-gateway bridge (UC4), SOS
preemption (UC5), a dense 60-node cluster with no broadcast storm (NFR-6),
**CRDT group-membership convergence across a partition** (FR-33), and **PoW
postage throttling a spam flood without delaying SOS** (FR-46).

## Toolchain

Rust stable (built and tested on 1.97). No external services or network access
required to build, test, or run.
