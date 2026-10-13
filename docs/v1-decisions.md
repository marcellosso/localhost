# V1 engineering decisions

Settled decisions for building V1. `localhost-v1.md` / `localhost-v2.md` are product direction; where they disagree with this file, **this file wins**. Vocabulary is defined in `GLOSSARY.md`; the hard-to-reverse decisions and their reasoning are in `docs/adr/`.

## Architecture

- **UI-free core** (ADR-0001; in Rust since XP2, ADR-0007 and `docs/cross-platform.md`). `core/` holds all project state, Run lifecycle, logs, environment, persistence. It never imports SwiftUI or AppKit. The app target is only a client. In V1 the core runs in-process; V2 may host it in a daemon behind CLI/MCP, so keep the API shaped for that:
  - async API; state exposed as observable snapshots + an event stream (`serviceStateChanged`, `runStarted`, `runEnded`, …);
  - every state-changing call takes an **Origin** (`.mainWindow`, `.menuBar`; later `.cli`, `.agent`);
  - failures are a typed error enum (`portConflict(port:owner:)`, `commandNotFound`, `workingDirectoryMissing`, `dependencyFailed(…)`, `dockerDaemonUnavailable`, …), never bare strings;
  - all core types `Sendable`/`Codable`-friendly.
- **Identity**: Projects and Services have a UUID identity plus a **Key** (slug). Project Keys unique app-wide, Service Keys unique per Project; editable but validated. Internal references (Dependencies, log files) use UUIDs.
- **Layout**: `apps/macos/` holds the macOS app: `localhost.xcodeproj`, `App/` (SwiftUI target) and `LocalhostCoreFFI/` (the core as the Swift module `LocalhostCore`). `core/` is the Cargo workspace, `apps/desktop/` the Tauri app. No Tuist/XcodeGen.
- **Toolchain**: Swift 6 language mode, strict concurrency, macOS 15 minimum, bundle id `com.sixpetals.localhost` (Keychain service for Secrets: `com.sixpetals.localhost.secret`). Non-sandboxed, distributed outside the Mac App Store (ADR-0002).
- **Dependencies**: none by default; each third-party package must be justified (an MCP SDK in V2 is expected).
- **Tests**: Swift Testing (`@Test`). The core is test-driven against **real processes** (fixture scripts that fork grandchildren, trap signals, exit with codes). No UI tests in V1.

## Persistence

- Project Configuration is stored by localhost, never in the project folder (ADR-0004): `~/Library/Application Support/localhost/projects/<project-uuid>.json`, one Codable JSON file per Project, atomic writes, `schemaVersion` field.
- Service working directories are stored **relative to the Project path**.
- Secret values live in the Keychain, not in JSON, and are never exported (M1).
- **Unreadable Project files** never stop the others from loading, and are never silently dropped. A damaged file (or a second copy of a Project or of its Key; the `<id>.json` file wins) is moved aside as `<name>.unreadable`, so it is kept but never loaded or overwritten. A file from a newer schema version is left in place for that version. Both are listed in an alert at launch, before the Orphans question. Only an unreadable `projects` folder fails the core.
- Removing a Service or Project (or dropping it on Replace) deletes its log files.

## Running Services

