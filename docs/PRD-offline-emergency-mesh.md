# Product Requirements Document — Project Lifeline (working title)

**A decentralized, offline-first emergency communication network.**
An app-first mesh that keeps people connected when the internet and cellular networks fail, and that "comes alive" the moment any single node touches connectivity.

| | |
|---|---|
| **Document version** | 1.0 (draft) |
| **Date** | 23 July 2026 |
| **Status** | For implementation / code generation |
| **Primary jurisdiction** | India (DoT / WPC) — design is jurisdiction-portable |
| **Related design docs** | Tech landscape, OSS catalog, App-first spectrum, Gateway model, Network-layer design, Component reference (see Appendix C) |

> **How to use this PRD with code generation:** Requirements are numbered (`FR-*`, `NFR-*`) and each critical module has explicit acceptance criteria and data schemas. Treat Section 11 (data models) and Section 12 (protocols) as the contract; Section 13 recommends concrete libraries so generated code targets real dependencies. Build in the phase order of Section 16.

---

## 1. Executive summary

Project Lifeline is a peer-to-peer messaging network whose **core product is a mobile and web/desktop app**. Every user with the app is a node. Nodes communicate directly using the radios and transducers already inside their devices (Bluetooth LE, Wi-Fi Aware, ultrasound, optical), forming a local mesh that needs no towers, SIMs, or internet.

Messages are **end-to-end encrypted** and delivered using **delay-tolerant, store-and-forward routing**: a message is carried, replicated, and relayed opportunistically until it reaches its recipient — even by physically moving devices ("data mules"). Users who own extra hardware (a LoRa radio, an internet uplink) can opt in as **privileged gateway nodes** that bridge the offline mesh to long-range frequencies and the wider internet. Because delivery is store-and-forward, **reaching any one gateway restores reach for the whole mesh.**

Delivery is made **verifiable** using decentralized cryptographic primitives (signed hash-linked logs, signed delivery receipts, CRDT state merge) — deliberately **without a global consensus blockchain**, which would require the very connectivity a disaster destroys.

The build is ~80% integration of hardened open-source components (Reticulum, libsignal, DTN Bundle Protocol, CRDTs) and ~20% new glue (sound/light transports, delivery-receipt protocol, gateway discovery, anti-abuse).

---

## 2. Problem statement & background

During disasters (tsunami, earthquake, flood, cyclone) and network shutdowns, cellular and internet infrastructure is frequently destroyed or overloaded exactly when communication matters most. Existing offline options each have gaps: satellite is restricted in India and expensive; ham radio needs a license and forbids encrypted traffic; single-hop Bluetooth apps don't scale; and no consumer product combines a phone-native mesh, opportunistic gateways, and verifiable delivery.

**Opportunity:** a purely software-installable app that turns ordinary phones into a resilient mesh, extensible with cheap gateways, with cryptographically provable delivery.

---

## 3. Goals & non-goals

### 3.1 Goals
- **G1** — Let two app users exchange messages with **no internet, no cellular, no servers**.
- **G2** — Deliver messages across a partitioned/multi-cluster area via store-and-forward, with **eventual delivery at high probability** and **cryptographic proof of delivery**.
- **G3** — Full **end-to-end encryption** with self-sovereign identity (no phone number, no central registrar).
- **G4** — Allow any node with connectivity or a radio to act as a **gateway** that bridges the mesh to the internet / long-range, restoring reach for the whole network.
- **G5** — Use only **license-free / lawful transports** by default (phone ISM radios + sound + light + LoRa on 865–867 MHz in India).
- **G6** — Prioritize **emergency/SOS traffic** above all other traffic at every hop.
- **G7** — Ship as a **mobile app** (primary) and a **web/desktop app** (gateway console + light use).

### 3.2 Non-goals (v1)
- **NG1** — Not a real-time voice/video platform (bandwidth-bound; async-first).
- **NG2** — Not a full web-browsing replacement; a gateway restores **reach, not bandwidth**.
- **NG3** — No global consensus blockchain in the delivery path.
- **NG4** — No reliance on satellite or ham transports in the default build (legal constraints; optional/adjacent only).
- **NG5** — No content moderation of ciphertext in transit (moderation is endpoint-side).

