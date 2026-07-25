# Implementation status — traceability to the PRD

Legend: **✅ Done** (implemented + tested) · **◐ Partial** (schema/protocol present;
UI/transport/persistence layer pending) · **○ Planned** (later phase).

This build covers **the decentralized core** (PRD §16: P1.0/P1.1), the **L0/L1
transport abstraction** (modular across BLE/Wi-Fi Aware/ultrasound/optical/LoRa/
internet), plus the first slices of **P2.2 (CRDT sync, FR-33)** and **P3.0
anti-abuse (PoW postage, FR-46)** — i.e. the full L1–L6 substrate. What remains
is real radio/socket *backends* behind the finished `Interface` seam, the mobile
UI, and the web console. A full design-doc gap analysis and research-paper agenda
is in [`GAPS.md`](GAPS.md).

## Identity & onboarding (§8.1)
| Req | State | Where |
|---|---|---|
| FR-1 Ed25519/X25519 keypair on first launch | ✅ | `core::identity::Identity::generate` |
| FR-2 Address derived from public key | ✅ | `Identity::derive_address` = `blake3(sign_pub)[..16]` |
| FR-3 Shareable identity + fingerprint | ✅ | `Identity::public`/`fingerprint` + **server-rendered QR** of the invite code (`GET /api/qr.svg`) shown in the GUI |
| FR-4 Passphrase/biometric unlock of keystore | ○ | needs platform secure storage |
| FR-5 Encrypted key backup/restore | ✅ | `core::identity::KeyBackup` (Argon2id + AEAD) |

## Contacts & discovery (§8.2)
| Req | State | Where |
|---|---|---|
| FR-6 Add contact by QR TOFU (MITM-resistant) | ◐ | crypto binding + header-sig verify done; **QR displayed for in-person exchange** + paste-code add; camera *scanning* needs a device (○) |
| FR-7 BLE/Wi-Fi-Aware advertisement discovery | ◐ | beacon-based peer discovery works over the transport (`NodeEngine`, proven over the relay in `lifeline-node`); BLE/NAN *backend* ○ |
| FR-8 DHT online / gossip announces offline | ◐ | signed gateway announces + gradient now **propagate live in `NodeEngine`**: gateways emit signed announces, nodes gossip them hop-by-hop and build a gradient toward the nearest gateway (announce sig verified when the gateway is a known contact); proven forming 0→1→2 along a mesh line. Kademlia DHT (online) ○ |
| FR-9 Contact store | ✅ | contact directory **persisted encrypted** across restarts (`core::vault` + node); verification-state field in `proto::Contact` |

## Messaging (§8.3)
| Req | State | Where |
|---|---|---|
| FR-10 Send E2E text | ✅ | `core::message::seal_bundle` |
| FR-11 Message states | ◐ | `proto::MessageState`; sim tracks delivered/verified |
| FR-12 Group messages (sender-keys, CRDT membership) | ✅ | CRDT membership + `core::group` sender-keys, **wired end-to-end in `NodeEngine`** (`create_group`/`add_group_member`/`send_group`: distribution + fan-out + decrypt, out-of-order buffer); tested (3-member fan-out, non-member excluded, multi-sender) |
| FR-13 Small attachments + content addressing | ✅ | **`core::content`** (BLAKE3-CID blocks, Merkle manifest, dedup, integrity-checked reassemble) + **mesh fetch-by-CID protocol** in the engine: `store_content`/`fetch_content`, `BlockRequest`/`BlockResponse` over the mesh (binary blocks, hash-verified on arrival), request retransmission, cached-block short-circuit — proven pulling a multi-block object between two nodes |
| FR-14 Priority classes SOS>ALERT>NORMAL>BULK | ✅ | `proto::Priority` |
| FR-15 Local encrypted history | ✅ (encrypted store) | message history **persisted encrypted at rest** (`core::vault`: Argon2id + XChaCha20-Poly1305) — restored on restart, verified end-to-end; full-text search ○ |