- **Command**: one command line string, run as `$SHELL -l -c "<command line>"` in a new process group / session (ADR-0003). No separate arguments field.
- **Environment of a Run** (later layers override earlier): login-shell env → Project variables → Service variables → active Environment Profile. Plus output hints `FORCE_COLOR=1`, `CLICOLOR_FORCE=1`, `PYTHONUNBUFFERED=1`.
- **Output**: stdout and stderr via separate pipes (ADR-0005). Viewer renders ANSI colour; stderr lines get a subtle marker.
- **Logs**: in-memory ring buffer per Run for the viewer, plus each Run's output written to `~/Library/Application Support/localhost/logs/…`. Keep last 5 Runs per Service, 10 MB cap each (rotate). The combined project stream is a merged read-time view.
- **Service States**: Stopped / Starting / Running / Stopping / Exited / Failed, exactly as defined in `GLOSSARY.md`. A requested stop always ends in **Stopped** regardless of exit code. A Service has at most one active Run.
- **Starting → Running**: no Port configured → alive after a ~1.5s grace period; Port configured (M1) → the Port accepts connections (global 30s timeout → Running with a warning that clears once the Port opens). Dying during Starting → **Failed** with output visible; exiting with code 0 during Starting → **Exited** (one-shot commands such as migrations).
- **Configuration changes** (variables, active Profile, command line, …) apply on the next start. A Run whose launch configuration differs from what would be built now shows "Restart to apply" (computed in the core). Nothing restarts automatically.
- **Failed** shows the typed reason (or exit code/signal) plus the last 20 lines of both streams. A Failed Run whose output contains `Cannot connect to the Docker daemon` (or, from current Docker CLIs, `failed to connect to the docker API`) reports `dockerDaemonUnavailable` instead (diagnosed after the fact, never a pre-check).
- **Timeouts** (stop escalation, the Port timeout, the grace period) count only time the Mac is awake (`SuspendingClock`), so a Run frozen by sleep isn't escalated or timed out on wake.
- **Output lines** longer than 64 KB are delivered in pieces of at most 64 KB, cut at a UTF-8 character boundary; bytes that aren't UTF-8 show as U+FFFD.
- **Stop escalation**: SIGINT to the process group → wait ~5s → SIGTERM → wait ~3s → SIGKILL. State stays **Stopping** until the whole tree is gone; log a line if force-killed. Global defaults for now.
- **Orphans**: each active Run's (pid, pgid, process start time) is recorded on disk. On launch, survivors are detected (start time guards against PID reuse) and the user is asked "N services from the last session are still running — Stop them / Leave them" (default Stop). Never re-adopt them as Running.
- **App lifetime**: closing the main window does not quit; Services keep running. ⌘Q / menu-bar Quit asks for confirmation if anything is running, then stops everything in parallel. Dock icon only while the main window is open (`.regular` ↔ `.accessory`), once the menu bar exists.

## Dependencies between Services (M1)

- Start All starts in dependency order and waits for each Dependency to reach Running.
- Starting one Service also starts its not-yet-running Dependencies.
- Stopping one Service does **not** stop its dependents. Stop All stops in reverse dependency order.
- If a Dependency fails, dependents don't start and show the reason. Cycles are rejected at edit time (`dependencyCycle([Key])`; the form disables choices that would close a cycle). Deleting a Service removes it from every Dependency list, after confirming.
- A Dependency is satisfied when it is Running **or has Exited** (one-shot steps such as migrations). A Service waiting for its Dependencies is **Starting** with no new Run yet; its Run is launched once they are satisfied.
- Restart = Stop + Start of that one Service; dependents are untouched.
- A per-Service **"Not in Start All"** flag skips it in Start All, unless something being started depends on it (Dependencies always win).
- Stop All (or stopping a Service others are waiting on) cancels pending starts; those Services end **Stopped**, not Failed.

## Ports and Listeners (M1)

