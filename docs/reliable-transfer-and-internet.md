# Reliable multi-carrier transfer, and internet-over-mesh

Two designs, both taking their cues from how the internet and the web were
actually built.

---

## Part 1 — "Use every carrier, get all the data, exactly once"

The disaster requirement: a message/object should ride **every** available carrier
(BLE, Wi-Fi Aware, LoRa, ultrasound, internet, Nostr, Meshtastic…) simultaneously,
and the recipient must end up with **all** of it, with **no duplicates** and **no
corruption** — over links that are lossy, out-of-order, and intermittent.

### What the internet's designers already solved (and how Lifeline mirrors it)

| Principle (where it came from) | What it guarantees | Lifeline's mechanism |
|---|---|---|
| **End-to-end principle** (Saltzer–Reed–Clark '84) | reliability belongs at the *endpoints*, not per-hop — links can be dumb/lossy | the E2E sealed bundle is verified only by the recipient; relays are dumb carriers |
| **The IP hourglass / narrow waist** | one packet format over *many* link layers | the opaque `Bundle` is the waist; every carrier is an `Interface` |
| **TCP: sequence numbers + ACK + retransmit + reassembly** | complete, ordered, gap-free delivery over an unreliable channel | fragmentation/reassembly (`frame`), selective-repeat **ARQ** on lossy links, adaptive re-spray |
| **IP fragmentation (id + offset)** | one datagram crosses small-MTU links and is reassembled | per-interface MTU fragmentation + reassembly |
| **Content addressing** (Git/IPFS/BitTorrent) | a chunk is *self-verifying* (name = hash) and *globally deduplicated* | `content::cid_of` (BLAKE3), `Manifest` (a merkle root over chunk CIDs) |
| **BitTorrent swarm** | fetch different chunks from different peers in parallel; verify each; assemble | `BlockStore::missing(manifest)` (the gap set) + solicited, CID-verified block fetch |
| **Multipath TCP** | one transfer striped across multiple paths, reassembled at the receiver | the engine drives *all* interfaces concurrently and dedups by `bundle_id` |
| **HTTP conditional GET / ETags** | don't re-transfer what the peer already has | **set reconciliation** (`lifeline-reconcile`) + delta ideas (`differential-transfer.md`) |

### How the three guarantees are already met

- **No duplicates.** Two independent dedup layers: (1) the router dedups whole
  bundles by `bundle_id` (`seen`), so the same bundle arriving over BLE *and* LoRa
  is stored once; (2) chunks are **content-addressed** — the same content has the
  same CID and is stored once, no matter how many carriers or peers deliver it.
  Duplicate arrivals are *idempotent*, which is exactly what you want when
  spraying over every carrier at once.
- **Completeness.** A `Manifest` is the authoritative list of every chunk CID in
  the object (a merkle root). You have "all the data" **iff** `missing(manifest)`
  is empty; `reassemble(manifest)` then verifies the whole object end-to-end. This
  is the definition of done — no guessing.
- **Integrity.** Every chunk is verified against its CID on receipt (`cid_of ==
  cid`), and the manifest root is signed end-to-end. A carrier cannot inject a
  corrupt or substituted chunk.

So the *core* of Part 1 is **already in place** — it falls straight out of
content-addressing (no-dupe + integrity) + a manifest (completeness) + a
multi-interface engine (all carriers) + ARQ/reassembly (lossy links).

### The one genuine gap: **swarm (multi-source) fetch** — now built

Previously `fetch_content(manifest, from)` pulled missing blocks from **one**
named provider. The BitTorrent lesson is to pull the gap from **whoever has it,
over whatever carrier, in parallel**. This is now implemented as
`NodeEngine::fetch_content_swarm(manifest, providers, now)`:

1. **HAVE discovery.** The fetcher sends each provider a `HaveQuery` listing the
   still-missing CIDs of the object; a provider answers `HaveReply` with the
   subset it holds. The fetcher records a per-CID holder set — a BitTorrent-style
   HAVE bitmap, but scoped to one object's manifest.
2. **Spread + rotate.** Each missing block is requested from a provider known to
   hold it (else any provider), with the choice offset by a per-fetch rotation
   counter and the block index — so different blocks go to different providers in
   the same round (parallel multi-source), and a block a provider failed to
   deliver is re-asked of a *different* provider next round (routes around a dark
   or black-hole provider). No block is requested from more than one provider per
   round, so there is no duplicate traffic.
3. **Monotone + idempotent.** Completeness only ever grows (the `missing` set
   shrinks) and blocks are content-addressed, so parallel/duplicate arrivals are
   harmless and the object provably completes.

Verified by `swarm_pulls_disjoint_blocks_from_two_providers` (two providers each
holding half the blocks — completion *requires* both) and
`swarm_routes_around_a_dead_provider` (the primary provider goes dark; the fetch
completes via the live one). `lifeline-reconcile` remains the tool for
*whole-store* CID-set sync between peers; HAVE queries are the per-object case.

---

## Part 2 — Internet access over the mesh (node-authorized)

The idea: if **one** node has real internet and is reachable over the mesh, let
authorized users reach the internet *through* it — the mesh becomes the access
network, that node the exit. Precedents: a **SOCKS/HTTP proxy**, a **VPN/Tor exit
node**, café **captive portals**, an **eSIM/APN** bearer.

**The critical security distinction (the user's requirement):**
> Ordinary **mesh messages are open** to everyone (any node relays any bundle).
> Actual **internet egress is node-level authorized** — a gateway forwards to the
> internet *only* for identities it has granted, and refuses everyone else, while
> still relaying their mesh messages.

This is **capability-based access control at the exit**, cleanly separate from the
open store-carry-forward relay of L4.

### DTN-appropriate shape: request/response, not live sockets

A live TCP tunnel over a high-latency, intermittent DTN is impractical. The web's
own **request/response** model fits perfectly and is latency-tolerant: the client
sends a sealed `NetRequest{method, url, headers, body}` addressed to the gateway;
the gateway authorizes it, performs the fetch on the real internet, and returns a
sealed `NetResponse{status, headers, body}` back over the mesh (large bodies
chunked via `content`/erasure). This is a **store-and-forward web proxy** — "fetch
me this URL / submit this API call," carried by the mesh. (A low-latency one-hop
BLE/Wi-Fi link could later carry a live SOCKS tunnel; the request/response form is
the universal, DTN-safe base.)

```
 app → local proxy → seal NetRequest → mesh (any carriers) → gateway
                                                              │ authorize(requester)?
                                                    yes ──────┤ fetch() ─→ internet
                                                     no  → refuse            │
 app ← local proxy ← open NetResponse ← mesh ←───────────────┴──────────────┘
```

### Security model

- **E2E-sealed request & response.** The `NetRequest`/`NetResponse` are sealed
  to/from the gateway with the ordinary E2E path, so relays carrying the bundle
  see only that *some* bundle is bound for the gateway — never the URL, headers,
  cookies, or response. (The gateway itself necessarily sees the plaintext request
  — it's the exit — exactly like a VPN/Tor exit or your ISP; users choose which
  gateway to trust.)
- **Authorization is per-identity and explicit.** The gateway holds an
  `AccessPolicy` (allow-list / signed grants / postage-paid quota). An
  unauthorized requester's `NetRequest` is answered with a signed *refusal*, and
  no fetch happens — but that node's ordinary mesh messages still relay normally.
  The full authorization model — attenuatable, offline-verifiable **capability
  tokens** and the `ServiceClass` egress tiers — is its own design record:
  [`capability-egress-and-service-class.md`](capability-egress-and-service-class.md).
- **SSRF / abuse guards on the exit.** An exit that fetches arbitrary URLs is a
  server-side-request-forgery and abuse risk. The gateway MUST: allow only
  `http`/`https`; reject `localhost`, `*.local/.internal`, and literals in
  loopback/private/link-local IP ranges; and (in the real fetcher) re-check the
  *resolved* IP to defeat DNS-rebinding. Rate-limit and size-cap per requester.
- **Accountability.** Grants are revocable; a gateway operator opts in and scopes
  who/what/how-much, because they bear the egress liability.

### Module shape (built alongside this doc)

`lifeline-inet`: the transport-agnostic core, tested without a network —
- `NetRequest` / `NetResponse` wire types;
- `AccessPolicy` trait + an `AllowList` (grant/revoke);
- a `Fetcher` trait (real HTTP behind a feature; a mock for tests);
- `InternetGateway::handle(requester, req)` = authorize → SSRF-check → fetch →
  seal response, refusing the unauthorized without fetching;
- `is_safe_url` SSRF guard.

The client-side **local proxy** (an OS HTTP/SOCKS proxy that serializes app
requests into sealed `NetRequest` bundles) and the real `reqwest`/`ureq` fetcher
are the integration layer, noted as the next step.
