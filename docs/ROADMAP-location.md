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
      [design](ble-transport.md)). **Desktop backend done:** a real `GattPort` over
      `btleplug` (central role) — scan → connect → subscribe → notify/write — wired
      behind the `ble-radio` feature and `LIFELINE_BLE`
      ([`crates/node/src/ble_backend.rs`](../crates/node/src/ble_backend.rs)); build
      with `cargo build -p lifeline-node --features ble-radio`. **Remaining
      (hardware-bound):** on-device verification, ATT-MTU negotiation (we currently
      advertise the safe 20-byte minimum), duty-cycling, and the CoreBluetooth /
      Android dual-role backend in the mobile app.
- [ ] **Wi-Fi Aware** transport for higher-bandwidth phone-to-phone links.
- [x] **Mobile app: the Nearby / find-each-other view** (Expo) — a Nearby tab
      showing contacts nearest-first with distance + compass bearing, plus an
      opt-in "Share my location" control ([`mobile/screens/NearbyScreen.tsx`](../mobile/screens/NearbyScreen.tsx)).
- [~] Mobile **location permissions** — foreground / when-in-use is wired
      (`expo-location`, [`mobile/lib/location.ts`](../mobile/lib/location.ts)), and
      SOS/geocast now attach a real GPS fix. **Remaining:** background permission +
      a battery-aware update cadence (pairs with live sharing below).
- [x] **Live / continuous location sharing** ("share for 15 min / 1 hour") — the
      mobile Nearby screen now pushes a fresh fix on an interval until a chosen
      deadline, with a live countdown and auto-stop; positions still TTL-expire.
- [ ] **Differential / relative positioning** using `lifeline-timesync` +
      `lifeline-geo` for crowd-grade accuracy (the "differential GPS" idea).
- [x] **Geohash location channels** (join-by-place) so strangers at the same
      event can coordinate without being contacts — `join_region`/`leave_region`/
      `post_to_region` in the engine (built on the geocast region key), node
      commands + `/api/place/*`, messages threaded under `place:<geohash>`, and an
      end-to-end test. *(App channel screen is the remaining UI follow-up; the
      client actions are wired.)*
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
- [x] `STATUS.md`: find-each-other / live / privacy / place-channels added to the
      `FR-*` traceability (FR-43a–d), plus the desktop BLE backend row.
- [x] `ARCHITECTURE.md`: §3.6 documents the peer-position → Nearby path, geocast
      addressing, and place channels.
- [x] `docs/geo-and-differential.md`: extended with the shipped find-each-other /
      Nearby + place-channel design (build plan §2).
- [x] README **feature list**: mentions "find each other" + an honest alpha caveat
      on the offline radios.
- [~] Each app README references the **same feature set** and **honest status** —
      root README + `mobile/README` (Nearby) done; `saas/`/`crates/node/` READMEs
      still to sweep.

## E. Messaging consistency (across the board)

The same story, told the same way, **everywhere**: web app, mobile, marketing
site, SaaS, white papers.

- [x] Marketing "Where Lifeline helps" use-cases section (SaaS + static site).
      *(PR #62)*
- [x] Add a **"Find each other" card** to the *Features* list on both marketing
      sites (static site already had one; added to the SaaS `FEATURES`).
- [x] Mobile app mirrors the find-each-other framing — the **Nearby screen** ships
      (share / scope / live / countdown), and the mobile README lists it.
- [~] Keep the **alpha / native-radio-not-shipped** caveat *identical* on every
      surface — root README now carries the honest BLE status; marketing + white
      papers + in-app still to audit for drift.
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