---

## 4. Target users & personas

- **P1 — General public ("Asha").** Non-technical smartphone user in a disaster-prone area. Wants to reach family and send "I'm safe" / SOS. Needs one-tap simplicity, works with zero setup once installed.
- **P2 — Volunteer / relief worker ("Ravi").** Coordinates on the ground; keeps the app running (relay node); may carry a LoRa gateway. Needs group messaging, location sharing, reliability.
- **P3 — Gateway operator ("Meera").** Tech-comfortable user or organization running a fixed node with LoRa and/or an internet uplink. Needs a console, status, and (later) incentive tracking.
- **P4 — Emergency authority / NGO.** Wants to broadcast alerts and receive field reports; integrates with official systems (e.g., India Cell Broadcast ingest). Needs authenticated broadcast identity and audit trails.

---

## 5. Key use cases & user stories

Each story has acceptance criteria (AC) usable as test cases.

- **UC1 — Offline 1:1 message.** *As Asha, I can send a text to a saved contact with no internet.*
  - AC1: With airplane-data off and Wi-Fi/cell unavailable but Bluetooth on, a message to an in-range contact is delivered and shows a **verified-delivered** state.
  - AC2: Message is end-to-end encrypted; a packet capture on a relay shows only ciphertext.
- **UC2 — Multi-hop delivery.** *As Asha, my message reaches a contact who is not in direct range.*
  - AC1: With relay nodes bridging two clusters, the message arrives; the app shows hop-count ≥ 2.
  - AC2: If no path exists yet, message state is **queued (carrying)**, not failed.
- **UC3 — Data-mule delivery.** *My queued message is delivered after a courier device physically moves into range of the recipient/gateway.*
  - AC1: A message queued while isolated is delivered after a carrier node transits, without user action.
- **UC4 — Gateway lights up the mesh.** *As Ravi, when one node gets internet, my out-of-area message is delivered.*
  - AC1: With all local towers down but one node on data, a message addressed to a remote recipient is bridged and a delivery receipt returns.
- **UC5 — SOS broadcast.** *As Asha, I can send an SOS with my GPS location that preempts other traffic.*
  - AC1: SOS is flagged highest priority; relays forward it ahead of normal messages; it reaches all gateways.
- **UC6 — Sound/light fallback.** *When Bluetooth is disabled/jammed, I can still transfer a message to a nearby phone via sound or a scanned code.*
  - AC1: With BLE off, an ultrasound exchange delivers a short message between two phones in the same room.
  - AC2: A streamed-QR exchange transfers a message across an air gap.
- **UC7 — Add contact securely.** *I can add a contact in person by scanning their code, establishing a trusted key.*
  - AC1: Post-scan, messages between the two are E2E encrypted with the exchanged keys; a MITM cannot impersonate.
- **UC8 — Gateway console (web).** *As Meera, I run a gateway from my laptop with a plugged-in LoRa radio.*
  - AC1: Web app drives the radio via Web Serial, shows queue depth, throughput, and connected peers.

---

## 6. Product scope

### 6.1 MVP (Phase 1)
Native mobile app; identity + E2E messaging; BLE + Wi-Fi Aware mesh; ultrasound fallback; store-and-forward with replication + signed receipts; internet gateway when any node has data; SOS priority. **Demo goal:** kill the towers, keep one phone on data, whole room still messages out with proof of delivery.

### 6.2 Phase 2 — Reach
LoRa gateway hardware integration (865–867 MHz); privileged-node registration & discovery; multi-gateway routing; optical (streaming-QR) transport; web/desktop gateway console (Web Serial); CRDT sync for state.

### 6.3 Phase 3 — Harden & integrate
Onion/metadata privacy; reputation + PoW anti-abuse; proof-of-relay incentive (optional); official-alert ingest (India Cell Broadcast); optional plaintext ham bridge for licensed operators; independent security audit.

---

## 7. System architecture (7-layer stack)

