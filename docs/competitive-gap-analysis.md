# Competitive gap analysis — Nostr, bitchat, Buzz vs. Lifeline

Independent study (three fresh-context agents, primary sources only, mid-2026) of the
three systems nearest Lifeline, then a gap analysis against Lifeline's *actual* code.
Goal: find the capabilities a globally-serious decentralized offline messenger must
cover that we are missing, and separate them from the ones we already cover (sometimes
better). Each gap below is tagged **HAVE / PARTIAL / MISSING** against verified source.

## The three systems, in one line each
- **Nostr** — keypair-is-identity, self-authenticating events, dumb relays, outbox
  routing. Superb identity/portability; **weak** crypto messaging layer (no forward
  secrecy in NIP-44/17, recipient metadata leaks, no key rotation/recovery). The
  ecosystem's answer to the FS/group gap is **MLS via Marmot / White Noise**.
- **bitchat** (permissionless.tech, Dorsey-adjacent) — BLE-mesh, no accounts, Noise-XX
  live sessions, four-layer store-and-forward incl. day-rotated courier tags, Nostr
  internet fallback, triple-tap panic wipe. **Weakest part (their words):** stable
  8-byte sender IDs → passive enumeration/tracking; no cover traffic; no FS on stored mail.
- **Buzz** (Block, launched 2026-07-21) — Nostr/NIP-29 self-hostable team+agent
  workspace. **Deliberately not** E2E for group channels (relay stores plaintext for
  search) and **not** offline. Strong on: portable identity, agents-as-first-class
  keypairs, OS-keyring custody, human-in-the-loop moderation, extend-by-`kind`.

## What Lifeline already covers (validated by the studies — don't rebuild)
| Capability | Lifeline state | Note |
|---|---|---|
| Offline-first DTN store-carry-forward | **HAVE** | Our core reason to exist; Buzz has none, bitchat's is the model |
| Forward-secret 1:1 messaging | **HAVE** | Rotating prekey ring + retention window (`core/prekey.rs`); Nostr NIP-44 has *zero* FS |
| Set reconciliation on reconnect | **HAVE** | `lifeline-reconcile` (range-based) = Nostr's emerging NIP-77 Negentropy |
| Transport agnosticism | **HAVE** | `ExternalNet`/`BridgeInterface`; Nostr + Meshtastic bridges — Marmot calls this "the right instinct" |
| Signed, hash-linked audit log | **HAVE (better)** | Ours is *signed*; Buzz's is a keyless SHA-256 chain (tamper-evident, not tamper-resistant) |
| Extend-protocol-without-breaking-clients | **HAVE** | Version gate + unknown-tolerant enums ≈ Buzz's `kind` dispatch |
| Content-addressed media fetch | **HAVE** | CID + swarm fetch ≈ Nostr Blossom/NIP-94 |
| Anti-spam postage | **HAVE** | PoW postage ≈ NIP-13, plus black-hole reputation |
| Group message crypto (scale) | **HAVE** | Sender-keys/Megolm: O(1)/msg, FS *along the chain*, signed, reorder-tolerant — **but see G1** |

---

## The gaps, tiered

### Tier 1 — convergent, central to the thesis, real gaps

**G1. Post-compromise security + membership-change rekey for groups (MLS/TreeKEM).** — **PARTIAL**
All three studies converge here. Lifeline's group crypto (`core/src/group.rs`) is
Signal/Megolm **sender-keys**: forward-secret *along each chain*, but with **no
post-compromise security** and **no automatic rekey on membership change** — a leaked
sender key exposes all future messages until a manual rotation, and a removed member
keeps a working key until then. The 2026 state of the art (Nostr→**Marmot**, **White
Noise**) is **MLS / TreeKEM**: logarithmic-cost rekey, forward secrecy *and* PCS,
async membership. This is the single biggest crypto gap. The **differentiating** version
is MLS over DTN/mesh (async, offline Welcome/Commit propagation) — Marmot does it over
relays; nobody has done it well over an offline mesh. High effort, high payoff.

