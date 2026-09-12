# Reddit + community launch (#111, #112)

Rule of thumb for every one of these: **lead with the honest status, engage as a
person, never copy-paste the same text across subs** (crossposting identical
marketing reads as spam and gets removed). Read each sub's self-promotion rules
first; several require you to be an active participant before posting a project.

The mesh/DTN/privacy crowd is exactly your Stage-0 audience — they reward candor
and will happily help finish the radio layer. Do NOT go to general-consumer subs
yet; the product isn't consumer-ready and you'll burn goodwill.

---

## r/meshtastic  (most aligned — they already live in phone-to-phone mesh)

**Title:** `Lifeline: an open-source DTN mesh messenger with offline-verifiable delivery receipts (alpha, looking for radio-layer help)`

**Body:**
> I've been building Lifeline — an open-source, E2E-encrypted store-carry-forward
> mesh messenger. Different emphasis from Meshtastic: it's a full DTN with
> custody transfer, CRDT sync that reconciles after a partition heals, erasure
> coding over lossy links, and a *cryptographic proof of delivery* you can verify
> offline. It already bridges to Meshtastic and Nostr, so it interops rather than
> competes.
>
> Honest status: the decentralized core runs today (over a relay/LAN), but the
> native BLE / Wi-Fi Aware radios are the part that isn't shipped yet — the
> platform-independent BLE segmentation/reassembly stack is done and a desktop
> btleplug backend is wired, but on-device CoreBluetooth/Android is in progress.
> If radio-layer/embedded folks want to help push that over the line, that's the
> #1 thing I need. Not security-audited yet either — don't rely on it for real
> emergencies.
>
> Repo + a 60-second `./quickstart.sh` in the README. What would make this
> genuinely useful alongside a Meshtastic setup?

## r/privacy  (angle: no accounts, no metadata honeypot, sealed sender)

**Title:** `Lifeline: mesh messenger with no accounts and no server that holds who-talked-to-whom (open source, alpha)`

**Body:** Emphasize: self-certifying identity (address = hash of your public
key), sealed sender, no phone number, no central directory to subpoena, source
reputation that's immune to Sybil badmouthing. Be explicit it's alpha and
unaudited; this sub is (rightly) skeptical of unaudited "secure" messengers —
frame it as "here's the threat model and what's NOT done," link SECURITY.md.

## r/amateurradio  (angle: DTN, LoRa/gateway bridging, off-grid)

**Title:** `Open-source DTN messenger with a gateway-bridging design (one uplink drains the whole mesh) — LoRa backend wanted`

**Body:** Lead with the DTN/gateway-gradient design and the "one connection
lights the mesh" property; note LoRa is a designed-but-unbuilt bearer behind a
finished Interface seam and you'd love a hand.

## Other channels (#112), in priority order
- **Lobsters** (needs an invite; `distributed`/`privacy`/`security` tags) — same honest frame as Show HN.
- **awesome-list PRs** (#113): awesome-decentralized, awesome-p2p, awesome-selfhosted, awesome-dtn if one exists.
- **Mesh/DTN Discords + Matrix rooms**: Meshtastic, Reticulum/NomadNet, Briar community.
- **Hacker communities**: the `decentralized`, `dtn`, `off-grid` tags on Mastodon/fedi.
- **Fediverse thread** from the project account (#116) mirroring the Show HN comment.

---

## Guardrails
- Never post to disaster-prep / emergency / prepper consumer communities yet —
  the offline radios aren't shipped and it isn't audited. That audience comes at
  Stage 2, after #64 and #65.
- Every post links to the honest status; never claim "works with no internet"
  without the "designed to; here's what runs today" qualifier.
- One post per community, spaced out over days, each genuinely tailored.