| Layer | Responsibility | Primary building block |
|---|---|---|
| **L7 Application** | Messaging, SOS, groups, alerts, gateway console | Native (Flutter/RN) + Web |
| **L6 Trust & anti-abuse** | PoW postage, reputation, endpoint moderation, (opt) incentive | Custom + Hashcash |
| **L5 Integrity & proof** | Hash-linked logs, signed receipts, CRDT merge | SSB-style logs + Automerge/Yjs |
| **L4 DTN routing & delivery** | Store-carry-forward, spray-and-wait, custody, TTL, priority | Reticulum + BP7 concepts |
| **L3 Discovery & naming** | Address = key; DHT online; gossip announce offline | Kademlia (libp2p) + announces |
| **L2 Identity & E2E crypto** | Keypair identity, X3DH, Double Ratchet, sealed sender | libsignal / Noise + libsodium |
| **L1 Interface abstraction** | Transport-agnostic framing | Reticulum interfaces |
| **L0 Transports** | BLE, Wi-Fi Aware, ultrasound, optical, LoRa, internet, data-mules | Platform APIs + ggwave + RNode |

The **decentralized core (L2 + L5 + L6)** functions fully offline and converges on reconnect.

---

## 8. Functional requirements

### 8.1 Identity & onboarding
- **FR-1** Generate an Ed25519/X25519 keypair on first launch, stored in platform secure storage (Keystore/Keychain).
- **FR-2** The user's network **address is derived from the public key** (hash/truncation); no phone number or email required.
- **FR-3** Display the user's identity as a shareable QR code and short fingerprint.
- **FR-4** Support optional local passphrase/biometric to unlock the key store.
- **FR-5** Support key backup/export (encrypted) and restore.
- **AC:** Two fresh installs can exchange identities by QR and message without any server.

### 8.2 Contacts & discovery
- **FR-6** Add a contact by scanning their QR (trust-on-first-use, verified key).
- **FR-7** Discover in-range nodes via BLE/Wi-Fi Aware advertisements (ephemeral, privacy-preserving IDs).
- **FR-8** When online, resolve a contact's reachable paths via the DHT; offline, via gossiped announces.
- **FR-9** Maintain a local contact store with verification state (verified / unverified / blocked).
- **AC:** Blocking a contact drops their messages at the endpoint and stops advertising to them.

### 8.3 Messaging
- **FR-10** Compose and send E2E-encrypted text messages (UTF-8), max size configurable (default 16 KB pre-compression).
- **FR-11** Support message states: `composing → queued → carrying → in-transit → delivered(verified) → read`, plus `expired` and `failed`.
- **FR-12** Support group messages (sender-keys model; membership as a CRDT set).
- **FR-13** Support small attachments (compressed images, ≤ 64 KB) via chunking + reassembly; larger via content-address fetch (Phase 2+).
- **FR-14** Support message priority classes: `SOS(0) > ALERT(1) > NORMAL(2) > BULK(3)`.
- **FR-15** Local, encrypted, searchable message history.
- **AC:** A delivered message displays a delivery receipt verifiable offline (see FR-30).

### 8.4 Transports (L0/L1)
Each transport implements a common **Interface** contract: `advertise()`, `scan()`, `open(peer)`, `send(frame)`, `recv() → frame`, `mtu`, `caps`.
- **FR-16** **BLE mesh** transport (primary): advertise presence, exchange frames with in-range peers; handle iOS background limits gracefully.
- **FR-17** **Wi-Fi Aware (NAN)** transport (Android) for higher bandwidth; Wi-Fi Direct fallback.
- **FR-18** **Ultrasound** transport via ggwave/Quiet (speaker+mic); used when radios are off/jammed or for phone↔laptop bridging. Configurable audible vs. near-ultrasonic mode.
- **FR-19** **Optical** transport: streaming/animated QR (screen↔camera) and flashlight-beacon mode.
- **FR-20** **LoRa gateway** transport (Phase 2): pair to an RNode/Meshtastic-compatible device over BLE/USB; enforce region = IN865, power ≤ 1 W.
- **FR-21** **Internet uplink** transport: when a node has connectivity, connect to a cloud relay / other gateways over TCP/QUIC (Reticulum TCP interface).
- **FR-22** Transports run concurrently; the router picks the best available per message.
- **AC:** Disabling any single transport does not stop delivery if another path exists.

