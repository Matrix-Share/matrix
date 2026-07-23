# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project aims
to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Sender-keys group encryption** (`core::group`, FR-12): ratcheting per-sender
  chain key, sealed key distribution, bounded skipped-key cache (reordering
  tolerant), Ed25519-signed messages, and late-join forward secrecy.
- **Content-addressed blocks** (`core::content`, FR-13): BLAKE3-CID blocks +
  Merkle manifest for large attachments / cached alerts — intrinsic integrity,
  free dedup, missing-block reporting (IPFS model).
- **Endpoint features**: "I'm safe" broadcast (FR-41), location sharing (FR-43),
  and **blocklist enforcement** (FR-48 — blocked senders dropped, no receipt).
- **Erasure / fountain coding** (`core::erasure`, FR-28): Reed-Solomon splits a
  message into `k + m` fragment bundles; **any `k` reconstruct** it, so it
  survives partial carrier escape. `NodeEngine::submit_erasure` + reassembly;
  verified end-to-end and surviving a 20%-loss partition in the simulator.
- **Real UDP/LAN transport** (`transport::UdpInterface`): multicast discovery +
  unicast, so two nodes mesh over actual sockets with **no relay** (verified
  end-to-end). Opt-in in the node via `LIFELINE_UDP_PORT`.
- **Reputation / black-hole avoidance** (`router::Reputation`, FR-47):
  credit/penalize + pessimistic gossip-merge; the router routes around demoted
  relays (never blocking SOS or direct delivery); demotions propagate mesh-wide.
- **Parser hardening** (NFR-1): panic-freedom fuzz tests over the `proto` CBOR
  and `transport::frame` parsers (600k+ random/mutated/truncated inputs), plus
  bounded reassembly — the Bridgefy framing lesson.
- **Web GUI + node daemon** (`lifeline-node`): a browser chat client over the
  mesh — identity/invite code, auto-discovery, end-to-end-encrypted messages
  with delivery/verified ticks, live mesh status, and an in-GUI network
  self-test that runs the acceptance simulator.
- **Zero-knowledge relay** (`lifeline-relay`): forwards opaque ciphertext frames
  between nodes (the internet-gateway fabric); never reads or forges content.
- **Transport abstraction** (`lifeline-transport`): the `Interface` contract,
  MTU-aware fragmentation/reassembly, an in-process test medium, concrete
  BLE/Wi-Fi Aware/ultrasound/optical/LoRa/internet interfaces, a `ChannelInterface`
  for real networks, and the multi-interface `NodeEngine` runtime.
- **CRDT sync** (`lifeline-sync`): ORSWOT + LWW + version vectors + shared state,
  with causal-stability garbage collection (Shapiro).
- **PoW postage** anti-abuse (`proto::pow`), enforced at router admission (FR-46).
- **Hash-linked log compaction** (`core::log::Checkpoint`) — bounded growth (Tarr).
- Docker + docker-compose for self-hosting; full OSS project scaffolding.

### Notes
- This is a pre-1.0 engineering preview: the decentralized core, transport
  abstraction, and self-hostable app are functional and tested (all crates green),
  but real radio backends, a security audit, and several PRD features remain (see
  `STATUS.md` and `GAPS.md`).