## Transports (§8.4)
| Req | State | Where |
|---|---|---|
| FR-16..21 BLE / Wi-Fi Aware / ultrasound / optical / LoRa / internet | ◐ | **`transport::Interface` contract + caps + MTU fragmentation for all six, driven by `NodeEngine`; delivery proven over each in tests. A real `UdpInterface` (multicast/LAN) meshes two nodes with no relay (verified over real sockets). Lossy-link ARQ (`transport::arq`, selective-repeat with cumulative+bitmap SACKs) recovers dropped fragments — proven delivering a message over a 30%-loss channel.** BLE/ggwave/LoRa radio backends ○ (platform work) |
| FR-22 Concurrent transports, router picks best | ✅ | `NodeEngine` runs multiple `Interface`s concurrently; same bundle over any (test: BLE+ultrasound+internet) |

## Routing & delivery (§8.5)
| Req | State | Where |
|---|---|---|
| FR-23 Store-carry-forward | ✅ | `router::DtnRouter` + `store` |
| FR-24 Binary spray-and-wait (budget L) | ✅ | `router::offer_to` |
| FR-25 Custody transfer | ✅ | signed custody receipts + `router::release_custody`, now with the **automatic engine round-trip**: a committed `CustodyRole::Custodian` (gateway/base/provisioned mule) signs for relayed bundles it stores and the previous-hop carrier frees its copy — never releasing bundles it originated, so delivery is never reduced (proven end-to-end over a forced relay line) |
| FR-26 TTL, hop limit, dedup | ✅ | `router::ingest` |
| FR-27 Priority queueing, SOS preempts | ✅ | `router::offer_to` (strict priority sort) |
| FR-28 Erasure/fountain coding | ✅ | `core::erasure` (Reed-Solomon, any k-of-n) + `Bundle.frag`; `NodeEngine::submit_erasure` + reassembly; **proven end-to-end and surviving a 20%-loss partition in `sim`** |
| FR-29 Gateway gradient + ≥95% delivery AC | ✅ | `router::gateway`; **proven in `sim` (100%)** |

## Integrity, proof & sync (§8.6)
| Req | State | Where |
|---|---|---|
| FR-30 Append-only hash-linked log | ✅ | `core::log::HashLog` (tamper-evident, tested) + signed `Checkpoint` compaction (Tarr "logs grow forever") |
| FR-31 Signed delivery receipt | ✅ | `core::receipt::make_delivery_receipt` |
| FR-32 Match receipts → verified; adaptive retry | ✅ | matching + verify + delivery-status CRDT; **adaptive retry** (`router::respray` + engine re-spray of unverified messages past a window, capped) — tested |
| FR-33 CRDT state merge after partition | ✅ | `sync` (ORSWOT + LWW + version vectors); **proven converging in the mesh** (`sim`); causal-stability GC bounds metadata (Shapiro) |
| FR-34 Offline verification function | ✅ | `core::receipt::verify_delivery` (pure, offline) |

## Gateway node (§8.7)
| Req | State | Where |
|---|---|---|
| FR-35 Toggle gateway mode | ✅ | `router::set_gateway`, now driveable in the live node via `LIFELINE_GATEWAY` (emits announces + bridges); surfaced in the GUI (gateway badge + gradient) |
| FR-36 Signed gateway announce | ✅ | `proto::GatewayAnnounce` + `sim::make_announce` |
| FR-37 Bridge bundles to internet/LoRa | ✅ (internet) | **gateway bridging wired into the live engine**: a gateway routes the mesh downhill to itself (gradient) and pushes bundles onto every off-mesh uplink (`bridges_offmesh`) — proven with a mesh-only node's message escaping to an **off-mesh destination** reachable only via the gateway, receipt returning. LoRa radio backend ○ |
| FR-38 Gateways handle ciphertext only | ✅ | router never decrypts; treats payload as opaque |
| FR-39 Web gateway console | ◐ | **`lifeline-node` serves a two-view web product** (theme-aware) over HTTP/WS — **Messages** (connect flow, contacts, pinned mesh thread, per-message lifecycle chips) + **Network** dashboard (stat tiles, broadcast-to-mesh, SOS/safe, live propagation activity feed, peer list, in-GUI self-test); LoRa-over-Web-Serial console ○ |

