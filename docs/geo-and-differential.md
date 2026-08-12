# Geo-encoding & Differential-GPS ideas for Lifeline

Two GPS-adjacent directions, researched and grounded in Lifeline's architecture.
The one premise to get right first: **GPS is strictly receive-only** — a receiver
derives *position* and *time* from satellite timing and can never transmit back.
So "sending through GPS" is impossible. But GPS is a free, global, offline
**sensor** that hands every device two gifts — precise **position** and precise
**time** — and those are inputs that unlock a lot. The correct model (and exactly
how a Lifeline node already works): **use GPS as a sensor feeding the mesh; let
the other radios do the sending.**

Two real footnotes to the premise:
- **Receive-only data channels adjacent to GNSS exist.** SBAS/WAAS uplinks
  correction data to geostationary satellites that every receiver decodes — proof
  that a one-to-many "read-only broadcast riding GNSS" is real (you can't inject
  your *own* data, but the pattern is exactly an alert downlink).
- **The genuine "send the other way" already shipped** — as a *separate satellite
  radio*, not GPS. Direct-to-device satellite messaging (3GPP NTN Release 17) is
  in the iPhone 14+, Galaxy S25, Pixel 9; the phone "treats the satellite as
  another cell tower."

---

## Angle 1 — Location as an information mechanism

| # | Mechanism | Lifeline seam it fits | Verdict |
|---|-----------|------------------------|---------|
| A | **Geographic routing + geocast** — forward toward a position; deliver to "everyone in region R" | the `router::RoutingPolicy` trait + a geocast destination | **Highest value, buildable now** |
| B | **Location as content** — geo-tagged SOS/hazard/resource reports aggregated into a shared map | `sync` CRDT | Straightforward |
| C | **Time as a hidden channel** — GPS time → TDMA slotting, duty-cycle rendezvous, TESLA broadcast auth | beacon/engine scheduling | High value (see Angle 2.1) |
| D | **SBAS-style alert downlink** — wide-area authenticated one-to-many alert | "cell-broadcast ingest" backlog item | Reference/strategic |
| E | **Satellite-NTN bearer** — the ultimate offline uplink | `ExternalNet`/gateway | Strategic, API-gated today |

**A is the disaster-native idea.** "SOS to anyone within 2 km," "resource drop
here to whoever's near" — addressing a *region* instead of an identity. BitChat
already proves geohash-as-address works off-grid (the geohash string *is* the
channel id, hierarchical, per-cell pseudonyms). Payloads already carry `Coords`.
The DTN caveat: a destination's position goes stale, so blend geo with
store-carry-forward rather than pure greedy forwarding.

---

## Angle 2 — Differential GPS, transferred

DGPS/RTK works on one principle: **when an error is common-mode and
spatially/temporally correlated across receivers, a reference at known ground
truth measures it once and broadcasts a correction everyone nearby applies to
cancel it.** Three ingredients: a reference with ground truth, a correlated
error, a broadcast channel. Where does Lifeline have that pattern?

1. **Differential *time* (flagship).** A node with GPS lock (or a fixed
   anchor/gateway) is the "reference station." It broadcasts precise time; nodes
   *without* sky view (indoors/underground/urban canyon — the disaster case)
   discipline their cheap oscillators to it. Ground truth = GPS time; correlated
   error = oscillator drift; broadcast channel = the mesh. This turns a handful
   of sky-view nodes into a **timing backbone** for a whole GPS-denied cluster,
   unlocking all of Angle 1.C (TDMA slotting on ultrasound/LoRa, coordinated
   duty-cycling for battery, TESLA-style cheap broadcast authentication).
   *Rigorous, buildable, high value.*
2. **Differential *positioning* (cooperative localization).** GPS-equipped nodes
   are anchors; GPS-denied nodes estimate position by ranging to them (RF, or
   *acoustic ranging over the ultrasound bearer*). Feeds geo-routing (Angle 1.A).
   Real field, but honest caveat: phone RF ranging is coarse without UWB.
3. **Differential *reputation* (the novel transfer).** The DGPS insight
   generalizes: a trusted reference cancels correlated error in a noisy shared
   estimate. Lifeline's black-hole **reputation gossip** (FR-47) is exactly that —
   noisy, partial-view, with the audited weakness that a malicious node can
   **defame an honest one**. A trusted **anchor** broadcasting periodic
   "reputation corrections" is a *differential-reputation reference* that damps
   defamation — and the Network-RTK/Virtual-Reference-Station idea (interpolate
   from multiple base stations weighted by location) maps to weighting several
   anchors by proximity/trust. An analogy, not literal DGPS, but apt and
   prototype-worthy; it hardens a mechanism Lifeline already ships.

---

## Build plan

Ranked, each landing on a seam that already exists:

1. **`lifeline-geo` primitive (this PR).** Geohash encode/decode + region
   containment (geocast membership) + neighbor/radius coverage + haversine
   distance. Pure, self-contained, fully tested — the foundation for geo-routing,
   geocast, and the "who's near a location" query. (Mirrors how `lifeline-dht`
   landed: clean library first, wiring next.)