### 8.5 Routing & delivery (L4)
- **FR-23** Implement **store-carry-forward**: hold messages locally with TTL until a forwarding opportunity appears.
- **FR-24** Implement **spray-and-wait** replication (configurable copy budget L, default 6) to bound overhead.
- **FR-25** Implement **custody transfer**: a receiving node acknowledges custody before the sender may drop its copy.
- **FR-26** Enforce **hop limit** and **TTL**; drop expired bundles; **deduplicate** by bundle ID.
- **FR-27** Priority queueing at every relay/gateway; SOS preempts all lower classes.
- **FR-28** **Erasure/fountain coding** for cross-partition messages (Phase 2) so partial carrier escape still reconstructs.
- **FR-29** Gateway selection via **gradient** toward the nearest/healthiest announced gateway.
- **AC:** Under a simulated 3-cluster partition with one moving mule, ≥ 95% of messages eventually deliver.

### 8.6 Integrity, proof & sync (L5)
- **FR-30** Each identity maintains an **append-only hash-linked log**; every sent bundle's hash is appended.
- **FR-31** On decrypt, the recipient emits a **signed delivery receipt** = `Sign_R(hash(bundle), timestamp)`; receipts propagate back opportunistically.
- **FR-32** The sender matches returned receipts to log entries to establish **verifiable delivery**; unmatched entries trigger retry on new paths.
- **FR-33** Shared state (group membership, presence, delivery status, blocklists) is modeled as **CRDTs** that merge deterministically after partitions heal.
- **FR-34** Provide a verification function: given a bundle + receipt + sender/recipient public keys, return valid/invalid **offline**.
- **AC:** Two partitions with divergent group edits merge with no conflicts and identical final state on reconnect.

### 8.7 Gateway node
- **FR-35** A node can be toggled into **gateway mode** (requires a bridging capability: internet and/or LoRa).
- **FR-36** Gateways broadcast a **signed announce** (capability, freshness, coarse load) into the mesh.
- **FR-37** Gateways bridge bundles between the local mesh and (a) the internet relay fabric and (b) long-range LoRa.
- **FR-38** Gateways only ever handle **ciphertext**; they cannot read/forge message content.
- **FR-39** Gateway console (web/desktop): show queue depth, peers, throughput, uplink status; drive LoRa via Web Serial/USB.
- **AC:** With one gateway online, a message from an otherwise-isolated cluster reaches a remote recipient and returns a receipt.

### 8.8 Emergency features
- **FR-40** One-tap **SOS** with GPS coordinates, battery %, and optional short note; priority SOS(0).
- **FR-41** **"I'm safe"** broadcast to saved contacts.
- **FR-42** Receive and display **authority broadcasts/alerts** (authenticated broadcast identity; Phase 3 Cell Broadcast ingest).
- **FR-43** Location sharing among a group with configurable interval.
- **AC:** SOS is delivered/queued even with an empty contact list (broadcast to any reachable node/gateway).

### 8.9 Security & anti-abuse (L2/L6)
- **FR-44** All messages E2E encrypted (X3DH + Double Ratchet); relays/gateways see ciphertext + minimal routing metadata only.
- **FR-45** **Sealed sender** to hide sender identity from relays.
- **FR-46** **PoW "postage"** per message (Hashcash-style), scaled inversely by priority; SOS exempt or minimal.
- **FR-47** **Reputation** for relays/gateways, gossiped; demote nodes with missing custody/delivery receipts.
- **FR-48** Endpoint moderation: report/block keys; import/gossip blocklists; no in-transit content inspection.
- **FR-49** Optional **onion wrapping** for metadata privacy (Phase 3).
- **AC:** A flood of low-priority messages from one identity is throttled without delaying SOS traffic.

### 8.10 Settings & platform
- **FR-50** Battery-saver mode (duty-cycle scanning/advertising).
- **FR-51** Per-transport enable/disable toggles.
- **FR-52** Storage cap for carried (non-owned) bundles with LRU eviction respecting TTL/priority.
- **FR-53** Diagnostics view: active transports, queue, known gateways, delivery stats.

