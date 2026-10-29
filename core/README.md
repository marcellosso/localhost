# localhost core (Rust)

The core every localhost client shares: Projects, Services, Runs, logs,
environment and Secrets. The macOS app, the Linux and Windows app, the
`localhost` command and `localhostd` all run on it. How it is organised and
the rules it keeps are in [AGENTS.md](../AGENTS.md#architecture).

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
cargo run -p localhost-cli -- mcp                 # an MCP server on stdin/stdout, for coding agents
cargo install --path localhost-cli                # put `localhost` in ~/.cargo/bin
```

Projects and Services are named by Key, name, or letters of either in order
(`FuzzyMatch`).

`localhost mcp` (`src/mcp/`) speaks JSON-RPC lines on stdin/stdout. It runs
Services in the shared core (below), the one the app drives, with
`Origin::Agent`. `up` runs its own `ServiceManager` in the foreground; the
Runs of such a session are read from the Run records, never started twice
or stopped.

## The shared core

On macOS and Linux, the apps and `localhost mcp` drive one core in the
background, so what one starts is Running in the others
(`localhost-core/src/remote/shared.rs`). It speaks the `localhostd` protocol
(below) on a Unix socket, `localhostd.sock` in the storage root, that only
the user may connect to. The first client that needs it starts it from its
own executable: `localhost daemon <storage root>`, or the app with
`--daemon <storage root>`. A lock file keeps it to one per storage root.

- A client says what it is (`Client`: app, agent, CLI). When one
  disconnects, the core stops the Runs nobody needs any more: none while an
  app is connected, else those no remaining client started (a Run's
  Dependencies count as started by whoever started it).
- It exits once no client has been connected for 10 s, stopping every Run.
- Its standard error goes to `localhostd.log` in the storage root.

```sh
cargo run -p localhost-cli -- daemon "$HOME/Library/Application Support/localhost"   # serve it in the foreground
```

## localhostd

`localhost-daemon` builds `localhostd`, the core driven over its standard
input and output. The Windows app runs it inside each WSL distro it uses.
The protocol is in `localhost-core/src/remote/`: JSON lines, a
`Hello` first, then requests with ids, replies, Service events and log
batches. `Remote` is the app's side of it.

- The app sends the Projects with their Secret values, which `localhostd`
  holds in memory only.
- Runs' logs and records go to `~/.local/share/localhost/localhostd/`.
- When its input closes, it stops every Run and exits: its only client is
  the app.

```sh
cargo run -p localhost-daemon -- --stdio    # speaks the protocol on stdin/stdout
```

## The macOS app's library

`localhost-ffi` is the static library the macOS app links. It is the core
built with its `ffi` feature, which adds UniFFI bindings next to the types
they describe (`localhost-core/src/ffi/`, `localhost-core/uniffi.toml`).
`scripts/build-core.sh` builds it and generates the Swift bindings into
`apps/macos/LocalhostCoreFFI`:

```sh
scripts/build-core.sh            # universal (arm64 + x86_64) release build, as shipped
scripts/build-core.sh debug      # this Mac's architecture only, faster
scripts/build-core.sh --check    # fails if the output is older than core/
```

`localhost-core/tests/fixtures/swift-schema-1/` holds files written by earlier
(Swift) versions of localhost; the core must keep reading them.
