# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project aims
to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Security
Fixes from an internal cryptography audit of the E2E core (NFR-1; an independent
audit remains a pre-launch gate):
- **True sealed sender (HIGH)**: the sender's Ed25519 signature was over the
  relay-visible header and carried in cleartext, so an observer with a suspect
  list could trial-verify it to deanonymize the sender. The signature now lives
  **inside the recipient-sealed envelope** (`SenderAuth`); the `Bundle` carries no
  wire signature. Integrity and sender-authentication are preserved for the
  recipient; delivery proof is now sender-private by design (you cannot both hide
  the sender and let third parties verify them).
- **Group impersonation (HIGH)**: `ReceiverKeyState::from_distribution` now binds
  `address_of(sign_pub) == owner`, and the engine rejects any group op whose
  claimed `owner` isn't the E2E-authenticated bundle sender — closing a hole
  where a member could forge messages attributed to another member.
- **Decompression bomb (MED)**: payload inflate is now bounded
  (`decompress_to_vec_with_limit`, 16 MiB ceiling) so an authenticated peer can't
  OOM a recipient.
- **Zeroization**: group chain/skipped keys and derived AEAD keys are wiped on
  drop.
- **Onion length-hiding**: the delivered payload is padded to fixed cells so the
  last relay can't read the message length (full constant-cell routing is future
  work).
- Delivery-receipt signing bytes are domain-separated; the honest forward-secrecy
  posture is documented (no rotation yet — a ratchet is the follow-up);
  `generate_from_rng` is marked testing-only; identity local secret arrays are
  zeroized.

A second pass audited the transport/router/app layers (the Bridgefy-class mesh
surface) and hardened it:
- **Local API is now loopback-only by default** (`127.0.0.1`) with **Host-header
  validation** — closes the "any device on the LAN can read history / send as the
  user / fire a false SOS" exposure and DNS-rebinding. The insecure default vault
  passphrase now logs a prominent warning.
- **Self-verifying gateway announces**: `GatewayAnnounce` carries the gateway's
  key; the engine verifies **every** announce (not just from known contacts),
  rejects implausibly far-future expiry, and throttles re-gossip — closing
  gradient-poisoning and announce-flood vectors.
- **Bounded mesh-control collections** (memory-exhaustion DoS): router dedup
  (`seen`) and `bridged` sets, gateway cache, reputation map, `bridge_out`, the
  ARQ reassembly map, and discovered-peer/contact maps are all capped with
  eviction.