---

## 9. Non-functional requirements

- **NFR-1 (Security).** E2E encryption on by default; keys never leave the device unencrypted; pass an independent security audit before public launch (Phase 3 gate).
- **NFR-2 (Privacy).** No central account, phone number, or PII required; minimize metadata exposed to relays; ephemeral rotating advertising IDs.
- **NFR-3 (Reliability).** ≥ 95% eventual delivery in the 3-cluster + mule simulation; no silent message loss (every message resolves to delivered/expired/failed with a reason).
- **NFR-4 (Performance).** Local 1-hop delivery < 3 s; receipt round-trip in a connected mesh < 10 s; app cold start < 2 s.
- **NFR-5 (Battery).** Background mesh participation ≤ ~5%/hour drain in battery-saver mode on mid-range hardware (target; validate empirically).
- **NFR-6 (Scalability).** Stable behavior in a 500-node dense cluster without broadcast storms (dedup + hop limits enforced).
- **NFR-7 (Portability).** iOS 15+, Android 10+; web app on Chromium (Web Bluetooth/Serial) + desktop. Graceful capability degradation where APIs are unavailable (esp. iOS background/peer limits).
- **NFR-8 (Offline-first).** All core functions work with zero connectivity; no feature may hard-depend on a server.
- **NFR-9 (Interoperability).** Wire formats versioned; target Reticulum/BP7 compatibility where feasible.
- **NFR-10 (Compliance).** Ship only lawful default transports (see Section 14); radio transports enforce regional band/power config.
- **NFR-11 (Accessibility).** Core flows usable one-handed, high-contrast, large-tap-target, localizable (English + major Indian languages).

---

## 10. Constraints & assumptions

- Web browsers cannot do raw Wi-Fi Aware or background operation; the web app targets **gateway-console + light proximity use**, not a background offline leaf.
- iOS restricts background BLE/peer APIs more than Android — validate the transport matrix per platform early.
- A **permanently isolated recipient cannot receive**; the system's obligation is to prove *whether* delivery occurred, not to defy physics.
- Satellite and ham transports carry legal constraints and are **not** in the default build.

---

## 11. Data models & schemas

All binary fields base64url in JSON representations; canonical wire format is CBOR (compact). Types indicative for code generation.

### 11.1 Identity
```json
{
  "id": "b64url(hash(pubkey))",        // network address
  "sign_pub": "b64url(ed25519_pub)",
  "kex_pub":  "b64url(x25519_pub)",
  "display_name": "string?",
  "created_at": 1690000000
}
```

### 11.2 Bundle (message envelope)
```json
{
  "v": 1,
  "bundle_id": "b64url(uuid_or_hash)",  // unique; used for dedup
  "dst": "recipient_address",
  "src_sealed": "b64url(sealed_sender_blob)",  // hides real sender from relays
  "priority": 0,                         // 0 SOS .. 3 BULK
  "created_at": 1690000000,
  "ttl_s": 604800,                       // default 7 days
  "hop_limit": 32,
  "hops": 0,
  "copies_left": 6,                      // spray-and-wait budget
  "ciphertext": "b64url(...)",           // Double Ratchet encrypted payload
  "sig": "b64url(sender_sig_over_header)"
}
```

### 11.3 Payload (post-decrypt, endpoint-only)
```json
{
  "type": "text|sos|safe|location|alert|receipt|group_op|attach_chunk",
  "body": "string?",
  "coords": {"lat": 0.0, "lon": 0.0, "acc_m": 0}?,
  "battery_pct": 0?,
  "attach": {"id":"", "idx":0, "total":0, "bytes":"b64url"}?,
  "group_id": "string?"
}
```

### 11.4 Delivery receipt
```json
{
  "type": "receipt",
  "bundle_id": "b64url",
  "recipient": "address",
  "delivered_at": 1690000000,
  "sig": "b64url(Sign_R(bundle_id || delivered_at))"
}
```

### 11.5 Custody receipt (relay-level)
```json
{ "bundle_id":"b64url", "custodian":"address", "at":1690000000, "sig":"b64url" }
```

