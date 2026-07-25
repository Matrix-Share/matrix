# Nostr: what to learn, and how to integrate

*A strategy note. Goal: **augment** the existing ecosystem — plug Lifeline into
Nostr's already-adopted relay network for internet reach — rather than rebuild a
lesser version of something that already works.*

Sourced from the Nostr NIPs (`github.com/nostr-protocol/nips`, esp. NIP-01, 44,
17, 59, 65, 40, 42, 13, 19) and BitChat's design (permissionlesstech whitepaper +
DeepWiki), July 2026.

---

## 1. What Nostr is (in one breath)

"Notes and Other Stuff Transmitted by Relays." Identity is a **secp256k1
keypair** (no accounts, no servers). Everything is a **signed JSON event**
`{id, pubkey, created_at, kind, tags, content, sig}` where `id = sha256(canonical
array)` and `sig` is a BIP-340 Schnorr signature. Clients publish events to
**dumb relays** (WebSocket: `EVENT` / `REQ` / `CLOSE` → `EVENT` / `EOSE` / `OK` /
`CLOSED`) and subscribe with **filters** (`authors`, `kinds`, `#<tag>`,
`since`/`until`, `limit`). Relays **store-and-forward**: "regular" events persist,
so an **offline recipient's messages sit on a relay until they reconnect**. All
intelligence is in the client; relays are interchangeable commodity servers, so
censoring one just means using another.

**Why it matters to us:** those relays are a **free, global, already-adopted
store-and-forward internet fabric** with real market presence — exactly the
"internet gateway" role our own `lifeline-relay` plays, but with thousands of
public instances we don't have to run.

## 2. The precedent: BitChat already does mesh + Nostr

BitChat (Jack Dorsey / permissionlesstech) is a **BLE-mesh messenger with Nostr
as its internet bridge** — the same shape as Lifeline. A `MessageRouter` "prefers
a live mesh link, falls back to Nostr, and engages the courier system when
neither can deliver promptly." Concretely:

- **Private messages** → **NIP-17/59 gift-wrapped** events (`kind:1059`,
  throwaway per-message key, NIP-44 content) published to public relays; the
  recipient resubscribes with a **24-hour lookback** to collect offline mail.
- **Geohash channels** → public regional chat as **`kind:20000` ephemeral events
  tagged `#g <geohash>`**; anyone subscribed to that geohash filter receives it.
  Presence is `kind:20001`. A **new pseudonymous Nostr key per geohash**, derived
  `HMAC-SHA256(device_seed, geohash)`, prevents cross-location correlation.
- The bridge lives **at the sender's device**, not a mesh gateway — so internet
  traffic never floods back through the RF mesh (the failure mode of
  Meshtastic-over-MQTT bridges).

This is direct proof the pattern works and has traction. It also tells us where
the bar is — and where we already clear it.

## 3. Where Lifeline already *leads* (don't reinvent)

We are not a Nostr client with worse tooling. On the axes that matter for an
emergency mesh, Lifeline already has capabilities Nostr/BitChat lack:

| Capability | Nostr / BitChat | Lifeline |
|---|---|---|
| **Forward secrecy** | ❌ NIP-44 uses a **static** conversation key — a leaked key decrypts *all* past DMs (NIP-EE/MLS is a future fix) | ✅ **rotating prekeys** with a retention window (`core::prekey`) |
| **Verifiable delivery** | ❌ no delivery proof | ✅ signed delivery + custody receipts, verified offline |
| **Offline / multi-bearer** | internet relays (BitChat adds BLE) | ✅ BLE + Wi-Fi Aware + **ultrasound + optical + LoRa** + internet behind one `Interface` seam |
| **Physical-carrier DTN** | relay store-and-forward only | ✅ store-carry-forward + **spray-and-wait + erasure coding + data mules** |
| **Large content** | events are small JSON; big/binary impractical | ✅ **content-addressed blocks** (BLAKE3 CID) fetched by hash |
| **Bandwidth adaptation** | none | ✅ compression + **priority-aware bearer selection** (SOS preempts) |
| **Gateway routing** | per-sender bridge | ✅ **signed gateway announces + gradient**, bundles flow downhill |

**Positioning:** Lifeline is the *resilience + verifiability* layer; Nostr is the
*reach + adoption* layer. Bridging them is **symbiotic** — Lifeline gains a global
relay network and market presence; the Nostr world gains a mesh/gateway extension
that keeps working when the internet doesn't.

## 4. What to learn / adopt from Nostr

1. **Nostr relays as an internet fabric** (the big one) — a `NostrInterface`
   lets any two Lifeline nodes reach each other over public Nostr relays with
   **no `lifeline-relay` to operate**, and get relay-backed offline mailboxing
   for free.
2. **Outbox model (NIP-65)** — nodes advertise which relays reach them; discovery
   is follow-the-pointers, no central directory. Maps onto our gateway announces
   and contact records (a contact can carry "my Nostr inbox relays").
3. **Gift-wrap metadata hygiene (NIP-59)** — throwaway per-message envelope key +
   **`created_at` randomized ±2 days** + power-of-two padding. Cheap wins for our
   internet path (we already have sealed sender + onion + prekeys; adopt the
   timestamp/padding hygiene).
