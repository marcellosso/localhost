# AGENTS.md

How localhost works, the words it uses, and the rules every change keeps. Read it before changing anything, whether you are a person or a coding agent. `CONTRIBUTING.md` covers building, testing and opening a pull request; the user-facing docs are `README.md` and `docs/configuration.md`.

## The product

localhost remembers how each of a developer's local projects runs, and gives them one place to start, stop and watch it. A Project is configured once; from then on the developer opens localhost, picks the Project and starts what they need, instead of a desktop full of terminal tabs whose only job is keeping processes alive.

> Open any project and immediately know how to run it.

It ships as a native SwiftUI app on macOS, one Tauri app for Linux and Windows (with Projects on Windows itself or in WSL), and a `localhost` command. All of them run on one Rust core.

localhost is **not** a Docker Desktop replacement, a container or Kubernetes tool, a production or remote-server dashboard, an IDE, a terminal emulator or a cloud service. Docker is just one way a Service may run. Node, Python, Bun, Cargo, Compose and plain shell commands should all feel equally natural.

### Principles

- **Project-first, not process-first.** People think in "My SaaS", not in PIDs. Processes are details under a Project and its Services.
- **Local-first.** Configuration, logs and state live on the machine. No account, no backend, works offline.
- **Commands are universal.** A Service is a working directory, a command line and an environment. Knowledge of a runtime may shape a *suggestion*, never how a Service runs.
- **Native and immediate.** Fast to launch, keyboard first, the platform's menus, windows and accessibility.
- **Useful before clever.** Solve forgotten commands and terminal clutter very well before anything fancier.
- **Beautiful is functional.** State, hierarchy and errors must read at a glance.
- **Nothing silent.** No automatic restarts, no Suggestions applied without review, no files written into the user's Project, no configuration rewritten behind their back.

## Repository

```text
core/                    Cargo workspace: the UI-free core, shared by every client
  localhost-core/        Projects, Runs, logs, environment, Listeners, detection, persistence
  localhost-cli/         the `localhost` command, and `localhost mcp` for coding agents
  localhost-daemon/      `localhostd --stdio`, the core driven over stdio (runs inside WSL distros)
  localhost-ffi/         the static library the macOS app links (UniFFI)
apps/macos/              the macOS app
  App/                   SwiftUI sources; Views/Theme.swift holds every colour and surface
  LocalhostCoreFFI/      the core as the Swift module `LocalhostCore`: generated bindings + a thin layer
  DMG/                   the release DMG's window
apps/desktop/            the Linux and Windows app (Tauri 2)
  src/                   React + TypeScript UI; src/bindings.ts is generated, never edited
  src-tauri/             Rust shell hosting the core
skills/                  agent skills, e.g. using localhost's MCP server
site/                    the website (Astro): landing page, docs, changelog; reads docs/ and the desktop tokens
scripts/                 build-core.sh, build-release.sh, signing and installer artwork
docs/configuration.md    the Project file format (user-facing)
```

## Vocabulary

The code, the UI and the docs use the same words. Use these, and avoid the alternatives listed.