- **Abuse clamps**: `copies_left` is clamped to `[1, 16]` (no spray storm); SOS
  bundles are never evicted (an SOS flood can't push out held emergencies);
  block-fetch accepts **solicited** responses only (no unsolicited store-fill).
- **Relay hardening**: bounded per-connection queue (drop-on-full, no slow-reader
  OOM) and a max-connections cap.
- **GUI**: the last unescaped DOM sink (`initials()`) is now escaped
  (defense-in-depth; message bodies were already escaped — no XSS was reachable).

### Changed
- **Split the mis-layered `transport` crate (from the architecture review).** The
  crate named for the L1 bearer seam actually contained the `NodeEngine`
  orchestrator, so it depended "upward" on `router`/`sync`/`core`. The engine now
  lives in a new **`lifeline-engine`** crate; `lifeline-transport` is a
  **proto-only seam** (just `Interface`/`ExternalNet`/framing/ARQ), so
  implementing a new bearer no longer drags in the router, CRDTs, or runtime. The
  engine's integration tests moved with it; the ≥95%-delivery acceptance sim and
  the full suite pass unchanged.

### Added
- **Complete web app — every engine feature now has a UI.** The browser GUI
  previously covered 1:1 messaging, broadcast, SOS/safe, location, and
  diagnostics. Added full-stack support (Command + API endpoint + engine wiring +
  UI) for the engine capabilities that had none:
  - **Group messaging (FR-12).** Create a group, add contacts (each gets your
    sender key), and send — group threads appear in the sidebar with a member
    bar, an add-member picker, per-sender labels on incoming messages, and a
    dedicated group composer. Endpoints `/api/group/{create,add,send}`; group ids
    persist across restarts.
  - **Endpoint moderation (FR-48).** Block/unblock a contact from the chat's ⋯
    menu or the peers list; blocked state is shown throughout. Endpoints
    `/api/{block,unblock}`.
  - **Message priority.** A composer toggle sends at Alert priority (jumps queues
    at every hop), alongside the existing private/onion send.
  - **Polish:** incoming location messages linkify to a map; the snapshot now
    carries `groups[]` and per-contact `blocked`. Verified end-to-end in a live
    node (group create → chat → member bar, block/unblock, priority, dark mode).
- **Pluggable `RoutingPolicy` seam (from the architecture review).** The
  forwarding strategy — binary spray-and-wait, gateway-gradient escape hatch,
  reputation route-around, bandwidth hold-back — was inlined in
  `DtnRouter::offer_to`, so a different strategy (epidemic, PRoPHET,
  Reticulum-style) meant editing the router. Extracted a `RoutingPolicy` trait
  (`decide(&OfferContext) -> OfferAction`) with the shipped behaviour as the
  default `SprayAndWaitPolicy`, injectable via `DtnRouter::with_policy`. The
  router keeps the *mechanism* (iteration, copy-budget mutation, stats); the
  policy is a pure, unit-tested decision function fed scalar context (no store
  internals). Behaviour is unchanged — the ≥95%-delivery acceptance sim passes.

### Changed
- **Crypto hygiene (from the architecture review).**
  - **Removed the vestigial `SecureChannel` trait.** It advertised a swap-in point
    for a Double Ratchet but was stateless associated functions over concrete
    dalek key types — it literally could not express a stateful ratchet, and
    nothing dispatched through it. `SealedBox::seal`/`open` are now inherent
    methods; the docs state honestly that forward secrecy comes from the rotating
    prekey ring, and a real ratchet would be its own session type, not a drop-in.
  - **`hkdf_sha256` now returns `Zeroizing<Vec<u8>>`**, so derived key material is
    wiped on drop by default. This *closes a real leak*: `group::ratchet` was
    dropping raw HKDF message-/chain-key output un-zeroized on every group message.
  - **Centralized every domain-separation label into one `core::domain` module.**
    Previously scattered across nine files with a duplicate `INFO_MSG` name
    (meaning two different things) and message-sealing labels misplaced in
    `identity`. Values are byte-for-byte unchanged (no wire/at-rest break — crypto
    round-trips confirm it); the win is a single audited list, with a test
    asserting all labels are distinct so a future collision fails CI.
- **Robustness hardening (from the architecture review).**
  - **Crash-safe persistence.** The encrypted vault and identity are now written
    via temp-file-then-`rename` (`write_atomic`), so a crash mid-write keeps the
    previous good file instead of a truncated one — which the loader would
    otherwise discard as "start fresh", silently losing all contacts + history.
  - **Graceful shutdown.** SIGTERM/Ctrl-C now triggers `axum`'s graceful shutdown,
    then sends the engine a `Command::Shutdown` that forces a final state flush;
    `main` joins the engine thread before exiting, so a freshly rotated prekey
    ring and recent messages survive a stop. The periodic save and the shutdown
    flush share one `persist_state` path so they can't diverge.
  - **Bounded CBOR decoder.** `from_cbor` now runs an iterative structural
    pre-scan (`guard_cbor`) that bounds document size (16 MiB) and nesting depth
    (128) *before* `ciborium` — which has no recursion limit — sees the bytes,
    closing a remote allocation-bomb / stack-overflow DoS. The guard is iterative,
    so it can't itself overflow. Same bug-class as the fixed 4 KiB scratch cap.
- **Protocol evolution hardening (NFR-9 interoperability).** From an independent
  architecture review: the wire format could not evolve without partitioning the
  mesh. Fixed:
  - `WIRE_VERSION` is now **load-bearing** — stamped as `Bundle.v` and **checked
    at ingest** (`DtnRouter::ingest` rejects an incompatible version before the
    dedup/store/deliver paths; new `dropped_version` stat). Documented evolution
    policy: additive optional fields and new enum values do *not* bump it; only a
    structurally incompatible header change does.
  - `Priority` and `PayloadKind` now **wire-encode as integer discriminants** (via
    `serde(from/into = u8)`) instead of variant-name strings, and decode unknown
    values to a **safe fallback** — an unknown `Priority` → `Bulk` (still relayed,
    never preempts; it rides in the cleartext header every hop reads), an unknown
    `PayloadKind` → `Unknown` (the engine ignores it). So a new priority/payload
    class rolls out gradually **without hard-erroring un-upgraded nodes**. Bumped
    `WIRE_VERSION` → 2.
  - The engine's inbound payload dispatch is now an **exhaustive `match`** (no
    wildcard), so adding a `PayloadKind` is a compile error until it is routed —
    closing a silent-misroute gap where a new control type fell through to the app
    inbox.

