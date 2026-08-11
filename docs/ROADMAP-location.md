# Roadmap — location, "find each other," and consistent messaging

The [find-each-other feature](../crates/node/web/index.html) and
[USE-CASES.md](USE-CASES.md) make a promise: Lifeline keeps you connected — and
findable — when the network can't. This is the checklist to make that promise
**fully real** (production-grade, not just demo-real) and to keep the
**documentation and messaging consistent across every surface**: the mesh node +
web app, the mobile app, the marketing website, the hosted SaaS, and the white
papers.

`[x]` = shipped · `[ ]` = to do. Rough priority is at the bottom.

---

## A. Make it real (engineering)

The biggest honesty gap today: the "phone-to-phone, works with no internet at
all" promise the use cases lean on is **designed but not shipped** — nodes
currently mesh over a relay/LAN that stands in for the radios. Closing this is
what turns the use cases from aspiration into fact.

- [x] Core find-each-other: peer-position tracking, distance + compass bearing,
      share-with-everyone, and the Nearby view (web). *(PR #62)*
- [~] **Native BLE transport** (gap G6) — the single load-bearing item for
      *every* offline use case. **Done + tested:** the platform-independent stack
      — ATT-MTU segmentation/reassembly, the `BleDriver` engine↔radio bridge, and
      the `GattPort` seam — verified end-to-end over an in-memory GATT fabric
      ([`crates/transport/src/ble.rs`](../crates/transport/src/ble.rs),
      [design](ble-transport.md)). **Remaining (hardware-bound):** a real
      `GattPort` — `btleplug` on desktop (central), CoreBluetooth/Android in the
      mobile app (dual-role) — plus MTU negotiation and duty-cycling, verified
      on-device.
- [ ] **Wi-Fi Aware** transport for higher-bandwidth phone-to-phone links.
- [x] **Mobile app: the Nearby / find-each-other view** (Expo) — a Nearby tab
      showing contacts nearest-first with distance + compass bearing, plus an
      opt-in "Share my location" control ([`mobile/screens/NearbyScreen.tsx`](../mobile/screens/NearbyScreen.tsx)).
- [~] Mobile **location permissions** — foreground / when-in-use is wired
      (`expo-location`, [`mobile/lib/location.ts`](../mobile/lib/location.ts)), and
      SOS/geocast now attach a real GPS fix. **Remaining:** background permission +
      a battery-aware update cadence (pairs with live sharing below).
- [ ] **Live / continuous location sharing** ("share for 15 min") with periodic
      updates and auto-expiry — today it's a single snapshot.
- [ ] **Differential / relative positioning** using `lifeline-timesync` +
      `lifeline-geo` for crowd-grade accuracy (the "differential GPS" idea).
- [ ] **Geohash location channels** (join-by-place) so strangers at the same
      event can coordinate without being contacts (bitchat-style).
- [ ] Nearby **staleness policy**: expire/dim old fixes (we currently keep all),
      and surface **accuracy** (`acc_m`, already captured) in the UI.

## B. Location privacy & safety (make it trustworthy)

Location is sensitive. Sharing it must be **consensual, scoped, and revocable** —
otherwise the safety feature becomes a tracking risk.

- [x] Explicit **opt-in per share** and a prominent **"stop sharing"** control
      (mobile Nearby screen; sharing is off until you tap Share).
- [x] **Per-group / per-contact scopes** — "share with this group only"
      (`LocationGroup` command + `/api/location_group`; scope chips in the app).
- [x] **Auto-expiry (TTL)** on shared positions; no indefinite tracking
      (`LIFELINE_LOCATION_TTL_SECS`, default 30 min; tested).
- [x] Document the **location threat model** in [SECURITY.md](../SECURITY.md):
      who can see a position, and the rendezvous-addressing caveat for it.
- [x] Verify **panic wipe** also destroys cached peer positions (the panic branch
      now clears `peer_pos`, POIs, and our own fix explicitly).

## C. Testing

- [x] `lifeline-geo` bearing/compass unit tests. *(PR #62)*
- [ ] Node **integration test** for `build_nearby` / peer-position tracking
      (two-engine round-trip) — currently only manually verified.
- [ ] Mobile **e2e** for the Nearby screen.

## D. Documentation consistency (across the board)

- [x] `USE-CASES.md` (7 categories), linked from README + WHITEPAPER. *(PR #62)*
- [ ] `STATUS.md`: add find-each-other / location-broadcast to the `FR-*`
      traceability (extends FR-43).
- [ ] `ARCHITECTURE.md`: document the peer-position → Nearby data path and the
      `LocationAll` command.
- [ ] `docs/geo-and-differential.md`: extend with the find-each-other +
      differential-positioning design.
- [ ] README **feature list**: mention "find each other."
- [ ] Each app README (`mobile/`, `saas/`, `crates/node/`) references the **same
      feature set** and the **same honest status**.

## E. Messaging consistency (across the board)

The same story, told the same way, **everywhere**: web app, mobile, marketing
site, SaaS, white papers.

- [x] Marketing "Where Lifeline helps" use-cases section (SaaS + static site).
      *(PR #62)*
- [ ] Add a **"Find each other" card** to the *Features* list on both marketing
      sites (Features currently omits it).
- [ ] Mobile app mirrors the find-each-other + use-cases framing.
- [ ] Keep the **alpha / native-radio-not-shipped** caveat *identical* on every
      surface (README, white papers, marketing, in-app) — audit for drift.
- [ ] Consistent **"sharing your location can save your life"** framing + the
      four headline scenarios on every surface.
- [ ] One **canonical feature list** (single source of truth) that every surface
      references, so messaging can't drift again.

---

## Priority order

1. **Native BLE (A)** — without it, the offline promise isn't literally true.
2. **Mobile Nearby parity + location-privacy controls (A/B)** — phones are where
   users are, and sharing must be safe before it's promoted.
3. **Live sharing + geohash channels (A)** — completes the crowd use case.
4. **Docs/messaging consistency sweep (D/E)** — cheap, and it's what makes the
   project trustworthy to a newcomer.

See also [GAPS.md](../GAPS.md) for the broader design-gap backlog and
[docs/competitive-gap-analysis.md](competitive-gap-analysis.md) for how these map
to what bitchat / Nostr / Buzz do.
