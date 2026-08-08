# Lifeline roadmap

A public, honest view of where the project is and what's next. This is a living
document — the source of truth is the [issue tracker](https://github.com/matrix-share/matrix/issues);
this page groups the big rocks so newcomers can orient fast.

> **Status: alpha.** The decentralized core is implemented and tested; the native
> phone-to-phone radio bearers and a one-tap installable app are the main things
> between "alpha" and "anyone can use it." See
> [`docs/RELEASE-READINESS.md`](docs/RELEASE-READINESS.md) for the honest cross-check.

## Now — making the offline promise real
The headline claim is "works phone-to-phone with no internet." Closing that gap
is the top priority.

- **Native BLE radio backend** — btleplug on desktop, CoreBluetooth/Android on mobile ([#64](https://github.com/matrix-share/matrix/issues/64))
- **Mobile find-each-other + location permissions** ([#67](https://github.com/matrix-share/matrix/issues/67))
- **Location privacy controls** — opt-in, per-group scope, auto-expiry ([#68](https://github.com/matrix-share/matrix/issues/68))
- **Frictionless install** — prebuilt binaries, `cargo install`, F-Droid, mobile beta ([#117](https://github.com/matrix-share/matrix/issues/117))

## Next — trust & hardening (pre-1.0 blockers)
- **Third-party security audit** ([#65](https://github.com/matrix-share/matrix/issues/65))
- **MLS / TreeKEM group encryption** for post-compromise security ([#66](https://github.com/matrix-share/matrix/issues/66))
- **Geohash location channels** — join-by-place ([#69](https://github.com/matrix-share/matrix/issues/69))

## Growth — being findable and adoptable
Tracked under [`area:growth`](https://github.com/matrix-share/matrix/labels/area%3Agrowth).

- **Record the demo** — the airplane-mode phone-to-phone proof ([#109](https://github.com/matrix-share/matrix/issues/109))
- **Launch** — Show HN ([#110](https://github.com/matrix-share/matrix/issues/110)), Reddit ([#111](https://github.com/matrix-share/matrix/issues/111)), other channels ([#112](https://github.com/matrix-share/matrix/issues/112))
- **Compounding reach** — awesome-lists ([#113](https://github.com/matrix-share/matrix/issues/113)), the theory blog post ([#114](https://github.com/matrix-share/matrix/issues/114)), arXiv preprint ([#115](https://github.com/matrix-share/matrix/issues/115))
- **Presence** — social accounts ([#116](https://github.com/matrix-share/matrix/issues/116)), repo social-preview + pin ([#119](https://github.com/matrix-share/matrix/issues/119))

## Polish & onboarding
- **Docs & messaging consistency sweep** ([#71](https://github.com/matrix-share/matrix/issues/71))
- **Bump-to-pair + QR crew join** ([#72](https://github.com/matrix-share/matrix/issues/72))
- **"Bars given"** — surface each phone's relaying contribution ([#73](https://github.com/matrix-share/matrix/issues/73))
- **Release cadence + changelog + "what's new" posts** ([#118](https://github.com/matrix-share/matrix/issues/118))

## Done (recent highlights)
The decentralized core, the runnable node + web app, UDP/LAN transport, CRDT sync,
reputation-based black-hole avoidance, erasure/fountain coding, geocast, "find
each other," POI wayfinding, the strobe beacon, the Situations hub, the SSDLC +
OpenSSF Scorecard, and the rebuilt marketing site + brand. See
[`CHANGELOG.md`](CHANGELOG.md) and [`STATUS.md`](STATUS.md) for the full record.

---
*Priorities can shift with contributor interest and real-world feedback. If
something here matters to you, comment on the issue or open a new one.*