### Added
- **Black-hole attribution — live reputation feedback (FR-47).** The reputation
  *scoring* primitive existed but nothing fed it from live evidence; now the
  source of a bundle turns real delivery outcomes into credit/penalty. Because an
  end-to-end delivery receipt is sealed to the **original sender**, the source is
  the only node that can soundly attribute — so `router::attribution::ForwardLedger`
  records which peers signed custody for our bundles (`process_custody`), credits
  every such custodian when the sealed delivery receipt verifies (`process_receipt`),
  and — only after a **grace count** of unconfirmed expiries — penalizes a
  custodian that keeps swallowing bundles without ever delivering (`tick`). Fully
  **passive**: it never changes what we store or forward, only the reputation
  scores, which `offer_to` already consults *and only routes around a demoted peer
  when an alternative exists* — so the ≥95%-delivery acceptance target is
  protected (verified: the acceptance sim still passes, and tests show a black hole
  is demoted while an honest custodian and a low-contact carrier within grace are
  not).
- **Kademlia DHT (`lifeline-dht`) — online peer & rendezvous discovery.** A new
  crate implementing the Kademlia DHT (Maymounkov & Mazières) over Lifeline's
  existing 128-bit `Address` keyspace (which already carried an `xor_distance`
  metric for exactly this "L3 discovery layer"). A node with internet can locate
  *where a peer is reachable* or *who is registered at a rendezvous key* with no
  central directory: `RoutingTable` (k-buckets across 128 XOR buckets, LRU with
  incumbent-retention against eviction flooding), iterative α-parallel
  `FIND_NODE`/`FIND_VALUE` lookups, `STORE` to the k closest, and `bootstrap`.
  **Transport-agnostic** by the same seam philosophy as the rest of Lifeline —
  the lookup is pure logic driven over a synchronous `DhtRpc` request→response,
  so it rides any carrier and is fully testable against an in-memory network.
  Verified: iterative lookup **converges to the globally-closest node** across a
  60-node network, a value stored by one node is **resolved by a distant one**,
  and bootstrap populates the table (5 tests).
- **Meshtastic (MQTT) adapter — second external network.** `lifeline-bridge::meshtastic`
  (`meshtastic` feature) speaks the **real Meshtastic wire format** — genuine
  `ServiceEnvelope`/`MeshPacket` protobuf (`prost`, hand-derived subset, byte-
  compatible with real packets) carried on a private application port — so
  Lifeline frames flow over actual Meshtastic LoRa hardware and public MQTT
  brokers. Because MQTT clients are synchronous, `MeshtasticNet` implements
  `ExternalNet` **directly** and is driven by the engine tick — no async runtime,
  no channel bridging: the cleanest possible demonstration that the seam extends
  to a structurally different network (announce/node-number addressing vs Nostr's
  pubkey events). Network I/O sits behind the small `MeshBus` trait: an in-memory
  `MockBroker`/`MockBus` makes the whole path testable without a broker (four
  tests: protobuf round-trip, two-adapter exchange, unknown-peer drop, channel
  isolation), and the live `MqttBus` (`mqtt` feature, `rumqttc`) talks to real
  brokers/devices. Wired into the node behind the `meshtastic` feature:
  `LIFELINE_MESHTASTIC_MQTT=host:1883` adds it as an extra engine bearer via
  `BridgeInterface`, node number derived from the identity (`derive_subkey`).
