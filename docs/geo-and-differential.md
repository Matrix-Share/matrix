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
2. **Geocast wiring.** A region destination on the bundle, a router delivery rule
   ("deliver if my position ∈ region"), and a `GeoRoutingPolicy` behind the
   `RoutingPolicy` trait that prefers geographically-closer peers. Needs peer
   positions in beacons.
3. **Differential time-sync beacon.** A GPS-anchor broadcasts disciplined time;
   GPS-denied nodes estimate offset and correct. Then TDMA slotting + duty-cycle
   rendezvous on the scarce bearers.
4. **Differential-reputation anchors** and **satellite-NTN bearer** as follow-ups.

## Sources

SBAS: NovAtel, ESA Navipedia · DTN geo-routing: Wang et al. 2016 survey ·
Geohash channels: BitChat · Time-sync/TDMA: WSN survey (RPI) · DGPS/RTK: Point
One Nav · Cooperative localization: MDPI Electronics 2025, Cambridge J. of
Navigation · TESLA/μTESLA/OSNMA · Satellite direct-to-device: Ericsson, 3GPP NTN
Release 17.