- One optional **Port** per Service (plus an optional URL). Other ports a Service opens are visible via discovery but aren't its Port.
- The Port reaches the command as `PORT=<actual port>` (ADR-0006); command lines use `$PORT`. No placeholder syntax, no framework flags.
- Open URL priority: configured URL with its port rewritten to the actual port → `http://localhost:<actual port>` → first URL detected in the Run's output. Detected URLs are clickable in logs; `0.0.0.0` displays as `localhost`.
- **Listener discovery** in the core via libproc (`proc_listpids`, `proc_pidfdinfo`, `proc_pidinfo` for cwd), not `lsof`. Scanned on demand (before starting a Service with a Port, when a Listener view opens) and polled ~2s only while a Listener view is visible.
- Attribution: owned by a Run (its process group) → inside a Project (cwd under the Project path) → **Unmanaged Listener**.
- Stopping a Listener we don't run: confirm (command, PID, cwd); signal its process group unless the group leader is a session leader (then the PID and its descendants); SIGINT → SIGTERM → SIGKILL. Refuse localhost itself and launchd jobs. No "keep/pin/ignore" action: Listeners appear while they listen.
- **Automatic port move**: app setting (default on) + per-Service "Port can change" (default on; the form hints when the command line doesn't mention `$PORT`). Next free port searching upward up to +100, skipping ports configured as other Services' Ports. The moved port belongs to that Run only; configuration is never rewritten. The UI says which port and why.
- Never auto-move when the Service has dependents (#15), or when the port is held by a Listener inside the same Project (likely its own leftover).
- A blocked start shows the owner and offers **Stop it and start** / **Start on <next free port>** (explicit, warns about dependents) / **Cancel**. During Start All a blocked Service fails with `portConflict`; its dependents get `dependencyFailed`.

## Environment (M1)

- Environment Profiles are Project-wide (per-Service overrides: #14).
- Any variable on any layer (Project, Service, Profile) can be a Secret: one Keychain generic-password item per Secret; JSON stores the name and `isSecret` only. Reveal is an eye-icon click (no authentication in V1); copying never logs the value.

## Docker Compose (M1)

No adapter in V1. A Compose service is just a command line run in the foreground (`docker compose up postgres`); Stop's SIGINT makes Compose shut down cleanly. Detection suggests the foreground form (the `-d` form would just end as Exited). Only extra: recognise "Docker daemon not running" and surface it as a typed error.

## Project detection (M2)

- `ProjectScanner` in the core runs small, independent `ServiceDetector`s (Compose, Node, Python, Cargo) over the Project root, every folder up to two levels deep (skipping dependency, build and hidden folders) and the folders detectors declare (Node workspace packages, Cargo workspace members). It only reads: no network, nothing written into the folder. Manifests are read by small built-in readers (JSON, a TOML subset, a YAML subset), with no third-party parser.
- Output is a list of **Suggestions**: plain command lines with working directory, Port, Port can change, whether it is checked by default, and the file it came from. Keys are unique within a scan.
- **Node**: every `package.json` script except lifecycle hooks is suggested, run as `<pm> run <script>`. The package manager comes from the nearest `packageManager` field, else the nearest lockfile (pnpm / Yarn / Bun / npm), else npm. One script per package is checked (`dev` › `develop` › `start` › `serve` › `watch` › first `dev:…`). In a workspace monorepo, the root's scripts (`turbo dev`) and packages outside the workspaces are listed unchecked.
- **Ports**: a known dev server (Vite, Next, Nuxt, Astro, Angular, Storybook, Wrangler, Gatsby, webpack, Docusaurus, Expo, react-scripts) gets its default port. When the tool takes a port flag, `$PORT` is passed to it (`pnpm run dev --port $PORT`, npm adds `--`). Tools that read `PORT` themselves run as written. A literal port in the script is kept with Port can change off. This is knowledge in a detector shaping a visible suggestion, not runtime behaviour (ADR-0006 still holds).
- **Compose**: only the file Docker would pick (`compose.yaml` › `compose.yml` › `docker-compose.yaml` › `docker-compose.yml`). One checked suggestion per Compose service, `docker compose up <svc>` in the foreground, Port = first published host port, Port can change off.
- **Python**: a folder with `pyproject.toml`, `requirements.txt`, `setup.py`, `Pipfile` or `manage.py`. Runner: uv / Poetry / Pipenv (lockfile walking up to the root, or `pyproject.toml`), else `.venv`/`venv`, else `python3`. Django → `manage.py runserver $PORT` (8000). A module-level FastAPI app → `uvicorn <module>:<app> --reload --port $PORT` (8000, `--app-dir src` for src layouts). Flask → `flask --app … run --debug --port $PORT` (5000). `pyproject.toml` scripts are listed unchecked.
- **Cargo**: `cargo run` for a package with `src/main.rs`; other binaries are listed unchecked as `cargo run --bin <name>`. Libraries and virtual workspace roots get nothing.
- **Review step**: Add Project scans as soon as the folder is chosen and lists the Suggestions under the name fields: likely Services checked, the rest folded under "N more found". Each row opens into an inline editor (name, Key, working directory, command line, Port, Port can change) and says which file it came from. The Project and its checked Services are saved in one write (`addProject(…, services:)`), as are the ones picked in **Detect Services…** (`addServices`).
- No suggested Dependencies in M2. **Detect Services…** rescans an existing Project; suggestions matching an existing Service (same working directory and command line) are hidden.

## UI (M1)

- Sidebar: Projects, plus a **Ports** view listing every Listener. The Project's Service list starts with an **All services** row (merged stream, Key-prefixed lines, colour stable per Service); clear there clears every visible buffer, never log files.
- Menu bar: Projects with any active or Failed Service, plus favourites; a red dot on the icon while any Service is Failed; only warns about Unmanaged Listeners holding a configured Port.
- "Open in editor": an app picked once (`NSWorkspace.open(_:withApplicationAt:)`), no default.
- Shortcuts: ⌘R start/restart, ⌘. stop, ⇧⌘R Start All, ⇧⌘. Stop All, ⌘F find, ⌃L clear logs, ⇧⌘O open URL, ⌥⌘T Terminal, ⌥⌘E editor, ⌥⌘F Finder, ⌘1–9 Projects, ⌘0 Ports, ⌥⌘1–9 Services, ⌘[ / ⌘] previous/next Service. Holding ⌘ (or ⌥⌘) ~0.4s shows number badges. ⌘K command palette and arrow-key column navigation (M2, below).

## Command palette (M2)

- ⌘K (Go menu) opens a palette over the main window: one search field over jumps (Projects, Services, Ports), Service actions (Start / Restart / Stop / Open in Browser / Terminal / Editor) and Project actions (Start All / Stop All / Editor / Terminal / Finder), offered according to each Service's state.
- Empty query: the selected Service's actions, the selected Project's Services and actions, the other Projects, Ports and New Project. Typing searches everything, including other Projects' Services and actions.
- Matching is `FuzzyMatch` in the core (every query word as letters in order, word starts and runs score higher), so a future CLI can resolve names the same way. Equal matches rank jumps first, then the selection.
- Rows show the shortcut that does the same outside the palette, so the palette teaches shortcuts. ↑/↓ or ⌃P/⌃N move, ↩ runs, esc closes. Closing gives focus back to where it was (or to the Services list after a jump).

## Keyboard navigation (M2)

- Focus moves across the columns like Finder's column view: → (and ↩ in the sidebar) moves right, ← moves left, esc in the log goes back to the Services list. ↑/↓ move within a list, as usual; the selection follows. Keys typed into text fields, and keys while a sheet or alert is up, are left alone.
- In the log, keys scroll instead of moving an invisible insertion point: ↑/↓ a line, space / ⇧space and page up/down a page, ⌘↑/⌘↓ and home/end to either end. Scrolling up pauses following, back at the bottom resumes it, as with the trackpad.
- Sidebar search: ↓ moves into the list, ↩ opens the first match's Services. Log filter (⌘F): ↩ keeps the filter and moves into the log, esc clears it.
- Done with an app-local key monitor on the main window rather than SwiftUI `onKeyPress`, because the lists (`NSTableView`) and the log (`NSTextView`) consume arrow keys before SwiftUI sees them.

## Release (M3)

- MIT license.
- CI (GitHub Actions, `macos-26`): every PR and push to `main` runs the core's `cargo test` (macOS and Linux), the Swift layer's `swift test` and an unsigned app build. All the macOS work is one job, skipped on draft PRs; the universal release build only runs in the release workflow.
- Distribution is a DMG on GitHub Releases, built by a workflow on `v*` tags into a **draft** release (notes generated from merged PRs) that the maintainer publishes. Version comes from the tag (SemVer, `0.x` for now). Fixes: branch → PR → merge → patch tag. No Sparkle, no in-app update check.
- **Signing**: no Apple Developer Program yet. Releases are signed with a long-lived (20-year) **self-signed certificate** kept in a repo secret (and backed up by the maintainer), so the Keychain keeps trusting the app across updates. Without that secret the workflow falls back to ad-hoc signing. When the Apple secrets exist it signs with Developer ID (hardened runtime), notarizes with `notarytool` and staples, with no other workflow change. The app is built unsigned and signed by `scripts/build-release.sh`, so every mode takes the same path; `scripts/create-signing-certificate.sh` creates the certificate and stores its secrets. The process and the secret names are in `RELEASING.md`.
- Consequences for users, documented in the README: a non-notarized build needs System Settings → Privacy & Security → **Open Anyway** on first launch (or `xattr -dr com.apple.quarantine`); an ad-hoc build asks again for Keychain access to each Secret after every update; switching signing identity (self-signed → Developer ID) asks once more, on that one update.
- **Public repo**: public releases come from a fresh repo under the sixpetals organisation, starting from a single initial commit (#40). `docs/` stays private, so user-facing docs (README, configuration reference, `CONTRIBUTING.md`) live outside it. Nothing is made public in M3.

## Export and import (M3)

- One YAML file per Project, close to `localhost-v1.md` §19, with a top-level `localhost: 1` format version. A small built-in emitter writes it; the built-in YAML-subset reader is extended for this schema (no dependency). Encode/decode lives in the core (`ProjectDocument`), reusable by a future CLI or in-repo file (ADR-0004).
- Contains: name, Key, `path` (home folder as `~`), Services (name, Key, working directory relative to the Project, command line, Port, URL, Port can change, Not in Start All, `depends_on` by Service Key), variables on every layer, Environment Profiles by name, the active Profile.
- Fields: `localhost`, `name`, `key`, `path`, `variables` (name → value), `secrets` (names), `services` keyed by Service Key with `name`, `command`, `cwd`, `port`, `url`, `port_can_change`, `start_all`, `depends_on`, `variables`, `secrets`, then `profiles` keyed by name (`variables`, `secrets`) and `active_profile`. Fields at their default are left out and variables are sorted, so the same Configuration always gives the same file. Unknown fields are errors, so a typo isn't silently dropped.
- Not exported: UUIDs, sort order, favourite, any Secret value. Secrets are listed by name without a value, so the import shows what still needs filling in.
- Import opens a review sheet: path prefilled; if the folder is missing the user must pick one. Keys, working directories, Dependencies (unknown Keys, cycles) and ports are validated with the editors' rules; errors name the line/field.
- Key clash: ask once, **Replace** (keeps the existing Project's UUID and log history, swaps in the imported configuration) or **Keep both** (the import gets a free Key, e.g. `my-saas-2`). Nothing is overwritten without Replace. Replace keeps the identity of Services matching by Key and Profiles matching by name, so their logs and the stored values of Secrets they still name survive; Services the file drops are stopped first.
- Round trip: export → import → export yields an identical file.

## UI direction

"Graded glass" (#26; mockups in `docs/design/ui-refresh/`): dense, calm, dark and light. **No Liquid Glass.**

- **Glass steps down from left to right**: the sidebar is most translucent, the services column (and Ports) is lighter glass, the log is always opaque. Under Reduce Transparency every surface is opaque.
- **Colour has a job**: one accent (pink) for the primary action and selection only; green / orange / red / grey only for Service State; each Service has a stable colour by its position in the Project, used wherever that Service appears (row, log prefix, menu bar, Ports).
- **State glyphs carry state by shape**: a dot with a halo (Running), a pulsing ring (Starting/Stopping, still under Reduce Motion), a diamond (Failed), a hollow ring (Stopped/Exited).
- **Monospace is a voice**: Keys, ports, commands and paths are always SF Mono; everything else SF Pro.
- **Primary actions are labelled**; icon-only controls name their shortcut in the tooltip; rare actions live in `⋯` and context menus.
- Colours and surfaces live in `Theme` (App target); views don't hard-code colours.

## Accessibility (M3)

- **VoiceOver**: a Service row, a Project row, a Listener row and a menu bar Service are each one element: the name as label, then state, summary, Port and command as value. Their buttons are its actions (Start / Stop / Restart / Open in Browser / Edit Service; Stop for a Listener). Summaries are spoken without "·". Icon buttons name what they act on ("Stop Web").
- **Announcements**: while VoiceOver runs, a Service settling (Running, Exited, Failed, Stopped) is announced, prefixed with its Project when that isn't the selected one; Failed at high priority with its reason. Starting/Stopping are only in the rows, so a Start All isn't a stream of speech.
- **Keyboard**: every action has a menu item, so it works from the keyboard (and menu search): Edit / Remove Service in the Service menu; Edit Project, Edit Environment, Favorite, Remove Project in the Project menu. In Ports, ⌘. stops the selected Listener. Disclosure and inline editors are buttons, not tap gestures.
- **Contrast**: WCAG AA in both appearances on the log, column and sidebar surfaces: text colours (State, Service colours, also in their tinted Port pills) 4.5:1, the Stopped ring and other glyphs 3:1, text on the accent 4.5:1. The light variants were darkened for this; dark ones already passed.
- **Reduce Motion**: movement (chevrons turning, rows growing, the pulsing ring) is off; fades stay. **Reduce Transparency**: every surface is solid, as with translucency off.

## Workflow

- One GitHub issue per milestone (M0–M3). Read the issue, `GLOSSARY.md`, this file, and the ADRs before starting.
- Work in chunks of the milestone checklist: branch per chunk (e.g. `m0/process-runner`), PR referencing the milestone issue, merged by the maintainer. Tick the checklist item in the issue when merged.
- Don't expand scope beyond the milestone issue without asking. Resolve new terms into `GLOSSARY.md`; record hard-to-reverse trade-offs as ADRs.