- **Live WebSocket Nostr client + node bearer.** `lifeline-bridge::ws` (`ws`
  feature) is an async `tokio-tungstenite` client that connects to **real Nostr
  relays** (`wss://…`): it subscribes with NIP-01 REQ filters (the shared
  `#L`=lifeline-mesh discovery channel + our own `#p` inbox), publishes outbound
  frames as signed EVENTs, verifies + de-dups inbound events, and learns the
  `PeerId ↔ nostr-pubkey` map — reusing the exact `nostr` codec, so it is a
  drop-in for the in-memory adapter. Proven end-to-end over a real WebSocket
  against an in-process relay (filter matching, offline replay, directed `#p`
  routing) in `crates/bridge/tests/nostr_ws.rs`. Wired into the node behind the
  **`nostr`** feature: `LIFELINE_NOSTR_RELAY=wss://relay.damus.io,wss://…` adds
  Nostr as an extra engine bearer (one reconnecting client per relay, exponential
  backoff, outbound fan-out), bridged to the engine through an ordinary
  `ChannelInterface` — **no engine change**. The node's Nostr keypair is derived
  from its long-term identity via the new `Identity::derive_subkey` (domain-
  separated, stable across restarts so the offline mailbox address persists, and
  unlinkable from the public Lifeline identity). Completes Phase 1 of
  [`docs/nostr-integration.md`](docs/nostr-integration.md).
- **External-network seam + Nostr connectivity.** New `transport::ExternalNet`
  trait + `BridgeInterface`: any message-passing network becomes a first-class
  engine interface by implementing **one trait** — the extension point for "more
  connectivity" (Reticulum, Meshtastic, Matrix, plain relays), with a
  documented, compiling template (`lifeline-bridge::skeleton`). The first adapter,
  **`lifeline-bridge::nostr`**, is a real Nostr integration: secp256k1 Schnorr
  NIP-01 events (`id = sha256(canonical array)`, verified like any Nostr client),
  a bundle↔event codec (opaque Lifeline ciphertext in event content, a shared
  `["L","lifeline-mesh"]` discovery channel, `["p", …]`-tagged directed messages),
  and relay-backed store-and-forward. **Proven end-to-end**: two full `NodeEngine`s
  discover each other, exchange an E2E-encrypted message, and return the signed
  delivery receipt entirely as signed Nostr events over a (mock) relay — **no
  `lifeline-relay`, no engine change**. This plugs Lifeline into the global,
  already-adopted Nostr relay network while leading with what Nostr lacks (forward
  secrecy, verifiable delivery, multi-bearer offline). Strategy + NIP mapping:
  [`docs/nostr-integration.md`](docs/nostr-integration.md).
- **Forward-secret prekeys** (`core::prekey`; audit MED-1): rotating,
  identity-signed recipient encryption keys with a retention window — the
  DTN-friendly alternative to a Double Ratchet (whose proofs assume timely,
  in-order delivery that store-carry-forward breaks). The recipient mints and
  publishes a signed prekey, retains a small ring of recent prekey secrets (≥ the
  message TTL so in-flight messages still open), then deletes older ones — after
  which a seized long-term key can't recover those messages. Proven: after
  rotation+prune an old ciphertext is unrecoverable while a current one opens; a
  message within the retention window still delivers; forged prekeys are rejected.
  **Now wired end-to-end in the live node**: a node rotates a `PrekeyRing`,
  advertises its current signed prekey in its beacon, senders seal to a contact's
  verified prekey (falling back to the long-term key otherwise — a forged prekey
  can only cost forward secrecy, never redirect a message), and the recipient
  opens via the ring. Proven end-to-end (message + receipt delivered over the
  prekey path). Follow-up: persist the ring across node restart.