## Emergency features (§8.8)
| Req | State | Where |
|---|---|---|
| FR-40 One-tap SOS + GPS + battery | ✅ | protocol + `NodeEngine::broadcast_sos` + **GUI SOS button** (geolocation + battery, graceful fallback) via `POST /api/sos` — verified delivering as `in-sos` end-to-end |
| FR-41 "I'm safe" broadcast | ✅ | `NodeEngine::broadcast_safe` fans out to all contacts (tested end-to-end) |
| FR-42 Authority alerts | ✅ | `core::alert` — Ed25519-signed alerts with an **authenticated broadcast identity**, trusted-authority root store, key↔address binding, expiry (tested incl. spoof/tamper). External Cell-Broadcast *ingest* ○ |
| FR-43 Location sharing | ✅ | `NodeEngine::submit_location` + **GUI share-location** (`POST /api/location`, browser geolocation) — verified delivered; periodic-interval scheduling is app-level |

## Security & anti-abuse (§8.9)
| Req | State | Where |
|---|---|---|
| FR-44 E2E encryption everywhere | ✅ | `core::message` / `core::crypto` + **forward-secret rotating prekeys wired end-to-end**: a node advertises a signed prekey in its beacon, senders seal to it, and the recipient opens via its retention-windowed `core::prekey::PrekeyRing` — so a seized long-term key can't recover pruned messages (proven: FS bundle opens only via the ring, then becomes unrecoverable after rotation; end-to-end delivery + receipt over the prekey path). The ring is **persisted encrypted at rest and restored on restart** (proven: an imported ring still opens in-flight messages, and forward secrecy holds across the restart). |
| FR-45 Sealed sender | ✅ | `core::message` — sender identity **and its signature** sealed to the recipient, so a relay/observer with a suspect list can't trial-verify a cleartext signature to deanonymize the sender (hardened after the internal audit; the `Bundle` carries no wire signature) |
| FR-46 PoW postage | ✅ | `proto::pow` (Hashcash, difficulty by priority, SOS exempt); enforced at router admission; **flood-throttle AC proven** (`sim`) |
| FR-47 Reputation gossip | ◐ | **`router::Reputation` (credit/penalize/pessimistic gossip-merge) demotes relays; `offer_to` routes around demoted black holes (never blocking SOS/direct delivery); demotion propagates mesh-wide — proven in `sim`.** Automatic black-hole *attribution* (custody-chain analysis) ○ |
| FR-48 Endpoint moderation (block/blocklists) | ✅ | shared blocklist CRDT (`sync`) + **endpoint enforcement** (`NodeEngine` drops blocked senders — no inbox, no receipt; tested) |
| FR-49 Onion metadata wrapping | ✅ | `core::onion` build/peel + the **engine forwarding path**: `submit_onion` builds the route, each relay peels one layer and re-seals to the next hop (buffering until the hop's key is learned), so no relay learns more than the next hop and the recipient sees only the last relay. Exposed as a "private send" in the node/GUI. Proven A→R1→R2→Bob end-to-end |

## Settings & platform (§8.10)
| Req | State | Where |
|---|---|---|
| FR-50 Battery-saver duty cycling | ○ | platform |
| FR-51 Per-transport toggles | ○ | with transports |
| FR-52 Store cap + priority/TTL-aware LRU eviction | ✅ | `router::store::BundleStore` |
| FR-53 Diagnostics view | ✅ | **GUI diagnostics panel** — link/messages/relaying sections: peers, gateways, queue depth + bytes, custody handoffs, duplicates, drops (expired / no-postage), retries |

## Non-functional
| Req | State |
|---|---|
| NFR-3 ≥95% eventual delivery (3-cluster + mule) | ✅ proven in `sim` (100%) |
| NFR-8 Offline-first, no server dependency | ✅ core has zero network dependency |
| NFR-9 Versioned wire format | ✅ `proto::WIRE_VERSION` on every envelope |
| NFR-1 Independent security audit | ◐ | **Framing hardening + panic-freedom fuzz tests (600k+ inputs). Internal audits (crypto/E2E + transport/router/app) found and fixed: true sealed sender, group owner↔key binding, decompression bound, key zeroization, onion length-hiding, receipt domain sep; loopback-only API + Host-validation, self-verifying gateway announces, bounded mesh-control collections (seen/gateway/reputation/ARQ/peers), spray-copy clamp, SOS-eviction protection, solicited-only block fetch, relay queue/conn caps, GUI sink escaping.** Independent third-party audit still a pre-launch gate (Phase 3) |

## Suggested next steps (in PRD phase order)
1. **P1.2 transports** — implement the `router` contact source over a real BLE
   transport (Android/iOS platform channels), then ultrasound (ggwave) as the
   radio-off fallback. The router API (`offer_to` / `ingest` / `known_ids`) is
   already the seam.
2. **Custody receipts (FR-25)** — wire signed `CustodyReceipt` exchange so nodes
   can drop copies early under store pressure.
3. **Sender-keys group encryption (FR-12)** — layer group message encryption on
   top of the converged CRDT membership already provided by `sync`.
4. **Reputation gossip (FR-47)** — demote relays that drop custody/receipts,
   composing with the PoW postage already in place.
5. **Fuzzing (NFR-1 gate)** — `cargo fuzz` targets on the CBOR parsers in
   `proto` (now including `postage`) before any wider deployment (Bridgefy
   lesson, PRD §15).

## Application & self-hosting (new)
| Capability | State | Where |
|---|---|---|
| Web GUI product (Messages + Network views; connect flow, contacts, mesh thread, lifecycle chips, live propagation dashboard) | ✅ | `lifeline-node` + `crates/node/web/index.html` |
| Broadcast to the mesh (fan-out to every contact, propagated) | ✅ | `NodeEngine::broadcast_text` + `POST /api/broadcast` |
| Payload compression (pre-encryption, DEFLATE, keep-smaller) | ✅ | `core::compress` framed into `seal_bundle`/`open_bundle` — shrinks every sealed bundle on scarce bearers; never inflates |
| Bandwidth-adaptive bearer selection ("straw, not a firehose") | ✅ | `router::offer_to` holds bulky NORMAL/BULK bundles off low-throughput links (per-bearer `soft_max_bytes`) so they wait for a fatter bearer; SOS/ALERT + final-hop always pass (tested over ultrasound vs internet) |
| Zero-knowledge relay (internet fabric, ciphertext-only) | ✅ | `lifeline-relay` |
| Real network transport | ✅ | `transport::ChannelInterface` + relay client |
| Extensible external-network seam | ✅ | `transport::ExternalNet` + `BridgeInterface` — any network becomes an engine interface via one trait (template in `lifeline-bridge::skeleton`) |
| **Nostr connectivity** | ✅ | `lifeline-bridge::nostr` — real secp256k1 NIP-01 events + relay store-and-forward; **two full nodes exchange an E2E message + receipt over a Nostr relay** with no engine change. Real WebSocket relay client is the thin remaining drop-in ([`docs/nostr-integration.md`](docs/nostr-integration.md)) |
| In-GUI acceptance self-test | ✅ | `/api/selftest` runs the 3-cluster+mule scenario |
| Docker / docker-compose self-hosting | ✅ | `Dockerfile`, `docker-compose.yml` |
| Identity persistence (encrypted at rest) | ✅ | Argon2id `KeyBackup` in `LIFELINE_DATA_DIR` |
| OSS project hygiene (LICENSE, CI, CONTRIBUTING, SECURITY, CoC, templates) | ✅ | repo root + `.github/` |
| OSS capability-migration map | ✅ | [`INTEROP.md`](INTEROP.md) |

### Delta-sync note (§12.3)
`sync` currently converges via **full-state CRDT merge** on each contact, which
is correct (idempotent/commutative/associative) and proven in the mesh. The
version-vector machinery is in place to move to **delta transfer** (send only
dots the peer lacks) as a bandwidth optimization; convergence guarantees are
unchanged.