### 11.6 Gateway announce
```json
{
  "type": "gw_announce",
  "gateway": "address",
  "caps": ["internet","lora_in865"],
  "load": 0.4,                 // 0..1 coarse
  "seq": 12345,                // monotonic freshness
  "expires_at": 1690003600,
  "sig": "b64url"
}
```

### 11.7 Hash-linked log entry
```json
{
  "seq": 42,
  "prev": "b64url(hash(prev_entry))",
  "event": "sent|received|receipt",
  "ref": "bundle_id",
  "at": 1690000000,
  "sig": "b64url(author_sig)"
}
```

### 11.8 Contact
```json
{ "address":"", "sign_pub":"", "kex_pub":"", "name":"", "verified":true, "blocked":false }
```

---

## 12. Protocol specifications

### 12.1 Message lifecycle
1. Sender builds **Payload**, encrypts with the Double Ratchet session to recipient → `ciphertext`.
2. Wrap in **Bundle**; seal sender; sign header; append `hash(bundle)` to sender's **log**.
3. Enqueue locally by priority; begin **spray-and-wait** (hand copies to up to `copies_left` distinct peers, decrementing).
4. Each receiving relay: verify header sig, dedup by `bundle_id`, decrement `hop_limit`, add a **custody receipt**, enqueue by priority.
5. On reaching a **gateway**, if `dst` is non-local, bridge over internet/LoRa; otherwise continue meshing.
6. Recipient decrypts, delivers to UI, emits a **delivery receipt**.
7. Receipt diffuses back; sender matches to log → state `delivered(verified)`. Unmatched after retry window → re-spray on new paths; at TTL → `expired`.

### 12.2 Discovery & gateway gradient
- Nodes broadcast ephemeral presence beacons on BLE/NAN.
- Gateways emit **signed announces** (12.6 schema) that propagate a few hops; each node caches best-per-gateway by `seq` and decays by `expires_at`.
- Router maintains a soft gradient (hops-to-nearest-gateway) to bias forwarding of gateway-bound bundles.

### 12.3 Anti-entropy sync (CRDT state)
- On peer contact, exchange **log frontiers** (latest `seq` per known author) and **CRDT version vectors**; transfer only deltas.
- Merge CRDTs deterministically; append any new log entries. Convergence target: one sync round per contact.

### 12.4 Delivery-proof verification
`verify(bundle, receipt, sender_pub, recipient_pub) -> bool`:
- Check `receipt.sig` over `bundle_id||delivered_at` with `recipient_pub`.
- Check `bundle.sig` over header with sender identity.
- Confirm `receipt.bundle_id == bundle.bundle_id`. Runs fully offline.

### 12.5 Priority & fairness
- Strict priority dequeue (SOS first) with per-sender fair-queuing within a class to prevent monopolization; PoW postage gates admission for NORMAL/BULK.

---

## 13. Technology stack (recommended)

> Chosen to maximize reuse of audited components. Alternatives noted.

- **Networking core / routing / transport abstraction:** **Reticulum (RNS)** — provides identity-based addressing, transport-agnostic interfaces, and transport-node bridging (your gateway model). *Prototype in Python (reference RNS); production core candidate: Rust reimplementation for mobile embedding.*
- **E2E crypto:** **libsignal** (Double Ratchet + X3DH + sealed sender) or **Noise Protocol** + **libsodium** if a lighter/ DTN-tuned ratchet is preferred. Verify libsignal licensing/usage terms.
- **DTN delivery semantics:** custody/bundle concepts per **RFC 9171 / DTN7**; spray-and-wait implemented in the router.
- **Discovery:** **libp2p Kademlia DHT** (online) + custom gossip announces (offline).
- **Integrity & sync:** **Automerge** or **Yjs** (CRDTs); **Secure Scuttlebutt-style** append-only hash-linked logs (design pattern, can be custom + libsodium signatures).
- **Data-over-sound:** **ggwave** (primary) / **Quiet** — cross-platform, browser-capable.
- **LoRa gateway:** **RNode firmware** / **Meshtastic**-compatible hardware (IN865).
- **Mobile app:** **Flutter** or **React Native** with native platform channels for BLE (CoreBluetooth/Android BLE), Wi-Fi Aware, camera/flashlight, audio (ggwave), secure storage. Native modules where background radio work demands it.
- **Web/desktop app:** Web Bluetooth + **Web Serial/WebUSB** (gateway console) + **Web Audio** (ultrasound) + camera (QR). Optional Electron/Tauri desktop wrapper.
- **Anti-abuse:** Hashcash PoW; custom reputation gossip.
- **Optional incentive layer (Phase 3):** proof-of-relay settled off-path to a ledger when online (Helium-style precedent); keep entirely out of the delivery path.