2. **Geocast wiring — core done.** ✅ `core::geocast` derives a deterministic
   keypair from a region's geohash (BitChat-style), so the sender seals to the
   region with the ordinary `seal_bundle` and any node *in* the region opens it —
   no wire-format change (the region is encoded in the bundle's `dst`, matched by
   the receiver encoding its own position at a fixed precision). The engine gains
   `set_position` + `broadcast_geo(lat, lon, radius, …)` and position-gated
   delivery; a geocast has no single recipient so it keeps spreading. Verified
   end-to-end: a node inside the region receives it, one outside doesn't, none are
   contacts. *Node/UI hookup done:* browser + mobile geolocation → position, an
   "alert an area" send form, and **geohash place channels** — `join_region` /
   `leave_region` / `post_to_region` let a node open geocasts for a *joined* cell
   regardless of its own position, so strangers at the same place coordinate
   without being contacts (messages thread under `place:<geohash>`; end-to-end
   test). *Remaining:* a `GeoRoutingPolicy` that forwards *toward* the region
   (today geocasts spray broadly).
   - **Find each other / Nearby — shipped.** ✅ Peer positions shared via
     `Location` / `LocationAll` / `LocationGroup` feed `build_nearby` (distance +
     compass bearing from `lifeline-geo`); the app renders a nearest-first list and
     an opt-in **live** share (scope + duration + auto-stop). Positions
     **auto-expire** (`LIFELINE_LOCATION_TTL_SECS`) so a share is never indefinite
     tracking, and the panic wipe clears them. This is "Angle 1 — location as an
     information mechanism" made concrete.
3. **Differential time sync — core done.** ✅ `lifeline-timesync`: a GPS node is a
   stratum-1 **reference** that broadcasts its time; GPS-denied nodes discipline
   their oscillators to it (stratum 2+) and re-broadcast, so corrections propagate
   hop-by-hop with quality degrading by stratum — *exactly* like DGPS accuracy
   degrading with baseline. Offset uses a median window so a single high-latency
   beacon can't skew the clock. Verified: a two-hop chain (GPS → A → B → C) lands
   C on network time; a closer reference wins; outliers are rejected; stale fixes
   stop advertising. *Remaining:* wire `advertise()` into the engine beacon +
   feed received beacons to `observe`, then use the shared clock for TDMA slotting
   / duty-cycle rendezvous / TESLA broadcast auth.
4. **Differential-reputation anchors** and **satellite-NTN bearer** as follow-ups.

---

## The differential pattern, applied elsewhere

DGPS is one instance of a reusable pattern: **a reference with ground truth
cancels correlated error in a noisy shared estimate, by broadcasting a correction
everyone applies.** Three ingredients — reference, correlated error, broadcast
channel — and Lifeline has several places that fit:

| Application | The noisy shared estimate | The reference (ground truth) | The correlated error it cancels | Fit / status |
|---|---|---|---|---|
| **Time** ✅ | each node's oscillator | a GPS node's time | drift, shared across cheap clocks | built (`lifeline-timesync`) |
| **Reputation** | black-hole reputation gossip (FR-47) | a known-good community **anchor** | partial views + adversarial defamation (a malicious node smearing an honest one — the audited weakness) | strong; hardens an existing mechanism; buildable |
| **Positioning** | a GPS-denied node's position | GPS-equipped **anchor** nodes + ranging | shared GNSS-denied conditions | real (cooperative localization); feeds geo-routing; ranging-accuracy caveat |
| **Congestion / admission** | each node's local view of mesh load | a **gateway** with a broad view | everyone underestimates global load from a local view | good; tunes PoW postage (FR-46) + spray rate; buildable |
| **Environmental sensing** | crowd-sourced sensor readings (barometric altitude, air quality, radiation) a disaster mesh might carry | a node at **known** conditions | a common bias across nearby sensors — *literally* how SBAS corrects ionospheric delay and aviation corrects barometric altitude (QNH) | the most direct SBAS transfer; novel for a messenger; speculative-but-apt |
| **Channel / link quality** | each node's bearer-quality estimate | a reference broadcasting observed RF state | shared interference / noise floor | moderate; improves adaptive-bandwidth bearer selection |

(The **gateway gradient** is *already* an instance of this pattern — a reference,
the gateway, broadcasts and nodes form a corrected distance estimate — so it
validates the approach rather than being a new application.)

The two worth building next are **differential reputation** (small, and it fixes
the exact defamation weakness the security audit flagged) and, once positions
flow, **differential/cooperative positioning** (which unlocks geographic
routing). Congestion and environmental-sensing corrections are strong
longer-horizon ideas.

## Sources

SBAS: NovAtel, ESA Navipedia · DTN geo-routing: Wang et al. 2016 survey ·
Geohash channels: BitChat · Time-sync/TDMA: WSN survey (RPI) · DGPS/RTK: Point
One Nav · Cooperative localization: MDPI Electronics 2025, Cambridge J. of
Navigation · TESLA/μTESLA/OSNMA · Satellite direct-to-device: Ericsson, 3GPP NTN
Release 17.
