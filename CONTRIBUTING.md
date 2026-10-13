# Contributing to localhost

Thanks for helping. This guide covers building, testing, how the code is organised and what a change needs to be merged.

For anything larger than a small fix, open an issue first so we can agree on the approach before you write it.

## Building

Requirements: macOS 15 or later, Xcode 26 (Swift 6.2), and Rust through [rustup](https://rustup.rs):

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin   # the app ships for both
```

```
core/                         # the UI-free core, in Rust: Projects, Runs, logs, persistence (+ the `localhost` CLI)
Packages/LocalhostCoreFFI     # the core as the Swift module `LocalhostCore`: generated bindings + a thin layer
localhost.xcodeproj           # the macOS app (SwiftUI); sources in App/
scripts/build-core.sh         # builds core/ for the app and generates the Swift bindings
```

### The app

Once after cloning, build the core so Xcode finds the package's library:

```sh
scripts/build-core.sh debug
```

Then open `localhost.xcodeproj`, select the `localhost` scheme and press ⌘R. The scheme rebuilds the core before every build (debug for Debug, universal for Release). Xcode doesn't show that step's output: if the build stops at "Check the Rust core", the reason is in `Packages/LocalhostCoreFFI/Generated/build.log`, or run `scripts/build-core.sh` in a terminal. Or from the command line:

```sh
xcodebuild -project localhost.xcodeproj -scheme localhost -configuration Debug -derivedDataPath build build
open build/Build/Products/Debug/localhost.app
```

Without a signing identity, add `CODE_SIGNING_ALLOWED=NO` (CI does). A build signed that way, or ad hoc, asks for Keychain access to Secrets after every rebuild; see "Keychain prompts" in the README.

To try a build without touching your real data in `~/Library/Application Support/localhost`, point it at another folder:

```sh
open -n --env LOCALHOST_STORAGE_ROOT=/tmp/localhost-dev build/Build/Products/Debug/localhost.app
```

Secrets still go to your login Keychain, under the service `com.sixpetals.localhost.secret`.

### The core

The core builds and tests on its own, without the Xcode project, on macOS and Linux (`core/README.md`):

```sh
cd core
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

The Swift layer has its own tests, which need the library built first:

```sh
scripts/build-core.sh debug
cd Packages/LocalhostCoreFFI && swift test
```

## Architecture

localhost is two parts: a core that does everything, and an app that shows it.

### The core: `core/`

All state, process management, logs and persistence live in the core, a Rust library shared by every client (ADR-0007): the macOS app hosts it in-process, the `localhost` command links it, and the Windows and Linux app will too (`docs/cross-platform.md`). It **never depends on a UI toolkit**. Later an MCP server for coding agents may drive the same core, possibly in a background process. That shapes its API:

- **async, on tokio**: long operations are `async`, and every public type is `Send + Sync` (and serializable where it is data);
- **snapshots plus events**: clients read the current state and subscribe to a stream of events, rather than reaching into mutable objects;
- **every state-changing call takes an `Origin`** (`.mainWindow`, `.menuBar`; later `.cli`, `.agent`), so a change can be traced to who asked for it;
- **failures are `LocalhostError`**, a typed enum (`PortConflict { port, owner }`, `DependencyFailed`, `DockerDaemonUnavailable`, …), never strings. Its `message()` is the user-facing text;
- **identity**: Projects and Services have a UUID and a **Key** (a slug such as `my-saas`, unique among Projects or within a Project). References inside the core (Dependencies, log files) use UUIDs; Keys are for people, and later for the CLI.

The main pieces:

| Type | Job |
|---|---|
| `ProjectLibrary` | The Project Configuration: validates and saves Projects and Services (`storage`, one JSON file per Project), keeps Secret values in the Keychain (`SecretStore`), builds a Run's environment. |
| `ServiceManager` | Starts, stops and restarts Services, one operation at a time per Service, in call order. Owns the Service States, Dependencies, Start All / Stop All, Port readiness and port moves. |
| `ProcessTree` | One Run's processes: launches `$SHELL -l -c "<command line>"` in its own session, reads stdout and stderr through separate pipes, and stops the whole group (SIGINT → SIGTERM → SIGKILL). |
| `LogStore`, `LogBuffer` | Each Run's output: an in-memory buffer for the viewer, and a log file per Run with rotation. |
| `RunRegistry` | Records active Runs on disk, so the next launch can find **Orphans** left by a crash. |
| `listener` | Finds every process listening on a TCP port (libproc on macOS, `/proc` on Linux, not `lsof`), which `ServiceManager::listeners()` attributes to a Run, a Project or nobody; stops one that localhost didn't start. |
| `ProjectScanner` + detectors | Reads a Project folder and returns **Suggestions** (Node, Compose, Python, Cargo). Reads only. |
| `ProjectDocument` | The YAML Project file for Export / Import ([CONFIGURATION.md](CONFIGURATION.md)). |
| `platform` | What differs per OS, behind small traits: process launch and stop, Listener scanning, process start times, the secret store, storage paths. |

### The bridge: `Packages/LocalhostCoreFFI`

The app calls the core through [UniFFI](https://mozilla.github.io/uniffi-rs/). The core's types derive their bindings behind its `ffi` feature, next to where they are defined, and `core/localhost-core/src/ffi/` adds the objects and functions Swift calls; `scripts/build-core.sh` builds the library and generates the Swift. The package compiles that into the module `LocalhostCore`, plus a thin layer in `Sources/LocalhostCore/`:

- the generated types are the model: the layer adds extensions (`Identifiable`, `Int` ports, `URL`s, `message`, `dependents(of:)`, …), never a second copy of a type;
- the library, manager and log store keep Swift labels and typed throws (`throws(LocalhostError)`) over the generated `ffi…` methods;
- the core's event streams become `AsyncStream`s, and `ServiceStatusStore` is a main-actor mirror of the Service States for SwiftUI to observe.

Logic belongs in the core, not the layer: the layer only reshapes. A new field in a Rust type reaches Swift on the next build. A new method needs an export in `src/ffi/` and, if the app should call it Swift-style, a wrapper in the layer. Error cases come out UpperCamel (`.PortConflict`), as UniFFI writes them.

A few rules the core keeps, which a change shouldn't break:

- A command line is one string run through the user's login shell, so their `PATH`, version managers and profile apply. There is no arguments field and no per-language runner.
- A Service's Port reaches the command only as `PORT=<port>`. localhost never rewrites command lines or knows framework flags at runtime (detectors may write `--port $PORT` into a Suggestion, which the user sees and edits).
- A requested stop always ends in **Stopped**, whatever the exit code. Stopped / Starting / Running / Stopping / Exited / Failed mean exactly what the README's table says.
- Configuration changes apply on the next start; a running Service shows "Restart to apply". Nothing restarts on its own.
- A Project's configuration is stored by localhost, never written into the Project's folder.
- Timeouts count only time the Mac is awake (`SuspendingClock`).

### The app: `App/`

A SwiftUI app and only a client of the core. `AppModel` owns the core (`ProjectLibrary`, `ServiceManager`, `LogStore`, `ServiceStatusStore`) and turns UI actions into core calls; views read it and never own processes or Runs.

- `Views/`: the main window (sidebar, Services column, log), the forms, Ports, Settings, the menu bar companion and the ⌘K palette.
- `Views/Theme.swift`: every colour and surface. Views don't hard-code colours.
- `AppIcon.icon`: the app icon, an Icon Composer document (open it with Icon Composer, in Xcode's Developer Tools) with its default (graphite), dark, tinted and clear looks. Xcode builds the icon for macOS 15 from the default look; an asset catalog app icon of the same name would be ignored.
- `LocalhostApp.swift`, `NavigationCommands.swift`: the scenes and the menu bar menus. Every action has a menu item (so it works from the keyboard and menu search), and its shortcut.
- The app isn't sandboxed: it runs arbitrary commands, reads process information and opens folders anywhere, which the App Sandbox doesn't allow. It is distributed outside the Mac App Store.

### Vocabulary

The code and the UI use the same words. When you name things, use these:

- **Project**: a folder whose Services localhost runs. **Service**: one long-running command in it (not "process", "task" or "script"). A Docker **Compose service** is a different thing, and one Service may run several.
- **Key**: the slug handle of a Project or Service. The **name** is the free-form label.
- **Run**: one execution of a Service, launch to the end of its whole process tree. A Service has at most one active Run.
- **Service State**: Stopped, Starting, Running, Stopping, Exited or Failed.
- **Dependency**: a Service that must be Running (or have Exited) before another starts.
- **Port**: the one configured port of a Service. **Listener**: any process listening on a TCP port; an **Unmanaged Listener** belongs to no Run and no Project.
- **Environment Profile**: a named set of variables for a whole Project; at most one is active. **Secret**: a variable whose value lives in the Keychain.
- **Suggestion**: a Service proposed by scanning a folder, applied only once the user checks it.
- **Orphan**: a process tree left over from a Run of a localhost session that ended unexpectedly.
- **Origin**: who asked for a change (main window, menu bar, …).

## Writing code

- **Swift 6 language mode with strict concurrency.** No `@unchecked Sendable` without a comment saying why it is safe.
- **Rust**: `cargo fmt`, and `cargo clippy --all-targets -- -D warnings` clean.
- **Few dependencies.** Each crate in `core/` is justified on its own; the app adds no Swift packages. Adding one needs a good reason, discussed in an issue first.
- **Tests**: the core's are Rust tests (`cargo test`), the Swift layer's use Swift Testing (`@Test`, `#expect`). The core is tested against **real processes**: shell commands that fork children, trap signals, print a lot or exit with a given code. Don't mock the process layer. Tests that wait for something get generous timeouts, since CI machines are slow.
- New behaviour in the core comes with tests. The app has no UI tests; check UI changes by running the app, in light and dark appearance, and with VoiceOver for anything interactive.
- **Accessibility is part of done**: rows are single VoiceOver elements with actions, icon-only buttons have a label naming what they act on and a tooltip with their shortcut, text meets WCAG AA contrast in both appearances, and movement is off under Reduce Motion (Reduce Transparency makes every surface solid).
- Match the surrounding code: its naming, its comment density (doc comments say *why*, not *what*), and its idioms.
- User-facing text is short and plain, uses the vocabulary above, and says what to do next when something goes wrong.

## Pull requests

1. Branch from `main`, one topic per branch.
2. Run `cargo test` in `core/` (and `swift test` in `Packages/LocalhostCoreFFI` if you touched the bridge), then build the app.
3. Open a PR describing what changed and why, how you checked it, and anything you left out. Link the issue it addresses.

CI runs the core's tests on macOS and Linux, the Swift layer's tests on a universal build, and an unsigned app build on every PR; all must pass. Commits are kept as written (merge commits, no squash), so make each commit message say what it does in one line.

Releases are cut by tagging `main`; see [RELEASING.md](RELEASING.md).

By contributing, you agree your contributions are licensed under the MIT license in [LICENSE](LICENSE).
