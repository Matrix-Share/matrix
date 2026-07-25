# OSS interoperability & capability-migration map

The design docs (`component-reference-oss-papers.html`, network-layer, spectrum,
gateway) name ~20 open-source projects across the seven layers. This document
maps **each** to (a) the capability we want from it, (b) the exact seam in our
codebase it plugs into, (c) its license vs. our Apache-2.0 project, and (d) a
concrete *migrate* (reimplement behind the seam) or *interop* (bridge to the real
project) plan.

## Why migration is mostly "implement a trait"

Our architecture was deliberately built as a set of narrow seams so external
capabilities drop in without touching callers:

| Seam | Trait / module | What plugs in |
|---|---|---|
| **L0/L1 transport** | `transport::Interface` | any radio/medium: BLE, ggwave, Meshtastic-LoRa, Reticulum TCP, libp2p streams |
| **L2 E2E channel** | `core::crypto::SecureChannel` | sealed box (now), Signal Double Ratchet, Noise, MLS |
| **L4 routing** | `router::DtnRouter` API | BP7 custody semantics, Reticulum routing |
| **L5 CRDTs** | `sync` (ORSWOT/LWW/VV) | Automerge/Yjs document types |
| **L5 logs** | `core::log::HashLog` | SSB-style feeds |
| **L6 anti-abuse** | `proto::pow`, router admission | Hashcash, GNUnet-style reputation |
| **App transport fabric** | `lifeline-relay` + `ChannelInterface` | any internet relay, Reticulum transport node, **Nostr relays** (see below) |

So "migrate capability X" almost always means **implement one trait** or **write
one bridge** — not rearchitect.

## License compatibility (practical OSS need)

We are **Apache-2.0**. Compatibility summary for the named projects:

- **Permissive, safe to depend on / vendor:** Reticulum (MIT), ggwave (MIT),
  Quiet (MIT), Automerge (MIT), Yjs (MIT), libsodium (ISC), age (BSD-3),
  IPFS/kubo (MIT/Apache-2.0), libp2p (MIT/Apache-2.0), dtn7-go (Apache-2.0/MIT).
- **Copyleft — bridge/interop only, do NOT statically link into our binary:**
  libsignal (AGPL-3.0), GNUnet (AGPL-3.0), Meshtastic firmware (GPL-3.0 — it runs
  on the *device*, we talk to it over serial/BLE, which is fine),
  Matrix Olm/Megolm (Apache-2.0 actually — safe), ION-DTN (custom/NASA — check).
- **Rule of thumb:** GPL/AGPL projects are integrated as **separate processes we
  talk to over a protocol** (serial, TCP, IPC), never linked. This keeps our
  Apache-2.0 licensing clean while still using their capability.

---

## Layer 0–1 — Transports & interface abstraction

| Project (license) | Capability we want | Seam | Plan |
|---|---|---|---|
| **Reticulum / RNS** (MIT) | Transport-agnostic addressing, announces, transport-node bridging, LXMF DTN messaging | `Interface` + `router` + `relay` | **Interop + selective migrate.** Our announce/gradient (`router::gateway`) and identity-as-address already mirror RNS. Action: add a `ReticulumInterface` (TCP/`RNode`) implementing `Interface`, and an LXMF↔bundle adapter so we can join existing Reticulum networks. |
| **Meshtastic firmware** (GPL-3.0) | Real LoRa mesh radio (IN865) | `Interface` (out-of-process) | **Interop.** Node pairs with a Meshtastic/RNode device over BLE/serial; a `LoRaInterface` speaks its frame API. GPL stays on-device — we only talk to it. Caps already model `lora_in865` (power/region enforced). |
| **ggwave** (MIT) | Data-over-sound (near-ultrasound) | `Interface` | **Migrate/vendor.** Wrap ggwave (C, has bindings) behind an `UltrasoundInterface`. Our `ultrasound()` caps + MTU fragmentation are already sized for it (128 B frames). |
| **Quiet** (MIT) | Alternate data-over-sound modem | `Interface` | **Interop.** Second `UltrasoundInterface` backend; pick per-platform. |
| **sudomesh/disaster-radio** (varies) | Solar LoRa relay node blueprint | Gateway deployment | **Reference.** Informs the pre-positioned solar gateway hardware (Problem C); our gateway/`bridges_offmesh` model targets it. |

## Layer 2 — Identity & E2E encryption

| Project (license) | Capability | Seam | Plan |
|---|---|---|---|
| **libsignal** (AGPL-3.0) | Double Ratchet + X3DH + sealed sender | `SecureChannel` | **Migrate the *design*, not the code.** AGPL blocks linking. We already implement sealed-sender + a `SecureChannel` seam; a DTN-tuned ratchet (OQ3) will be an independent Apache-2.0 implementation of the *published* Double Ratchet, not a libsignal import. |
| **Matrix Olm/Megolm** (Apache-2.0) | Group session (sender-keys) E2E | `SecureChannel` + `sync` membership | **Migrate.** License-compatible. Basis for FR-12 sender-keys group encryption on top of our converged CRDT membership. |
| **Noise Protocol** (public spec) | Simpler handshake framework for links | `SecureChannel` | **Migrate.** Candidate for an authenticated link handshake between adjacent nodes; snow (Rust, MIT/Apache) is a ready dependency. |
| **libsodium** (ISC) | Vetted primitives | `core::crypto` | **Adopted (equivalent).** We use RustCrypto/dalek equivalents; libsodium (via `sodiumoxide`) is a drop-in alternative if desired. No custom crypto. |
| **age** (BSD-3) | File/stream encryption | attachments (FR-13) | **Interop.** For encrypting large attachments/cached content at rest before chunking. |

