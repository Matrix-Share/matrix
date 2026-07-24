# Gap analysis — after re-reading all five design docs

Sources reviewed in full: `PRD-offline-emergency-mesh.md`, `app-first-spectrum-protocol.html`,
`decentralized-network-layer.html`, `gateway-bridge-architecture.html`,
`component-reference-oss-papers.html`.

This document captures what the docs demand that the earlier build had missed,
what this round closed, and what remains — organized around your two explicit
priorities: **modularity across network types** and **leveraging the research
papers**.

---

## 1. Modularity — "bind to no single transport" (the #1 gap, now closed)

The spectrum doc §3 and network-layer **L1** are emphatic:

> *"The mistake would be to build 'a Bluetooth messenger.' Build a
> transport-abstraction protocol where a message is independent of how it
> travels — so the same encrypted packet can hop over BLE now, sound across a
> noisy room next, Wi-Fi Aware to the next cluster, and out through a LoRa or
> internet gateway when one appears."*

**What was missing:** the router was transport-*agnostic*, but there was no
actual transport *contract*, no engine running multiple transports, and no
fragmentation — so a 16 KB bundle physically could not cross a 128-byte
ultrasound frame. The system was not yet modular across network types.

**Closed this round — `crates/transport`:**
- **`Interface` trait** (`caps / scan / send / broadcast / poll`) — the PRD §8.4
  contract. BLE, Wi-Fi Aware, ultrasound, optical, LoRa, and internet are each a
  `Box<dyn Interface>` the engine drives identically.
- **`InterfaceCaps`** with honest MTU/range/throughput per the spectrum doc's
  tiers, plus `region`/`max_power_mw` so LoRa enforces **IN865 ≤ 1 W** (CR-1).
- **MTU-aware fragmentation/reassembly** (`frame`) with a compact positional
  wire form and hard bounds (`MAX_FRAGMENTS`, strict `total` checks) — the
  framing layer hardened per the Bridgefy lessons.
- **`NodeEngine`** — identity + router + CRDT state driven over *any number of
  interfaces concurrently* (FR-22), discovering peers via beacons (FR-7/FR-8) and
  carrying the same opaque bundle over each.
- **Proof:** integration tests deliver *and verify* the same message over
  ultrasound (128 B, ~30 fragments), BLE, LoRa, internet, and over three
  concurrent heterogeneous interfaces.

**Update:** a real socket transport now exists — `transport::UdpInterface`
(multicast + unicast seeds) meshes two nodes over actual UDP with **no relay**
(verified over real sockets), and it's wired into `lifeline-node` (`LIFELINE_UDP_PORT`).

**Still open (needs hardware or is a later phase):**
- Radio backends (BLE, Wi-Fi Aware, ggwave ultrasound, streaming-QR, RNode LoRa)
  implementing `Interface`. The seam is done; the drivers are platform work.