- **Gateway-awareness in the live node** (FR-35/36/37): a node can run as a
  **gateway** (`LIFELINE_GATEWAY`), emitting signed announces (`core::announce`,
  verified when the gateway is a known contact) gossiped hop-by-hop so every node
  builds a **gradient** toward the nearest gateway; beacons now carry gateway
  flag + gradient so `offer_round` routes the last copy *downhill*, and a gateway
  **bridges** mesh bundles onto every off-mesh uplink. Proven: gradient forms
  0→1→2 along a mesh line, and a mesh-only node's message escapes to an off-mesh
  destination reachable only via the gateway. Surfaced in the GUI (gateway badge
  + gradient).
- **Bandwidth-adaptive routing** ("straw, not a firehose"): **payload
  compression** (`core::compress`, DEFLATE applied to the plaintext *before*
  encryption, kept only when smaller — shrinks every sealed bundle) and
  **adaptive bearer selection** (`router::offer_to` holds bulky NORMAL/BULK
  bundles off low-throughput links via a per-bearer `soft_max_bytes` so they wait
  for a fatter bearer; SOS/ALERT and final-hop delivery always pass). Tested over
  ultrasound vs internet.
- **Lossy-link ARQ / selective repeat** (`transport::arq`, Dhwani noise
  robustness): unicast bundle frames are now retransmitted until acknowledged.
  The sender blasts once then re-sends only unacked fragments on an RTO timer;
  the receiver replies with a compact selective ACK (cumulative base + a windowed
  bitmap) that fits even ultrasound's MTU. Proven delivering a message **and its
  receipt over a 30%-loss channel**; a reliable link triggers zero retransmits.
  Surfaced as a "Repaired frames" tile in the GUI.
- **Custody transfer round-trip** (FR-25): a `CustodyRole::Custodian`
  (gateway/base/well-provisioned mule) signs a custody receipt for relayed
  bundles it stores; the previous-hop carrier verifies it and frees its copy.
  Nodes never release bundles they originated, so custody only ever moves a
  bundle to a safer holder — delivery is never reduced. `LIFELINE_CUSTODIAN=1`
  runs a self-hosted node as a custodian.