| Term | Meaning | Avoid |
|---|---|---|
| **Project** | One local development project, rooted at a folder, grouping the Services needed to work on it. | stack, workspace, app, repo |
| **Service** | One independently manageable long-running command of a Project: working directory, command line, environment. Never inherently "a Node service". | process, task, job, script |
| **Compose service** | A service declared in a Docker Compose file. One Service may run one or many. | plain "service" |
| **Key** | The short, stable slug of a Project or Service (`my-saas`, `api`). Project Keys are unique app-wide, Service Keys within their Project. The **name** is the free-form label. | slug, id |
| **Project Configuration** | Everything localhost remembers about a Project. Stored by localhost, never in the Project's folder. | config file, manifest |
| **Project file** | The YAML file Export writes and Import reads, with Secrets by name only. | export, manifest |
| **Dependency** | A Service of the same Project that must be Running, or have Exited, before another starts. | requirement, prerequisite |
| **Environment Profile** | A named set of variables (Development, Testing) for a whole Project; at most one is active. | env, mode, stage |
| **Secret** | A variable whose value lives in the system's secret store, hidden until revealed. Any variable on any layer can be one. | credential |
| **Port** | The one TCP port a Service is expected to listen on. It decides when the Service is Running and where its URL points. Other ports it opens are not its Port. | listening port |
| **Suggestion** | A Service proposed by scanning a folder, applied only once the user checks it. | detected service |
| **Run** | One execution of a Service, launch until its whole process tree has ended. A Service has at most one active Run. | process, session, instance |
| **Execution Target** | Where a Project's Services run: this machine, or a WSL distro. | host, runtime, environment, agent |
| **Listener** | Any process listening on a TCP port, attributed to a Run, to a Project (its working directory is inside the folder), or to nobody. | port owner |
| **Unmanaged Listener** | A Listener that belongs to no Run and no Project. localhost can show and stop it, never adopt it. | rogue, stray |
| **Orphan** | A process tree left from a Run of a localhost session that ended unexpectedly. Never re-adopted as Running. | zombie, leftover |
| **Origin** | Who asked for a change: main window, menu bar, CLI, or an agent through `localhost mcp`. | source, actor, caller |
| **Service State** | Exactly one of the six states below. | status (in the UI) |

| State | Meaning |
|---|---|
| **Stopped** | Never started, or deliberately stopped, whatever exit code the stop produced. |
| **Starting** | A start was requested: waiting for Dependencies, or launched and not yet up. |
| **Running** | A Run is alive and has passed Starting. |
| **Stopping** | A stop was requested and the process tree hasn't fully ended. |
| **Exited** | The Run ended on its own with exit code 0. |
| **Failed** | The Run ended on its own unsuccessfully, or couldn't launch. |

## How it behaves

These are product rules, not implementation details. A change that breaks one needs an issue first.

**Running a command**
- A command line is one string, run as `$SHELL -l -c "<command line>"` in a new session, so the user's `PATH`, version managers and profile apply. On native Windows it is `cmd.exe /s /c "<command line>"` in a kill-on-close Job Object. There is no arguments field and no per-language runner.
- A Run's environment, later layers winning: the login environment (on Windows, the user's environment read fresh from the registry) → Project variables → Service variables → the active Environment Profile → `PORT`. Plus `FORCE_COLOR=1`, `CLICOLOR_FORCE=1`, `PYTHONUNBUFFERED=1`, because output goes through pipes.
- stdout and stderr are separate pipes, not a PTY. Lines over 64 KB arrive in pieces cut at a UTF-8 boundary; invalid UTF-8 shows as U+FFFD.

**States**
- Without a Port, a Run is Running after 1.5 s alive. With a Port, once the Port accepts connections; after 30 s it is Running anyway, with a warning that clears when the Port opens.
- Dying during Starting is Failed, with the output visible. Exiting 0 during Starting is Exited (one-shot steps such as migrations).
- A requested stop always ends in **Stopped**.
- Stop sends SIGINT to the process group, SIGTERM after 5 s, SIGKILL after 3 more; on Windows, Ctrl-Break and then terminating the job. The Service stays Stopping until the whole tree is gone, and the log says when it had to be forced.
- Timeouts count only time the machine is awake, so a Run frozen by sleep isn't escalated on wake.
- Failed carries a typed reason (`CommandNotFound`, `PortConflict`, `DependencyFailed`, `DockerDaemonUnavailable`, …) or the exit code or signal. Docker's "daemon not running" is diagnosed from the output after the fact, never pre-checked.

**Dependencies**
- Starting a Service starts its Dependencies first and waits until each is Running or Exited. Start All goes in dependency order; Stop All in reverse.
- Stopping or restarting a Service leaves its dependents alone. If a Dependency fails, dependents don't start and say why.
- "Not in Start All" skips a Service unless something being started depends on it. Cycles are rejected when edited.

