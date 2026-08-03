# Bluetooth LE transport — design & status

BLE is the bearer that makes Lifeline's headline promise literally true: two
phones that have never touched a tower exchange messages directly. This document
describes how the BLE bearer is built, what is implemented and tested today, and
what a real radio backend must provide.

## Where it sits

Lifeline's transport layer is a set of `Box<dyn Interface>` bearers the engine
drives identically ([interface.rs](../crates/transport/src/interface.rs)). BLE is
split into two halves so the reusable logic can be written and **tested without a
radio**:

```
   engine  ──frames(≤244B)──►  ChannelInterface(ble caps)   ← engine-facing, already exists
                                      │  Outbound / inbound channels
                                      ▼
                                  BleDriver          ← this module: ATT segmentation + bridge
                                      │  GattPort (seam)
                                      ▼
                          btleplug / CoreBluetooth / Android BLE   ← radio backend (hardware)
```

- **Engine-facing side** reuses [`ChannelInterface`](../crates/transport/src/channel.rs)
  with [`InterfaceCaps::ble()`](../crates/transport/src/caps.rs) (MTU 244). The
  engine already fragments every logical unit (beacon, bundle, CRDT blob) to that
  MTU; it is unaware a radio is underneath.
- **Radio-facing side** is [`crates/transport/src/ble.rs`](../crates/transport/src/ble.rs):
  a [`BleDriver`] that pumps frames between those channels and a [`GattPort`],
  doing the one thing BLE specifically requires — **ATT-MTU segmentation**.

## GATT profile

| | |
|---|---|
| Service UUID | `6c69666e-11e5-11ee-be56-0242ac120002` (custom; advertised so peers discover each other) |
| Frame characteristic | `6c69666e-11e5-11ee-be56-0242ac120003` (write-without-response **and** notify) |

Every node runs **both roles at once**: a **peripheral** advertising the service
and exposing the characteristic, and a **central** that scans for the service,
connects, subscribes to notifications, and writes segments. Symmetric dual-role
operation is what lets an arbitrary pair of phones connect — neither is "the
server."

## ATT segmentation (the BLE-specific framing)

A GATT write carries only `ATT_MTU − 3` bytes — as little as **20** on an
unnegotiated link, up to ~**512** after MTU negotiation. A 244-byte engine frame
therefore may not fit one write. `ble::segment` splits a frame into ordered
segments, each `[flag] ++ chunk`, where the final segment sets bit 0. Because a
GATT characteristic delivers writes/notifications **reliably and in order**, a
single "final" bit is sufficient — no sequence numbers. `SegmentReassembler`
rebuilds frames per peer and is **hard-bounded** (`MAX_REASSEMBLY`, 64 KiB/peer):
a peer that streams non-final segments forever has its buffer abandoned at the
ceiling rather than growing without bound. (Unbounded reassembly is exactly the
bug that broke Bridgefy — USENIX 2022 — so the bound is load-bearing.)

## The `GattPort` seam

A radio backend implements four methods; everything above is platform-independent:

```rust
pub trait GattPort {
    fn connected_peers(&self) -> Vec<PeerId>;      // currently-connected BLE peers
    fn att_payload(&self, peer: PeerId) -> usize;  // usable bytes/write (ATT_MTU-3)
    fn write(&mut self, peer: PeerId, bytes: &[u8]) -> Result<()>;
    fn drain(&mut self) -> Vec<(PeerId, Vec<u8>)>; // inbound segments
}
```

`PeerId` is a stable `u64` the backend derives from a peer's BLE address /
identifier. The [`BleDriver::pump`] loop publishes the connected-peer set to the
engine's `scan()`, segments outbound frames to each peer's ATT MTU, and
reassembles inbound segments into whole frames the engine polls.

## Discovery, connection lifecycle & power

The radio backend owns these (they are inherently platform APIs); the
recommended policy:

- **Discovery** — advertise the service UUID; scan on a duty cycle. On seeing a
  Lifeline peer, connect (bounded concurrent connections; prefer stronger RSSI).
- **Beacons** — once connected, the engine's own presence beacon (a `FrameKind::Beacon`)
  rides the same characteristic; no BLE-specific identity is exposed beyond the
  service UUID.
- **Duty cycling** — scan/advertise in bursts, and back off with battery level and
  connection count, to bound power draw. `BleDriver::pump` returns whether it did
  work so the radio thread can sleep when idle.
- **Contention** — broadcast storms are already suppressed above the bearer by the
  router (seeded fan-out + jitter; see the NFR-6 "dense 60-node cluster, no storm"
  simulation), so the BLE backend does not re-implement flooding control; it just
  moves frames.

## Platform support matrix

Dual-role BLE (advertise **and** scan/connect with a custom GATT server) is not
uniformly available. This determines which backend can serve which host:

| Platform | Central (scan/connect) | Peripheral (advertise + GATT server) | Backend |
|---|---|---|---|
| Linux (BlueZ) | ✅ | ✅ | `bluer`, or `btleplug` (central) |
| Android | ✅ | ✅ | Android BLE APIs (mobile app) |
| iOS | ✅ | ✅ | CoreBluetooth (mobile app) |
| macOS (desktop) | ✅ (`btleplug`) | ⚠️ limited | `btleplug` central; peripheral needs native CoreBluetooth |
| Windows | ✅ (`btleplug`) | ⚠️ limited | `btleplug` central |

**Consequence:** the phones — where users actually are — support full dual-role
BLE, so the **mobile app is the primary home** for the radio backend. Desktop
nodes can participate as centrals (and as gateways over other bearers).

## Status — what's done vs. what needs hardware

**Implemented and unit-tested (this repo, no hardware):**
- ATT segmentation + bounded reassembly (`segment`, `SegmentReassembler`).
- The `BleDriver` engine↔radio bridge, verified end-to-end over an in-memory
  two-node GATT fabric (a 1000-byte frame delivered across a 24-byte ATT MTU).
- The `GattPort` seam and the `LIFELINE_BLE` flag recognized by the node.

**Not yet (inherently hardware-bound, cannot be verified in CI):**
- A real `GattPort`: `btleplug` (desktop central) and CoreBluetooth / Android
  (mobile dual-role). These need a Bluetooth-capable host and a second device to
  test, so they land with on-device verification, not in the sandbox.
- MTU negotiation, connection management, and duty-cycling tuning on real radios.

See [ROADMAP-location.md](ROADMAP-location.md) §A for how this fits the broader
plan, and [ARCHITECTURE.md](../ARCHITECTURE.md) for the transport layer overall.