- **Lossy-link ARQ / selective repeat** — the in-memory medium is reliable; real
  ultrasound/LoRa drop frames, so fragment retransmission is needed (ties to
  Dhwani's "noise robustness" note).
- **Gateway-only transports** — modeling that LoRa/internet `bridges_offmesh`
  interfaces feed the gateway-bridge path end-to-end through the engine (today
  proven in `sim`, not yet in the engine).

---

## 2. Research-paper leverage — the "what to improve" agenda

The component-reference doc pairs each paper with a *what-to-improve* note. Status:

| Paper (layer) | "Improve" note | Status | Where |
|---|---|---|---|
| **Spyropoulos — Spray & Wait** (L4) | the actual replication policy | ✅ | `router` binary spray-and-wait |
| **Vahdat — Epidemic** (L4) | buffer/battery-hungry → constrain | ✅ | bounded copy budget + store cap |
| **Fall — DTN** (L4) | founding store-carry-forward blueprint | ✅ | `router` SCF + custody schema |
| **Cohn-Gordon / Alwen — Double Ratchet** (L2) | breaks under long one-way delays | ✅ | stateless sealed-box behind `SecureChannel` seam (OQ3) |
| **Back — Hashcash** (L6) | PoW postage to throttle spam | ✅ | `proto::pow`, router admission (FR-46) |
| **Shapiro — CRDTs** (L5) | **metadata growth/GC is the known cost — budget for it** | ✅ **new** | `sync`: causal-stability `gc_delivered` + `VersionVector::meet` |
| **Tarr — Secure Scuttlebutt** (L5) | **append-only logs grow forever** | ✅ **new** | `core::log`: signed `Checkpoint` compaction |
| **Kleppmann — Local-First** (L5) | design philosophy: fast completeness | ✅ | CRDT merge on contact |
| **Maymounkov — Kademlia** (L3) | weak to Sybil/eclipse + churn → pair with gossip + reputation | ◐ | XOR metric + gossip announces + **reputation done** (`router::Reputation`); online DHT backend ○ |
| **Douceur — Sybil** (L6) | identity hard without authority → PoW + reputation | ◐ | PoW identity cost via postage; **reputation gossip ○** |
| **Burleigh — RFC 9171 BP7** (L4) | build to the standard for interop | ○ | our bundle mirrors BP7 concepts (custody/TTL/priority); explicit BP7 endpoint-ID mapping pending |
| **Benet — IPFS** (L5) | content-addressing for cached pages/alerts + large files | ○ | `AttachChunk` schema only; content-addressed block store pending (FR-13) |
| **Nandakumar — Dhwani** (L0) | acoustic range/noise robustness | ◐ | ultrasound `Interface` + fragmentation done; FEC/ARQ for noisy channels ○ |
| **Helium — Proof-of-Relay** (L6) | incentive; proof-of-location was gamed | ○ | deferred (Phase 3), kept off the delivery path by design |
| **Nakamoto — Bitcoin** | *cautionary*: no global consensus in delivery path | ✅ | respected — no blockchain in the delivery path |

### Cross-cutting security analyses (Bridgefy — read these first)
- *Breaking Bridgefy* (CT-RSA 2021) & *…again* (USENIX 2022): **threat-model the
  mesh/framing layer, not just message crypto.**
  - ✅ Impersonation/MITM: header signature verified with the unsealed sender key.
  - ✅ Framing hardening: strict CBOR, bounded reassembly, `MAX_FRAGMENTS`,
    rejected malformed `total`.
  - ✅ **Panic-freedom fuzz tests** on the `proto` CBOR and `transport::frame`
    parsers (600k+ random/mutated/truncated inputs in CI). A dedicated
    `cargo fuzz` harness for continuous fuzzing remains a nice-to-have.

---

## 3. Remaining functional gaps (grounded in the docs)

| Gap | Doc basis | Priority |
|---|---|---|
| ~~Erasure/fountain coding across mules~~ | PRD FR-28; network-layer Problem C | ✅ done (`core::erasure` Reed-Solomon; survives 20%-loss partition in `sim`) |
| ~~Custody receipts exchange~~ | PRD FR-25; DTN BP7 | ✅ **done** — automatic engine round-trip (`CustodyRole::Custodian` signs, carrier releases; never releases originated bundles) |
| ~~Reputation gossip / black-hole avoidance~~ | Problem C; Kademlia/Douceur/Helium | ✅ mechanism done (auto-attribution ○) |
| ~~Parser panic-freedom fuzzing~~ | Bridgefy analyses | ✅ done (continuous `cargo fuzz` nice-to-have) |
| ~~Real socket transport (no relay)~~ | spectrum §3; L0 | ✅ `UdpInterface` done |
| ~~Sender-keys group encryption~~ | PRD FR-12 | ✅ done (`core::group` Megolm-style; fans out + decrypts across nodes) |
| ~~Content-addressed blocks (IPFS/IPLD)~~ | PRD FR-13; Benet | ✅ **done** — `core::content` blocks + engine **mesh fetch-by-CID** (`BlockRequest`/`BlockResponse`, hash-verified) |
| ~~Lossy-link ARQ / selective repeat~~ | Dhwani noise robustness | ✅ **done** — `transport::arq` selective-repeat; recovers a message over a 30%-loss channel |
| ~~Onion/metadata privacy~~ | PRD FR-49; gateway §5 | ✅ **done** — `core::onion` + engine forwarding path (peel-and-re-seal per hop), exposed as "private send" |
| **Official alert ingest (India Cell Broadcast)** | PRD FR-42 | ◐ signed-alert trust done (`core::alert`); external CB feed ○ |
| **Endpoint moderation enforcement** | PRD FR-48 | Low — blocklist CRDT done; drop-enforcement wiring |
| **BP7 interop mapping** | RFC 9171 | Low — credibility/interop |
| ~~Incentive / proof-of-relay~~ | Helium; kept off delivery path | ✅ done (`core::relay_proof`, witness-signed credits — self-witnessing rejected) |

## 4. Recommended next slice
Recently completed: erasure coding (FR-28), reputation (FR-47), parser
hardening (NFR-1), a real UDP/LAN transport, sender-keys groups (FR-12), and —
this round — **lossy-link ARQ**, **custody transfer round-trip (FR-25)**, **onion
forwarding in the engine (FR-49)**, and **mesh fetch-by-CID (FR-13)** (plus a
latent large-blob CBOR decode fix). Next, in priority order:
1. **Kademlia DHT online backend (FR-8)** — the XOR-metric routing table behind
   the already-working signed gossip announces.
2. **BP7 interop codec (RFC 9171)** — explicit endpoint-ID mapping for standards
   interop, and a **`cargo fuzz` harness** for continuous parser fuzzing.
3. **Custody + onion engine round-trips are done; next reliability item is
   gateway-directional custody** via propagated gateway gradient in the engine.
4. **Full-text encrypted history search (FR-15)** and **official Cell-Broadcast
   ingest (FR-42)**.