**G2. Metadata privacy: unlinkable presence identity + traffic-analysis resistance.** — **PARTIAL**
bitchat names stable sender IDs as its *"weakest part"*; Nostr leaks the recipient
p-tag, size, timing, IP even under gift-wrap. Lifeline seals content to rotating prekeys
(good) but (a) presence/beacon identity linkability needs a **per-epoch rotating
pseudonym**, (b) DTN carriers should address recipients by a **rotating tag = HMAC(recipient
key, epoch)** (bitchat's courier trick) so carriers route without learning who mail is
for, (c) `core/src/onion.rs` pads only the **innermost** cell and its own comment says
**"constant-size cells re-padded at every hop, Tor-style, remains future work"**, and
(d) there is **no cover/dummy traffic**. Convergent weak spot across all three.

**G3. Panic / duress wipe.** — **MISSING**
bitchat's triple-tap erases keys, mail, history, metrics. Lifeline has thorough
`zeroize`-on-drop but **no user-triggered emergency wipe** and no duress mode. For the
disaster / high-risk-user threat model this is table stakes, and it's cheap. Verified
absent (the "panic" hits in-tree are Rust `panic!`, not a wipe feature).

### Tier 2 — important, concrete, mostly known patterns

**G4. Key lifecycle: secure custody + rotation + revocation + recovery.** — **MISSING / PARTIAL**
Nostr's unsolved gap and Buzz's strong shipped pattern both point here. Lifeline rotates
*prekeys* and revokes *capabilities*, but has **no long-term identity-key rotation,
revocation, or recovery**, and keys are not in an OS keyring. Add: (a) **OS-keyring
custody** with outage-safe verified migration (Buzz's exact pattern — Keychain / Credential
Manager / Secret Service), (b) a **cold master key → rotatable subkey** scheme so
compromise/loss is survivable, (c) **social recovery** + a signed rotation announcement,
(d) optional **remote/hardware signer** seam (à la NIP-46) to keep the identity key off
the most-attacked device.

**G5. Trust & anti-spam layering under anonymity.** — **PARTIAL**
Nostr's lesson: anonymity breaks reputation filtering, so you need *layered* cost —
economic + web-of-trust + PoW + relay-auth, none sufficient alone. Lifeline has PoW +
black-hole reputation; **add a web-of-trust overlay** on the social/contact graph and a
**petname/favorites trust primitive that gates escalation** (bitchat: favoriting pins a
key and unlocks the internet path). Also human-readable **NIP-05-style handles** (as
*identification, not verification*).

### Tier 3 — deployment realism & moderation polish

**G6. Real-radio transport realities.** — **PARTIAL**
Lifeline is sim + UDP-mesh + LoRa/Meshtastic bridge; it lacks a **native BLE-mesh
transport** with bitchat's hard-won details: **contention-aware relay** (seeded
~log₂(degree) fanout, adaptive TTL clamping, 10–220 ms jitter — this is what stops BLE
broadcast storms), **BLE-MTU fragmentation** with bounded reassembly, and **adaptive
power / RSSI-gated duty cycling** (a node that flattens the battery in an hour is useless
in the scenario it's built for). Needed before a real handset deployment.

**G7. Moderation refinements.** — **PARTIAL**
We have moderation + priority. Buzz's model is more mature: **reports as private
structural state** (never in the event log, never fanned out), **signed role-checked
action commands** (admin can't ban an owner), **enforcement at the auth seam** (a ban
bites at authentication, not scattered filters), and **honest tombstones, no shadow-bans**.
Worth adopting the shape.

---

## Recommended incorporation order
1. **G3 panic/duress wipe** — cheap, high-value, closes an embarrassing gap fast.
2. **G2 metadata privacy** — rotating presence pseudonym + HMAC recipient tags for DTN
   carriers + per-hop constant cells; the convergent weak spot, and mostly within reach
   of existing crates (`onion`, `prekey`, router).
3. **G4 key lifecycle** — OS-keyring custody first (self-contained), then rotation/recovery.
4. **G1 MLS/TreeKEM group PCS over DTN** — the flagship. Biggest effort; do it as its own
   track (study Marmot's protocol-core + the Rust **MDK** first). This is also the most
   *differentiating* thing we could build (offline MLS).
5. **G5 web-of-trust + petname escalation**, then **G6 BLE realities**, then **G7 moderation shape**.

## Sources
Primary: [nostr-protocol/nips](https://github.com/nostr-protocol/nips),
[Marmot](https://github.com/marmot-protocol/marmot) + [MDK](https://github.com/parres-hq/mdk),
[permissionlesstech/bitchat](https://github.com/permissionlesstech/bitchat) (WHITEPAPER.md),
[block/buzz](https://github.com/block/buzz) (ARCHITECTURE/SECURITY/VISION docs, v0.5.0).
Lifeline state verified against `crates/core/src/{group,prekey,identity,onion,crypto}.rs`.
