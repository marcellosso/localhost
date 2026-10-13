# Cross-platform plan

localhost goes from a macOS app to macOS, Linux and Windows. There is real demand for both Linux and Windows, so `localhost-v1.md` §16 and `localhost-v2.md` §15 ("only with demand") no longer hold back this work. Where this file disagrees with `docs/v1-decisions.md`, **this file wins** for anything it covers. Vocabulary is in `GLOSSARY.md`; the hard-to-reverse choices are ADR-0007 to ADR-0009.

## Shape

```text
                 localhost-core (Rust)
   Projects · Runs · logs · environment · Listeners · detection
                          │
      ┌───────────────────┼────────────────────┐
      │                   │                    │
  SwiftUI app        Tauri app             CLI (later MCP)
  macOS, in-process  Windows + Linux,      same crates
  via UniFFI         links the core
```

- **One core, in Rust** (ADR-0007). Everything `Packages/LocalhostCore` does today moves to a Cargo workspace at `core/`. The Swift package is retired once the macOS app runs on the Rust core.
- **Two UIs** (ADR-0009): the existing SwiftUI app stays on macOS; one Tauri app (Rust shell, React UI) serves Windows and Linux. Both stay thin: anything that isn't drawing (state, validation, fuzzy matching, detection, the Project file) belongs in the core.
- **Execution Targets** (ADR-0008): a Project runs on an Execution Target, either this machine or a WSL distro. WSL Projects are run by the Linux build of the core inside the distro.

## What carries over unchanged

- The domain: `GLOSSARY.md`, the six Service States, Dependencies, Ports and `$PORT` (ADR-0006), Environment Profiles and Secrets, Suggestions, Orphans, the YAML Project file.
- The core's API shape (ADR-0001): async, observable snapshots plus an event stream, an Origin on every state change, typed errors, no UI imports.
- Configuration is stored by localhost, never in the project folder (ADR-0004). Output goes through pipes, not a PTY (ADR-0005).
- On macOS: the same storage folder, the same JSON schema, the same log layout, and the same Keychain service (`com.sixpetals.localhost.secret`). Updating to the Rust-core build must not lose a Project, a log or a Secret.

## Per platform

| Concern | macOS | Linux | Windows (native) |
|---|---|---|---|
| Storage | `~/Library/Application Support/localhost` | `$XDG_DATA_HOME/localhost` (`~/.local/share/localhost`) | `%APPDATA%\localhost` |
| Command | `$SHELL -l -c` (ADR-0003) | `$SHELL -l -c` | Decided in XP5 (pwsh / cmd, PATH from the registry) |
| Process tree | new session, signal the process group | same | Job Object (kill-on-close), `CTRL_BREAK_EVENT`, then terminate the job |
| Listeners | libproc | `/proc/net/tcp{,6}` + `/proc/<pid>/fd`, `cwd` | `GetExtendedTcpTable` |
| Secrets | Keychain | Secret Service (libsecret) | Credential Manager |
| Tray | menu bar extra | AppIndicator (GNOME needs the extension) | notification area |
| Distribution | DMG (`RELEASING.md`) | `.deb`, `.rpm`, AppImage. **Not Flatpak or Snap**: their sandboxes can't spawn host commands, the same reason as ADR-0002 | NSIS/MSI installer, Authenticode signing when a certificate exists |

Platform-specific code sits behind small traits in the core (process launching and stopping, Listener scanning, process info for Orphans, the secret store, storage paths, the login environment), with one implementation per OS. Everything else is shared and tested once.

## Order

Each step is one issue, and each ships something on its own:

1. **XP1: Rust core and CLI.** Reach parity with `LocalhostCore`, tested against real processes on macOS and Linux.
2. **XP2: macOS app on the Rust core.** Behaviour stays identical, and existing user data still loads.
3. **XP3: Linux app.** The Tauri app's first platform.
4. **XP4: Windows app with WSL Execution Targets.** Reaches Windows users whose projects live in WSL.
5. **XP5: Native Windows execution.** Projects on the Windows filesystem with Windows toolchains.

MCP and the shared daemon (`localhost-v2.md`) come after this plan, built on the same crates.

## Tauri UI notes

- The UI runs in the system webview: WKWebView, WebView2 on Windows, WebKitGTK on Linux. WebKitGTK is the slowest and quirkiest of the three, so performance work is measured on Linux first.
- Log output never crosses the bridge one line at a time. The core keeps the ring buffers; the UI receives batched appends at most once per frame and renders the log as a virtualized list. Snapshots are sent as diffs, not as the whole state on every event.
- The "Graded glass" direction (`docs/v1-decisions.md`, UI direction) carries over as tokens: colours, Service colours, State glyphs, monospace rules. Translucency is optional and off where the platform doesn't do it well. Accessibility targets match M3: screen-reader labels, full keyboard operation, AA contrast, reduced motion.