### 13.1 Suggested module structure (monorepo)
```
/core            # Rust: identity, crypto, bundle, router (DTN), logs, CRDT bridge
/transports
  /ble  /nan  /sound  /optical  /lora  /internet
/proto           # schemas (CBOR/JSON), wire versioning, codegen targets
/sync            # anti-entropy, CRDT, receipts, verification
/gateway         # bridge logic, announce, console API
/app-mobile      # Flutter/RN UI + platform channels
/app-web         # gateway console + light client
/sim             # network simulator for AC tests (partitions, mules)
/docs            # this PRD + design docs
```

---

## 14. Compliance & governance (India-first)

- **CR-1** Default transports must be **license-free**: phone ISM radios (BLE, Wi-Fi, UWB, NFC), **sound** and **light** (unregulated), and **LoRa on 865–867 MHz ≤ 1 W** (Low-Power SRD exemption). LoRa transport must hard-enforce region=IN865 and power cap.
- **CR-2** **No satellite transport** in the default build; satellite bridging only via a DoT/GMPCS-authorized operator (e.g., BSNL) as a separate, permissioned integration.
- **CR-3** **Ham radio bridge (optional, Phase 3)** must carry **plaintext, non-commercial, emergency** traffic only, operated by a licensed operator — never the E2E-encrypted general stream (amateur rules forbid encrypted/obscured content and restrict third-party/commercial traffic).
- **CR-4** Governing framework: **Telecommunications Act 2023**, administered by **DoT/WPC**; verify current rules before shipping radio features.
- **CR-5** Provide clear in-app disclosure of which transports are active and their legal basis; allow authorities' lawful requirements to be met without weakening E2E (endpoint-side, not backdoors).

---

## 15. Threat model & security requirements

Informed by the **Breaking Bridgefy** analyses (CT-RSA 2021; USENIX Security 2022): *correct ciphers are not enough — harden the mesh/framing layer and audit the whole path.*

| Threat | Mitigation | Requirement |
|---|---|---|
| Eavesdropping on relays | E2E encryption; ciphertext-only relaying | FR-44 |
| Sender deanonymization | Sealed sender; ephemeral advertising IDs | FR-45, NFR-2 |
| Impersonation / MITM | Verified key exchange (QR TOFU); signed headers | FR-6, FR-44 |
| Malicious gateway drops/delays | Multi-gateway + end-to-end receipts detect; reputation demotes | FR-31, FR-47 |
| Message forgery/replay | Signatures + unique bundle IDs + dedup | FR-26, 12.4 |
| Spam / DoS flooding | PoW postage + priority fair-queuing + rate limits | FR-46, 12.5 |
| Sybil identities | PoW identity cost + earned reputation + web-of-trust | FR-46, FR-47 |
| Framing/parsing exploits | Strict, fuzzed CBOR parsing; versioned wire format; audit | NFR-1, NFR-9 |
| Harmful content | Endpoint moderation (report/block/blocklists) | FR-48 |

**Security gates:** fuzz-test all wire parsers; third-party audit before public launch; no custom crypto primitives (use audited libraries).

---

## 16. Milestones & acceptance