**Ports**
- The Port reaches the command only as `PORT=<port>` (`$PORT`, or `%PORT%` in cmd). localhost never rewrites command lines or knows framework flags at runtime.
- If the Port is taken, a Run may move to the next free port up to +100 that isn't another Service's Port, when the app setting and the Service's "Port can change" allow it. Never for a Service with dependents, or when the holder is inside the same Project. The move lasts for that Run only; the configuration keeps the Port.
- A blocked start shows who holds the port and offers Stop it and start, Start on <next port>, or Cancel.
- Open in Browser: the configured URL with its port moved to the Run's, else `http://localhost:<port>`, else the first local URL the Run printed (`0.0.0.0` shown as `localhost`).
- Listeners are scanned on demand and polled about every 2 s only while a view shows them. localhost refuses to stop itself, launchd jobs, Windows services and Docker's backend.

**Configuration**
- Configuration changes apply on the next start. A running Service whose launch configuration changed shows "Restart to apply". Nothing restarts on its own.
- Project Configuration is one JSON file per Project in localhost's storage, written atomically. An unreadable file is moved aside as `<name>.unreadable`, reported at launch and never loaded or overwritten; a file from a newer version is left in place.
- Secret values live only in the secret store (Keychain, Secret Service, Credential Manager), never in JSON, logs or Project files. WSL distros get them in memory, at start.
- Removing a Service or Project stops it first and deletes its logs. The Project's folder is never touched.

**Logs**
- Each Run keeps an in-memory buffer of 10,000 lines for viewers and a log file. The last 5 Runs per Service are kept, up to 10 MB each with rotation. "All services" is a merged view at read time, each line prefixed with its Service's Key. Clearing empties buffers, never files.

**Sessions**
- Closing the window keeps the app and its Services running; the menu bar or tray shows them. Quitting asks first when anything is active, then stops everything in parallel, whoever started it.
- On macOS and Linux the apps and `localhost mcp` drive one shared core, so they see the same Runs. When a client disconnects, the core stops the Runs nobody needs any more: none while an app is connected, else those no remaining client started (with their Dependencies). It exits once no client has been connected for 10 s.
- Each active Run is recorded on disk (pid, process group, start time). At the next launch, survivors are offered as Orphans: Stop them (default) or Leave them.

**Detection**
- Scanning only reads: no network, nothing written. It looks at the root, folders up to two levels deep (skipping dependency, build and hidden folders) and folders detectors declare (workspace packages, Cargo members).
- Detectors (Node, Compose, Python, Cargo) are small and independent and return plain command lines. Every Suggestion is reviewed: likely ones checked, the rest folded away, each editable before anything is saved.