4. **Geohash public channels** — `kind:20000` + `#g` is a beautiful fit for
   **regional emergency broadcast** ("anyone near this location, SOS") without
   any pre-existing contact — something our contact-graph model can't do today.
5. **Align our anti-abuse with Nostr's** — NIP-13 PoW ≈ our Hashcash postage;
   NIP-40 expiration ≈ our bundle TTL; NIP-42 AUTH ≈ recipient-only delivery. Use
   compatible framings so a Lifeline↔Nostr bridge is clean.

## 5. Integration design — `NostrInterface`

The `Interface` seam means Nostr is **"just another network task"**; the
`NodeEngine` does not change. A `NostrInterface` is a `ChannelInterface` (already
in `transport`) fed by a `nostr_client` async task — exactly how `lifeline-relay`
is wired today, but speaking Nostr WebSocket/JSON instead of our TCP framing.

```
NodeEngine ──frames──> ChannelInterface ──> nostr_client task ──WebSocket──> [ Nostr relays ]
   (unchanged)                                  │  wrap bundle as Nostr event, sign, publish
                                                └── subscribe (REQ) for our events, unwrap → frames
```

**Bundle ↔ event mapping.** Our `Bundle` is *already* opaque E2E ciphertext, so
it needs no Nostr-level encryption for confidentiality — but we use Nostr's
gift-wrap for **metadata**:

- **Directed (mailbox) mode.** Wrap the bundle bytes (base64) as the content of a
  **`kind:1059` gift wrap** (NIP-59), signed by a **throwaway secp256k1 key**,
  `p`-tagged to the recipient's Nostr pubkey, `created_at` randomized. Publish to
  the recipient's inbox relays. The recipient's `nostr_client` subscribes for its
  `#p`, pulls (24h lookback on reconnect), unwraps → hands the bundle frame to the
  engine, which decrypts/verifies as usual. Relay-backed store-and-forward.
- **Geohash / broadcast mode.** For SOS / regional alerts, publish a
  **`kind:20000` `#g <geohash>`** event carrying the (still E2E-or-signed) bundle.
  Any Lifeline node subscribed to that geohash receives it — emergency reach with
  no prior contact.

**Identity mapping.** A node derives a **secp256k1 Nostr key** from a device seed
(separate from its Ed25519 identity, like BitChat), and **per-geohash ephemeral
keys** via `HMAC-SHA256(seed, geohash)` for unlinkability. To let contacts find
each other on Nostr, extend the signed identity/beacon record (or a contact card)
with the node's **Nostr pubkey + inbox relays** (self-signed, à la NIP-65). This
mirrors the `SignedPrekey` we just added.

**New dependencies** (all permissive): `secp256k1`/`k256` (Schnorr), a WebSocket
client (`tokio-tungstenite`), `serde_json` (already have serde). Isolated to the
`node` crate's network task — core/transport stay pure.

## 6. Honest tradeoffs to design around

- **Relay trust / availability.** Relays can drop, throttle, or vanish — publish
  to **several** (redundancy), and keep `lifeline-relay` and the mesh as
  independent paths. Never depend on a single relay.
- **Recipient metadata.** A gift wrap still exposes the **recipient's Nostr
  pubkey** to the relay. Mitigate with per-recipient/rotating Nostr keys and by
  reserving Nostr for the internet leg (the mesh legs stay sealed-sender + onion).
- **Message size / rate limits.** Events are small JSON — fine for text/SOS, bad
  for large blobs; route big content through our **content-addressed blocks**,
  not Nostr. Respect relay rate limits (our postage/priority already help).
- **Ephemeral retention.** `kind:20000/20001` may not persist; use gift-wrapped
  `kind:1059` (stored) for anything that must survive offline.

## 7. Recommendation & phased plan

Build the **`NostrInterface`** as the next feature — highest leverage: it plugs
Lifeline into a global, adopted relay network for internet reach and offline
mailboxing, reuses our existing seams (zero engine changes), and lets us lead
with the capabilities Nostr lacks (forward secrecy, verifiable delivery,
multi-bearer offline, erasure).

1. **Phase 1 — directed mailbox.** `nostr_client` task + `ChannelInterface`;
   node Nostr identity; publish/subscribe gift-wrapped bundles to a mapped
   recipient over a configurable relay set; offline mailbox via 24h lookback.
   *Outcome: two Lifeline nodes talk over public Nostr relays, no `lifeline-relay`.*
2. **Phase 2 — geohash emergency broadcast.** `kind:20000 #g` SOS/regional
   channels; per-geohash ephemeral keys. *Outcome: reach "anyone near a location"
   with no prior contact.*
3. **Phase 3 — true interop.** Publish a Lifeline node's Nostr pubkey + inbox
   relays (NIP-65-style) so **Nostr-native users can DM Lifeline users** and vice
   versa; optional read-only geohash viewers (like bitchat-world-view) for
   situational awareness.

Each phase is a self-contained PR behind the `Interface` seam, keeps core/
transport untouched, and is testable with a mock relay.