| Phase | Milestone | Exit criteria |
|---|---|---|
| **P1.0** | Identity + local E2E chat over BLE | UC1, UC7 pass on iOS+Android |
| **P1.1** | Multi-hop + store-carry-forward + receipts | UC2, UC3 pass; NFR-3 in sim (≥95%) |
| **P1.2** | Wi-Fi Aware + ultrasound fallback + SOS | UC5, UC6 pass |
| **P1.3** | Internet gateway (any node w/ data) | UC4 pass — "one node lights the mesh" demo |
| **P2.0** | LoRa gateway (IN865) + gateway discovery | Cross-cluster delivery via LoRa; CR-1 enforced |
| **P2.1** | Web gateway console (Web Serial) + optical | UC8 pass |
| **P2.2** | CRDT sync + erasure coding | FR-33, FR-28 AC pass |
| **P3.0** | Anti-abuse (PoW+reputation) + onion metadata | FR-46/47/49 AC pass |
| **P3.1** | Security audit + alert ingest + (opt) incentive | NFR-1 gate cleared; launch-ready |

---

## 17. Success metrics (KPIs)

- **M1** Offline delivery success rate (target ≥ 95% eventual in reference sim; measure in field pilots).
- **M2** Median local-hop delivery latency (< 3 s).
- **M3** % messages with verified delivery receipt.
- **M4** Battery drain/hour in background (≤ target).
- **M5** Max stable cluster size without storming (≥ 500 nodes).
- **M6** Gateway coverage: median hops-to-nearest-gateway in a pilot area.
- **M7** Time-to-deliver after a single gateway comes online in a partitioned cluster.

---

## 18. Open questions & risks

- **OQ1** iOS background BLE/peer limits — how much offline relaying is feasible without the app foregrounded? (De-risk in P1.0.)
- **OQ2** Reticulum embeddability on mobile — reference Python vs. a Rust core; effort of a production reimplementation.
- **OQ3** Double Ratchet under multi-day one-way delays — validate or adopt a DTN-tuned async variant.
- **OQ4** PoW postage cost tuning — deter spam without harming low-end devices/urgent traffic.
- **OQ5** Incentive layer — needed for gateway density, or does goodwill/institutional deployment suffice? Legal/tax implications of any token.
- **OQ6** Group-message security at scale (sender-keys) — revocation and membership-race handling via CRDT.
- **R1 (risk)** Rolling any custom crypto/framing → follow Bridgefy lessons: reuse audited libs, fuzz, audit.
- **R2 (risk)** Regulatory change on LoRa/satellite/spectrum — keep transport config server-updatable within legal bounds.

---

## Appendix A — Glossary
**Bundle** — the encrypted message envelope. **Custody transfer** — a relay assuming responsibility for a bundle before the prior holder drops it. **Data mule** — a moving device that physically carries queued bundles. **DTN** — delay/disruption-tolerant networking. **Gateway** — a privileged node bridging the mesh to internet/long-range. **Spray-and-wait** — bounded-copy epidemic routing. **CRDT** — conflict-free replicated data type. **Sealed sender** — hiding sender identity from relays. **PoW postage** — proof-of-work attached to a message to deter spam.

## Appendix B — Default parameters
TTL 7 days · hop_limit 32 · spray copies 6 · max text 16 KB · max attach chunk 64 KB · announce TTL 1 h · retry window 60 s (connected) / adaptive (partitioned) · carried-bundle store cap configurable (default 200 MB, LRU).

## Appendix C — Reference design docs & literature
Companion artifacts: *Offline comms tech landscape*, *Consumer-tech guide*, *Open-source catalog*, *App-first spectrum & protocol*, *Gateway-bridge architecture*, *Decentralized network layer*, *Component reference (OSS + papers)*. Key papers: Fall (SIGCOMM 2003); Vahdat & Becker (2000); Spyropoulos et al. (2005); RFC 9171; Maymounkov & Mazières (2002); Cohn-Gordon et al. (2017); Shapiro et al. (2011); Tarr et al. (2019); Kleppmann et al. (2019); Douceur (2002); Back (2002); Albrecht et al. (2021, 2022); Nandakumar et al. (2013). Full citations and links in the Component Reference doc.

---
*This PRD is a living document intended for iterative refinement via code generation. It is a design/engineering brief, not legal advice; verify all spectrum, satellite, amateur-radio, and licensing requirements with current DoT/WPC regulations before shipping any radio-transport feature.*