**Project files**
- One YAML file per Project, `localhost: 1` at the top, written by a built-in emitter and read by a strict built-in reader (`docs/configuration.md`). Same Configuration, same file; unknown fields are errors; export → import → export is identical.
- Import always goes through a review. On a Key clash: Replace (keeps the Project's identity, logs and matching Secrets) or Keep both (a free Key such as `my-saas-2`).

## Architecture

```text
                 localhost-core (Rust)
   Projects · Runs · logs · environment · Listeners · detection
                          │
      ┌───────────────────┼────────────────────┬──────────────────┐
  SwiftUI app        Tauri app             localhost CLI       localhostd
  macOS, via UniFFI  Linux + Windows,      same crates         inside a WSL distro,
                     links the crates                          driven over stdio
      └─────────┬─────────┘                    │
          the shared core: one per user, on a local socket (macOS, Linux)
```

All state, process management, logs and persistence live in the core, which never depends on a UI toolkit. The apps only draw and turn user actions into core calls. Its API is shaped so any client, in-process or over IPC, drives the same core:

- **async, on tokio**; every public type is `Send + Sync`, and serializable where it is data;
- **snapshots plus events**: clients read the current state and subscribe to a stream of changes, taken together so nothing falls between them;
- **every state-changing call takes an `Origin`**;
- **failures are `LocalhostError`**, a typed enum, never strings; `message()` is the user-facing text;
- **identity**: Projects and Services have a UUID and a Key. Internal references (Dependencies, log files, Secret accounts) use UUIDs; Keys are for people and the CLI.

| Piece | Job |
|---|---|
| `ProjectLibrary` | Validates and saves the Project Configuration (`storage`), keeps Secrets in a `SecretStore`, resolves a Run's variables. |
| `ServiceManager` | Starts, stops and restarts Services, one operation at a time per Service, in call order. Owns Service States, Dependencies, Start All / Stop All, Port readiness and moves. |
| `ProcessTree` | One Run's processes: launch, separate stdout/stderr pipes, stopping the whole group. |
| `LogStore`, `LogBuffer` | Each Run's output: in-memory buffer and rotated file. |
| `RunRegistry` | Records active Runs on disk, to find Orphans. |
| `Targets`, `remote` | Where each Project runs, behind one interface: this machine through the shared core (in-process on Windows), a WSL distro through a `localhostd`. Merges every target's statuses and events. |
| `remote::shared` | The shared core: the `remote` protocol on a Unix socket in the storage root, started by its first client, one per storage root. Decides which Runs a disconnecting client takes with it. |
| `listener` | Finds Listeners (libproc, `/proc`, `GetExtendedTcpTable`, never `lsof`) and attributes them. |
| `ProjectScanner` + detectors | Reads a folder and returns Suggestions. |
| `ProjectDocument` | The YAML Project file. |
| `platform` | What differs per OS, behind small traits: launching and stopping, Listener scans, process start times and usage, the secret store, storage paths. |

**The macOS bridge.** The core's types derive UniFFI bindings behind its `ffi` feature, next to where they are defined; `core/localhost-core/src/ffi/` adds the objects and functions Swift calls, and `scripts/build-core.sh` generates the Swift. `apps/macos/LocalhostCoreFFI/Sources/LocalhostCore/` is a thin layer: extensions on the generated types (never a second copy), Swift labels and typed throws (`throws(LocalhostError)`), `AsyncStream`s, and `ServiceStatusStore`, a main-actor mirror for SwiftUI. Logic belongs in the core; the layer only reshapes. Error cases come out UpperCamel (`.PortConflict`).

**The desktop bridge.** `apps/desktop/src-tauri/` exposes typed commands and events; `src/bindings.ts` is generated from them and the core's types (its `specta` feature) by `cargo test` in `src-tauri/`. The UI receives a snapshot, then diffs; log lines arrive in batches at most once per frame.

**The macOS app.** `AppModel` owns the core and turns UI actions into core calls; views read it and never own processes or Runs. Every action has a menu item (so it works from the keyboard and menu search) and its shortcut. The app is not sandboxed and ships outside the Mac App Store.

## Platforms

| | macOS | Linux | Windows |
|---|---|---|---|
| App | SwiftUI, macOS 15+ | Tauri: `.deb`, `.rpm`, AppImage | Tauri: per-user NSIS installer |
| Command | `$SHELL -l -c` | `$SHELL -l -c` | `cmd.exe /s /c`, or a WSL distro's `$SHELL -l -c` |
| Process tree | session + process group | session + process group | kill-on-close Job Object |
| Listeners | libproc | `/proc/net/tcp{,6}` + `/proc/<pid>/fd` | `GetExtendedTcpTable`, working directory from the PEB |
| Memory shown | physical footprint | PSS (`smaps_rollup`) | private bytes |
| Secrets | Keychain | Secret Service | Credential Manager |
| Storage | `~/Library/Application Support/localhost` | `$XDG_DATA_HOME/localhost` | `%APPDATA%\localhost` |
| Tray | menu bar extra | AppIndicator | notification area |

The Keychain service and Secret account names (`io.github.marcellosso.localhost.secret`, `<OWNER-ID>/<NAME>`), the storage layout and the JSON schema must never change: existing users' data depends on them, and `core/localhost-core/tests/swift_compatibility.rs` checks files written by earlier versions.

## Decisions

The hard-to-reverse choices, and why. Changing one is an issue and a discussion first.

- **One Rust core for every client.** Windows is where the hardest code lives (Job Objects, console control events, TCP tables, Credential Manager), and Rust is first-class there. The macOS app links it through UniFFI.
- **One shared core per user, on a local socket.** The app and coding agents must see the same Runs, so on macOS and Linux every client drives one core in the background, over the same protocol as a WSL `localhostd`. Any localhost executable can serve it (`localhost daemon`, the app with `--daemon`), so the `localhost` command, built on its own, needs no second binary. Runs belong to the core, not to a connection: a client disconnecting stops only what no one else needs, and an open app needs everything it shows. Quitting the app still stops every Service. Clients send the Projects and Secret values, as to a WSL `localhostd`, so the core never touches the Keychain; it keeps only the session variables of the client that started it, so one client's environment doesn't leak into every Run.
- **SwiftUI on macOS, one Tauri app for Linux and Windows.** It keeps the native macOS app, avoids three toolkits and avoids Electron's size. The cost: every feature is built twice, so both UIs stay thin and all logic is in the core.
- **Commands through the login shell.** GUI apps don't inherit the `PATH` set by nvm, asdf, mise, pyenv or Homebrew. A single command line also keeps pipes, `&&` and variables working as in a terminal.
- **cmd.exe on native Windows, with a fresh environment.** It is everywhere, fast, supports `&&`, runs npm's `.cmd` shims, and is what npm uses. The environment is rebuilt from the registry each start, since a GUI app's inherited one is stale. Stdin is `NUL`, so "Terminate batch job (Y/N)?" never waits.
- **Windows Runs in Job Objects, stopped with Ctrl-Break.** Windows has no process-group signals; a job holds the whole tree and dies with localhost even on a crash. Ctrl-Break needs a shared console, so the app attaches once to a hidden one.
- **WSL Projects run by a core inside the distro.** A Windows process sees nothing behind `wsl.exe`. The app copies its `localhostd` into the distro and drives it over stdio: no socket, port or firewall rule. Configuration and Secrets stay on Windows.
- **Configuration stored by localhost, not in the Project folder.** Writing into users' repos is a surprising side effect. The Project file is the explicit bridge.
- **Pipes, not a PTY.** Keeping stderr distinct matters for failures and, later, for agents. The colour hints make up for the missing terminal.
- **`$PORT`, and moves scoped to one Run.** It is the existing convention, needs no parser and works the same from any client. A moved port never rewrites the configuration.
- **Run stats sampled on request.** Nothing is sampled unless a stats popover is open (every 1.5 s). The tree includes descendants that left the session. Memory is each platform's honest number rather than RSS, which counts forked workers once each.
- **Not sandboxed; no Flatpak or Snap.** Sandboxes can't run arbitrary host commands or signal process groups.

## Design

The look is "Graded glass": dense, calm, light and dark. **No Liquid Glass.**

- **Glass steps down left to right**: the sidebar is most translucent, the Services column lighter, the log always opaque. Under Reduce Transparency, or with translucency off, every surface is solid.
- **Colour has a job**: one accent for primary actions and selection; green, orange, red and grey only for Service State; each Service has a stable colour by its position in the Project, used wherever it appears (row, log prefix, menu bar, Ports).
- **State glyphs carry state by shape**: a dot with a halo (Running), a pulsing ring (Starting, Stopping), a diamond (Failed), a hollow ring (Stopped, Exited).
- **Monospace is a voice**: Keys, ports, commands and paths are always monospaced.
- **Primary actions are labelled**; icon-only controls name their shortcut in a tooltip; rare actions live in `⋯` and context menus.
- Colours live in `apps/macos/App/Views/Theme.swift` and `apps/desktop/src/styles/tokens.css`, with the same values. Change both together; views never hard-code colours.
- The menu bar companion and the tray are quick controls, not a second app: Projects with an active or Failed Service plus favourites, their Services' Start / Stop / Restart / Open, Start All / Stop All, and a red dot while anything is Failed. Configuration, environment and logs stay in the main window.

## Accessibility

Part of done, on every platform.

- A Service row, Project row, Listener row and menu bar Service are each one screen-reader element: the name as label; state, summary, Port and command as value; the buttons as its actions. Icon buttons name what they act on ("Stop Web").
- A Service settling (Running, Exited, Failed, Stopped) is announced, with its Project when that isn't the selected one; Failed interrupts. Starting and Stopping aren't announced, so Start All isn't a stream of speech.
- Everything works from the keyboard: a menu item or shortcut for every action, arrow keys across the columns, a command palette (⌘K / Ctrl+K) that shows each action's shortcut.
- Text meets WCAG AA in both appearances (4.5:1, glyphs 3:1), also on selected rows and tinted pills.
- Reduce Motion turns movement off (chevrons, growing rows, the pulsing ring); fades stay.

## Performance

The log must stay smooth with very chatty Services. Lines never cross to a UI one at a time: the core keeps the buffers, apps take batches (at most once a frame on the desktop app, about every 50 ms on macOS) and render only the visible rows. WebKitGTK on Linux is the slowest webview: run `apps/desktop/scripts/log-perf.sh` there before and after any change on the log path, and put the numbers in the PR.

## Writing code

- **Match the surrounding code**: its naming, idioms and comment density. Doc comments say *why*, not *what*; don't restate a name, narrate history or cite issue numbers.
- **Few dependencies.** Each crate is justified on its own; the macOS app adds no Swift packages. Adding one needs an issue first. Beyond `serde_json`, manifests (TOML, YAML) and Project files are read and written by small built-in readers.
- **Rust**: `cargo fmt`, and `cargo clippy --all-targets -- -D warnings` clean, also for `--target x86_64-pc-windows-msvc`.
- **Swift**: Swift 6 language mode, strict concurrency. No `@unchecked Sendable` without a comment saying why it is safe.
- **TypeScript**: strict; `pnpm typecheck` and `pnpm test` clean. Never edit `src/bindings.ts`.
- **Tests**: new core behaviour comes with tests, against **real processes** (shell commands that fork, trap signals, flood output, exit with codes; Python fixtures on Windows). Don't mock the process layer. Waits get generous timeouts: CI machines are slow. The Swift layer uses Swift Testing (`@Test`, `#expect`); the desktop UI uses Vitest. There are no end-to-end UI tests: check UI changes by running the app in light and dark, and with a screen reader for anything interactive.
- **User-facing text** is short and plain, uses the vocabulary above, and says what to do next when something goes wrong. Errors are worded once, in the core.

## Verifying a change

```sh
cd core && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
cargo clippy -p localhost-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings

scripts/build-core.sh debug && (cd apps/macos/LocalhostCoreFFI && swift test)
xcodebuild -project apps/macos/localhost.xcodeproj -scheme localhost -configuration Debug \
  -derivedDataPath build CODE_SIGNING_ALLOWED=NO build

cd apps/desktop && pnpm install && pnpm typecheck && pnpm test
pnpm build && (cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings)
```

Run only what the change touches. `cargo test` in `src-tauri/` regenerates `src/bindings.ts`; commit it, CI fails if it is stale. Try changes without touching real data with `LOCALHOST_STORAGE_ROOT=/tmp/localhost-dev`.

## Workflow

- Issues are on GitHub (`gh issue view <n> --comments`). Triage labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`; kinds: `bug`, `enhancement`, `accessibility`, `documentation`.
- Branch from `main`, one topic per branch; open a PR that says what changed and why, how it was checked, and what was left out, and links the issue. Don't widen the scope beyond the issue without asking.
- Commits are kept as written (merge commits, no squash): one line saying what the commit does.
- Releases are cut by tagging `main` (`RELEASING.md`).

## Where it is going

A local development control plane: once localhost knows every Project, how each Service starts, what is running, its ports, environment and logs, a coding agent shouldn't need to guess shell commands. The CLI drives the core, and `localhost mcp` serves it to agents over MCP, through the shared core the apps drive too. Design for it:

- semantic actions (`start_service(project: "my-saas", service: "api")`), never a generic "run this shell command";
- stable identifiers (Keys, UUIDs) independent of display names;
- structured errors (`PortConflict { port, owner }`), never "Command failed";
- bounded output: log reads take limits, windows or searches;
- no hidden side effects: reads stay read-only, changes are explicit and carry an Origin.
