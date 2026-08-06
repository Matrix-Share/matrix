# Reddit launch variants (#111)

**Never cross-post identical text.** Each subreddit has its own culture and
self-promotion rules — read the sidebar first, and space posts a day or two apart.
Lead with what *that* community cares about. Reply to comments.

---

## r/rust
**Title:** `Lifeline: an offline, E2E-encrypted mesh messenger in Rust (15 crates, ~292 tests)`

Body — lead with engineering:
- Workspace layout: core/transport/router/sync/crypto crates; memory-safe by design.
- The interesting bits: sealed-sender envelopes, a forward-secret DTN ratchet with
  rotating prekeys, store-carry-forward routing, a pluggable transport seam, CRDT
  anti-entropy sync.
- SSDLC: clippy/fmt gate, fuzz targets, `cargo-deny`, OpenSSF Scorecard.
- Apache-2.0. Link the repo; invite critique of the architecture.
- Ask a real question ("how would you model the bearer seam?") to spark discussion.

## r/privacy  (and cross-adapt for r/programming)
**Title:** `Lifeline: mesh messaging with no servers, no accounts, no phone numbers`

Body — lead with the threat model:
- No central server means no metadata honeypot to subpoena or seize.
- E2E by default; relays move sealed envelopes and can't see sender/recipient.
- Forward secrecy; panic-wipe under duress.
- **Be candid** about current metadata limitations and the not-yet-audited status.
- Contrast with Signal (needs servers + phone number) honestly.

## r/meshtastic  (and r/amateurradio, adapted)
**Title:** `Phone-only mesh messenger with store-carry-forward + internet bridge (no LoRa needed)`

Body — respect the audience's expertise:
- Acknowledge Meshtastic's strengths (range, LoRa, maturity) up front.
- Lifeline's niche: no extra hardware; delay-tolerant carry when nodes are mobile;
  MQTT-style gateway analog so one connected phone drains the mesh.
- Honest on range: BLE/Wi-Fi is short-range vs LoRa — density is the trade.
- Ask for real-world testing help.

## r/preppers
**Title:** `Off-grid messaging that turns a crowd of phones into a network`

Body — lead with the scenario:
- Disasters/blackouts/shutdowns take out towers first; a phone mesh routes around it.
- SOS + live location, group channels, works with zero infrastructure.
- Plain-language, not jargon. Note it's alpha — a tool to watch/test, not to bet a
  life on yet. Point to the use-cases page.

---

## Universal rules
- [ ] Check each sub's self-promotion policy; some require a participation ratio.
- [ ] Post from an account with real history, not a fresh throwaway.
- [ ] Front-load the demo GIF (Reddit loves inline media).
- [ ] Disclose you're the author.
- [ ] Reply within the first hour — early engagement drives the ranking.
