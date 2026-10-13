# One Rust core shared by every client

localhost ships on macOS, Linux and Windows, and every client (the SwiftUI app, the Tauri app, the CLI, later MCP) uses one core written in Rust. Swift was considered: it is fine on Linux, but on Windows it has very few production users and a thin ecosystem, and Windows is where the hardest code lives (Job Objects, console control events, TCP tables, Credential Manager). Rust has first-class Windows bindings (`windows-rs`), mature cross-platform crates for the rest, and is also the language of the Tauri shell, so the core, the Windows/Linux app backend and the CLI share one language. The existing `LocalhostCore` and its real-process test suite serve as the specification for the rewrite.

## Consequences

- This replaces the "Swift package" part of ADR-0001. Its rules still apply: no UI in the core, an async event-based API, an Origin on every state change, typed errors.
- The macOS app calls the core in-process through UniFFI-generated Swift bindings, so quitting the app still stops every Service. The Tauri app links the crates directly.
- On macOS the storage folder, the JSON schema, log files and Keychain items stay exactly as they are, so updating loses nothing.
- Building the macOS app now needs a Rust toolchain (a universal static library built in CI).