## Layer 3 — Peer discovery & naming

| Project (license) | Capability | Seam | Plan |
|---|---|---|---|
| **libp2p Kademlia DHT** (MIT/Apache) | Online peer/route discovery by XOR distance | new `discovery` module | **Interop/migrate.** `Address::xor_distance` already implements the metric. Add a `libp2p-kad` backend for the *online* discovery path; pair with our offline gossip announces + reputation (Kademlia's Sybil/eclipse weakness — see `GAPS.md`). |
| **BitTorrent Mainline DHT** | Proven large-scale DHT | discovery | **Reference** for scale/robustness tuning. |
| **Reticulum announce/path** (MIT) | Offline gossip discovery | `router::gateway` announces | **Adopted (equivalent).** Our signed gateway announces + gradient are this pattern; interop via the `ReticulumInterface`. |

## Layer 4 — Delay-tolerant routing & delivery

| Project (license) | Capability | Seam | Plan |
|---|---|---|---|
| **dtn7-go** (Apache/MIT) | RFC 9171 Bundle Protocol 7 (bundles, custody) | `router` + `proto` | **Interop.** Our bundle already carries BP7 concepts (custody schema, TTL, priority, dedup). Action: add a BP7 endpoint-ID mapping + a `bpv7` codec so we can exchange bundles with dtn7 nodes (interop & credibility). |
| **NASA ION-DTN** (custom) | Reference DTN for challenged links | `router` | **Reference/interop** for space/agency-grade deployments; check license before any code reuse. |
| **IBR-DTN** (Apache-2.0) | Embedded DTN daemon | `router` | **Reference** for constrained-device tuning. |
| **reed-solomon-erasure** (MIT) | MDS erasure code | `core::erasure` | **Adopted** (FR-28): any k-of-n fragments reconstruct a message — the Problem-C "partial escape" answer. |

## Layer 5 — Integrity, logs & CRDTs

| Project (license) | Capability | Seam | Plan |
|---|---|---|---|
| **Automerge** (MIT) | Rich JSON CRDT documents | `sync` | **Migrate/interop.** Our ORSWOT/LWW cover membership/blocklists/presence/delivery. For richer shared docs (group metadata, forms), add `automerge` (Rust-native) behind the `sync` API; keep our GC discipline. |
| **Yjs** (MIT) | High-perf CRDT (JS) | web GUI | **Interop.** For collaborative in-browser state if the GUI grows; complements Automerge on the Rust side. |
| **Secure Scuttlebutt** (varies) | Signed hash-linked append-only feeds + gossip sync | `core::log` | **Adopted (equivalent).** `HashLog` *is* an SSB-style feed; we added checkpoint compaction for its "grows forever" problem (Tarr). Interop: an SSB feed export/import adapter. |
| **IPFS / kubo + IPLD** (MIT/Apache) | Content-addressed Merkle-DAG blocks | new `content` module (FR-13) | **Migrate the model.** Add content-addressed block storage (BLAKE3 CID) for large attachments and cached alerts/pages, fetched by hash — exactly Benet's model. Optional real IPFS gateway bridge when online. |

## Layer 6 — Trust, anti-abuse & incentives

| Project (license) | Capability | Seam | Plan |
|---|---|---|---|
| **Hashcash** (public) | PoW "postage" to throttle spam | `proto::pow` | **Adopted.** Implemented (FR-46), enforced at router admission, difficulty by priority, SOS-exempt. |
| **GNUnet** (AGPL-3.0) | Reputation & F2F/P2P trust | new `reputation` module | **Migrate the design.** AGPL blocks linking; implement an Apache-2.0 reputation-gossip that demotes black-hole relays (Kademlia/Douceur answer, `GAPS.md`), informed by GNUnet's approach. |
| **Helium** (varies) | Proof-of-relay incentive (DePIN) | optional off-path ledger | **Design adopted** (`core::relay_proof`): credits are signed by the *counterparty* next hop, so a relay cannot self-mint evidence — directly answering the location-spoofing that hurt Helium. Kept strictly *off* the delivery path. |

---

## Nostr — the internet fabric to plug into (highest-leverage interop)

Not in the original design docs, but the clearest win: **Nostr relays are a
global, already-adopted, store-and-forward internet fabric** (signed events on
"dumb relays, smart clients"). BitChat (BLE mesh + Nostr bridge) proves the
mesh↔Nostr pattern at scale. A **`NostrInterface`** (a `ChannelInterface` fed by
a Nostr WebSocket task) gives Lifeline nodes global internet reach + offline
mailboxing with **no engine change and no `lifeline-relay` to run** — while
Lifeline leads with what Nostr lacks (forward secrecy, verifiable delivery,
multi-bearer offline, erasure). Full plan, NIP mapping, and the bundle↔event
codec: [`docs/nostr-integration.md`](docs/nostr-integration.md).

## Priority adoption order (verifiable next)

1. **ggwave `UltrasoundInterface`** + **Meshtastic `LoRaInterface`** — makes the
   `Interface` seam real on hardware; both permissively licensed / out-of-process.
2. **BP7 interop codec** (dtn7) — standards credibility + interop, pure Rust.
3. **Automerge in `sync`** — richer shared docs behind the existing CRDT API.
4. **Reputation module** (GNUnet-inspired, clean-room) — closes the Sybil/
   black-hole gap alongside PoW.
5. **Content-addressed blocks** (IPFS model) — unlocks large attachments (FR-13).
6. **Reticulum interface + LXMF adapter** — join existing Reticulum networks.

Each is a self-contained PR behind an existing seam, keeps our Apache-2.0
licensing clean, and is testable in `crates/sim` or an integration test.
