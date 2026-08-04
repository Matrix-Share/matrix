# Fuzzing

Coverage-guided fuzz targets for Lifeline's **parsers of untrusted bytes** — the
framing layer where mesh messengers have historically been broken (Bridgefy,
USENIX 2022). Part of the [SSDLC](../docs/SSDLC.md).

## Targets

| Target | Fuzzes |
|---|---|
| `frame_decode` | `lifeline_transport::frame::Frame::decode` + `Reassembler` (wire fragments; re-encode round-trip is asserted) |
| `bundle_cbor` | CBOR decode of a `Bundle` (the top-level object received from any peer) |
| `payload_cbor` | CBOR decode of a `Payload` (the inner decrypted message body) |
| `ble_reassemble` | `ble::SegmentReassembler` (BLE ATT-segment reassembly stays bounded) |

## Run it

Requires a nightly toolchain and [`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz)
(libFuzzer):

```bash
rustup toolchain install nightly
cargo install cargo-fuzz

cargo +nightly fuzz list
cargo +nightly fuzz build                       # build all targets
cargo +nightly fuzz run frame_decode            # fuzz one (Ctrl-C to stop)
cargo +nightly fuzz run bundle_cbor -- -max_total_time=60
```

A crash writes a reproducer to `fuzz/artifacts/<target>/`; re-run the target with
that file as an argument to reproduce, then add it as a regression test.

CI builds every target on relevant PRs and runs each nightly
([`.github/workflows/fuzz.yml`](../.github/workflows/fuzz.yml)).
