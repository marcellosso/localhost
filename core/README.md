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

## The `localhost` command

`localhost-cli` builds a `localhost` binary on the same core and the same
storage as the app:

```sh
cargo run -p localhost-cli -- projects           # every Project
cargo run -p localhost-cli -- status acme         # which Services are running
cargo run -p localhost-cli -- logs acme [web]     # the latest Run's output
cargo run -p localhost-cli -- up acme [web …]     # run in the foreground; Ctrl-C stops everything
```

Projects and Services are named by Key, name, or letters of either in order
(`FuzzyMatch`).

`localhost-core/tests/fixtures/swift-schema-1/` holds files written by the
Swift core; the Rust core must keep reading them.
