# localhost core (Rust)

The core every localhost client will share: Projects, Services, Runs, logs,
environment and Secrets (ADR-0007, `docs/cross-platform.md`). It is being
ported from the Swift package in `Packages/LocalhostCore`, which the macOS app
still uses until XP2 (#47). The Swift sources and tests are the specification.

```sh
cd core
cargo test                     # everything, against real processes and a temporary storage root
cargo test -- --ignored        # also the real Keychain / Secret Service (touches your login store)
cargo clippy --all-targets -- -D warnings
cargo fmt
```

`localhost-core/tests/fixtures/swift-schema-1/` holds files written by the
Swift core; the Rust core must keep reading them.
