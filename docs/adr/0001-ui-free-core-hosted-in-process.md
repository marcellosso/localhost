# UI-free core package, hosted in-process for V1

All project state, process lifecycle, logs and environment handling live in a local Swift package (`LocalhostCore`) that never imports SwiftUI or AppKit; the app target (main window + menu bar) is only a client of it. In V1 the core runs inside the app process, so quitting the app stops every Service. We chose this over a background daemon from day one because V2's CLI/MCP needs don't exist yet, but the core's API is async, event-based, takes an Origin on every state change, and returns typed errors so it can later be hosted by a daemon and exposed over IPC/MCP without a rewrite.

Update (cross-platform): the core moves to Rust and is shared by every client (ADR-0007). The rest of this decision stands: no UI in the core, in-process in the apps, an API shaped for a later daemon.
