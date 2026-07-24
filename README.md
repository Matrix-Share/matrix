# Project Lifeline — decentralized, offline-first emergency mesh

[![CI](https://github.com/OWNER/lifeline/actions/workflows/ci.yml/badge.svg)](.github/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

An open-source, self-hostable, end-to-end-encrypted mesh messenger that keeps
people connected when the internet and cellular networks fail — and "comes
alive" the moment any single node touches connectivity. Every phone is a node;
messages are carried, replicated, and relayed opportunistically (even by
physically moving devices) until they reach the recipient, with **cryptographic
proof of delivery and no blockchain**. Built from the design docs in
[`docs/`](docs/).

> "Kill the towers, keep one phone on data, and the whole room still messages
> out — with cryptographic proof of delivery."

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

## Testing / the acceptance simulator

```
cargo test                            # 127 tests across all crates
cargo run -p lifeline-sim --release   # runs the PRD acceptance scenarios + report
```

The GUI also exposes a **"Run network self-test"** button that executes the
3-cluster + data-mule acceptance scenario live and shows ≥95%-delivery results.

## Repository docs

| Doc | What |
|---|---|
| [`docs/`](docs/) | The original PRD + design docs (spectrum, network layer, gateway, OSS/papers). |
| [`STATUS.md`](STATUS.md) | Requirement-by-requirement (`FR-*`) implementation status. |
| [`GAPS.md`](GAPS.md) | Design-doc gap analysis + research-paper "what-to-improve" agenda. |
| [`INTEROP.md`](INTEROP.md) | How each listed OSS project migrates onto our seams (Reticulum, ggwave, BP7, Automerge, …). |
| [`CONTRIBUTING.md`](CONTRIBUTING.md) · [`SECURITY.md`](SECURITY.md) | How to contribute; how to report vulnerabilities. |

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