- **Onion forwarding in the engine** (FR-49): `submit_onion` builds a route and
  each relay peels one layer and re-seals to the next hop (buffering until the
  hop's key is learned), so no relay learns more than the next hop. Exposed as a
  **private send** (shield toggle) in the node API + GUI.
- **Mesh fetch-by-CID** (FR-13): `store_content`/`fetch_content` plus
  `BlockRequest`/`BlockResponse` payloads pull content-addressed blocks by hash
  over the mesh, verifying each block against its CID before storing, with
  request retransmission and a cached-block short-circuit.

### Fixed
- **Large-blob CBOR decode** (`proto::codec`): `Bytes` now deserializes via
  `deserialize_byte_buf` instead of `deserialize_bytes`, which ciborium caps at
  its 4 KiB scratch buffer. Any wire blob over ~4 KiB (big attachments, block
  transfers, single-frame bundles on high-MTU links) previously failed to decode;
  now blobs of any size round-trip.

### Changed
- **Rebuilt web GUI into a two-view product (Messages + Network)**: the app now
  centers on real messaging and visible mesh propagation. **Messages** view has a
  prominent connect flow (QR invite / paste-a-code tabs), a contact list with
  avatars, previews, timestamps and unread markers, a pinned **"Mesh &
  broadcasts"** thread, and a chat pane with per-message lifecycle chips
  (Sending… → Delivered ✓✓). **Network** view is a live propagation dashboard:
  hero stat tiles (peers, gateways, delivered/verified, relayed copies, queue,
  retries, custody), a **broadcast-to-mesh** composer alongside SOS / I'm-safe,
  a client-diffed **live activity feed** that narrates propagation in real time
  (sent → verified → relayed), a peer list, and the packaged self-test. Verified
  end-to-end against a relay + two nodes.
- **Broadcast to the mesh** (`POST /api/broadcast`,
  `NodeEngine::broadcast_text`): send one message to every contact and ask the
  network to propagate it; recorded as a single mesh-thread entry that verifies
  as recipients confirm.
- **Redesigned web GUI ("Calm Utility")**: a modern, theme-aware (light/dark),
  accessible interface — monochrome surfaces + one accent, hairline borders,
  conversation list with previews, delivery/verified ticks, safety-accented
  emergency actions, avatars, live status, and a collapsible diagnostics panel.
  New endpoints `POST /api/safe` (FR-41) and `POST /api/location` (FR-43) back
  the emergency/share actions; inbound Location/SOS/Safe payloads render with
  readable summaries.

### Added
- **Encrypted persistence** (`core::vault`, FR-9/FR-15): the contact directory
  and message history are persisted **encrypted at rest** (Argon2id-derived key,
  reused across saves; XChaCha20-Poly1305) and restored on restart — verified
  end-to-end (message + contact survive a node restart; wrong passphrase yields a
  fresh node, no leak).
- **GUI: QR invite, SOS button, diagnostics** (FR-3/FR-6/FR-40/FR-53): the node
  serves a QR of its invite code (`GET /api/qr.svg`) for in-person contact
  exchange; a one-tap SOS button attaches GPS + battery (with graceful fallback)
  via `POST /api/sos`; and a diagnostics panel surfaces peers, gateways, queue
  depth, custody handoffs, duplicates, drops and retries.
- **Authenticated authority alerts** (`core::alert`, FR-42): Ed25519-signed
  alerts with a trusted-authority root store, key↔address binding and expiry, so
  a spoofed "evacuate now" is rejected offline.
- **Proof-of-relay** (`core::relay_proof`): portable, offline-verifiable claims
  built from **counterparty-witnessed** credits — a relay cannot self-mint
  evidence (directly answering Helium's spoofable proof-of-location). Kept
  strictly off the delivery path.
- **Onion routing / metadata privacy** (`core::onion`, FR-49): one SealedBox
  layer per relay on a chosen path; each relay peels exactly one layer and learns
  only the *next hop*, never the origin or (except the last relay) the recipient.
- **Adaptive retry** (FR-32): `router::respray` + an engine loop that re-sprays a
  message that stays unverified past a retry window (capped), so delivery retries
  on new paths rather than silently stalling.
- **Group messaging end-to-end** (FR-12): `core::group` sender-keys wired through
  `NodeEngine` (`create_group` / `add_group_member` / `send_group`) — sender-key
  distribution, single-encrypt fan-out to members, receive/decrypt with an
  out-of-order buffer. Tested: fan-out to all members, non-members excluded,
  multiple senders.
- **Custody receipts** (FR-25): signed `core::receipt::make/verify_custody_receipt`
  + `router::release_custody` (drop a copy once another node signs for custody,
  dedup-safe) — acknowledged hand-offs, no silent loss.
- **Sender-keys group encryption** (`core::group`): ratcheting per-sender
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

### Fixed
- **Flaky acceptance test**: `erasure_survives_lossy_partition` could fail near
  its threshold. Root causes addressed — the simulator now seeds node identities
  from its RNG (`Identity::generate_from_rng`) for reproducible runs, and the
  scenario uses a larger sample (40 messages) so the pass threshold is
  statistically meaningful. Verified stable over 15 consecutive runs
  (92–100% reconstruction vs a 90% floor).

### Notes
- This is a pre-1.0 engineering preview: the decentralized core, transport
  abstraction, and self-hostable app are functional and tested (all crates green),
  but real radio backends, a security audit, and several PRD features remain (see
  `STATUS.md` and `GAPS.md`).
