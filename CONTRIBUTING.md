# Contributing to Project Lifeline

Thanks for helping build resilient, offline-first emergency communication.
Lifeline is safety-critical software for people in disasters, so we hold a high
bar for correctness, security, and honesty about limitations.

## Ground rules

- **No custom crypto primitives.** Compose audited libraries only (see
  `crates/core`). Protocol/composition code is fine; new ciphers/hashes are not.
- **Harden the framing layer, not just the crypto.** Every wire parser must be
  strict and bounded (the Bridgefy lesson — see `GAPS.md`). Add fuzz targets for
  anything that parses untrusted bytes.
- **Offline-first.** No feature may hard-depend on a server. The relay is
  optional and zero-knowledge.
- **Be honest about limits.** A permanently isolated recipient cannot receive;
  we prove *whether* delivery happened, we don't defy physics.

## Development

```bash
# Prereqs: Rust stable (see rust-toolchain.toml)
cargo test                       # all crates
cargo run -p lifeline-sim --release   # acceptance simulator
cargo fmt --all                  # format
cargo clippy --all-targets -- -D warnings
```

Run the app locally without Docker:

```bash
cargo run -p lifeline-relay &                      # hub on :7000
LIFELINE_NODE_ADDR=127.0.0.1:8080 LIFELINE_NAME=Asha cargo run -p lifeline-node &
LIFELINE_NODE_ADDR=127.0.0.1:8081 LIFELINE_NAME=Ravi cargo run -p lifeline-node &
# open http://127.0.0.1:8080 and http://127.0.0.1:8081
```

## Pull requests

1. Branch from `main`; keep PRs focused.
2. Add or update tests. New acceptance criteria go in `crates/sim` or a crate's
   integration tests.
3. `cargo fmt`, `cargo clippy -D warnings`, and `cargo test` must pass (CI
   enforces this).
4. Update `STATUS.md` / `GAPS.md` if you change what a PRD requirement's state is.
5. Describe the threat-model impact of anything touching crypto, framing, or the
   relay.

## Architecture map

New transports implement `transport::Interface`. New E2E schemes implement
`core::crypto::SecureChannel`. New CRDTs live in `sync`. See `INTEROP.md` for how
external OSS projects map onto these seams.

## Reporting security issues

Do **not** open a public issue. See [`SECURITY.md`](SECURITY.md).
