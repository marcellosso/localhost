# V1 engineering decisions

Settled decisions for building V1. `localhost-v1.md` / `localhost-v2.md` are product direction; where they disagree with this file, **this file wins**. Vocabulary is defined in `GLOSSARY.md`; the hard-to-reverse decisions and their reasoning are in `docs/adr/`.

## Architecture

- **UI-free core** (ADR-0001). `Packages/LocalhostCore` holds all project state, Run lifecycle, logs, environment, persistence. It never imports SwiftUI or AppKit. The app target is only a client. In V1 the core runs in-process; V2 may host it in a daemon behind CLI/MCP, so keep the API shaped for that:
  - async API; state exposed as observable snapshots + an event stream (`serviceStateChanged`, `runStarted`, `runEnded`, …);
  - every state-changing call takes an **Origin** (`.mainWindow`, `.menuBar`; later `.cli`, `.agent`);
  - failures are a typed error enum (`portConflict(port:owner:)`, `commandNotFound`, `workingDirectoryMissing`, `dependencyFailed(…)`, `dockerDaemonUnavailable`, …), never bare strings;
  - all core types `Sendable`/`Codable`-friendly.
- **Identity**: Projects and Services have a UUID identity plus a **Key** (slug). Project Keys unique app-wide, Service Keys unique per Project; editable but validated. Internal references (Dependencies, log files) use UUIDs.
- **Layout**: `localhost.xcodeproj` + `App/` (SwiftUI target) + `Packages/LocalhostCore/{Sources,Tests}`. No Tuist/XcodeGen.
- **Toolchain**: Swift 6 language mode, strict concurrency, macOS 15 minimum, bundle id `com.marcellosso.localhost`. Non-sandboxed, Developer ID (ADR-0002).
- **Dependencies**: none by default; each third-party package must be justified (an MCP SDK in V2 is expected).
- **Tests**: Swift Testing (`@Test`). The core is test-driven against **real processes** (fixture scripts that fork grandchildren, trap signals, exit with codes). No UI tests in V1.

## Persistence

- Project Configuration is stored by localhost, never in the project folder (ADR-0004): `~/Library/Application Support/localhost/projects/<project-uuid>.json`, one Codable JSON file per Project, atomic writes, `schemaVersion` field.
- Service working directories are stored **relative to the Project path**.
- Secret values live in the Keychain, not in JSON, and are never exported (M1).

## Running Services

- **Command**: one command line string, run as `$SHELL -l -c "<command line>"` in a new process group / session (ADR-0003). No separate arguments field.
- **Environment of a Run** (later layers override earlier): login-shell env → Project variables → Service variables → active Environment Profile. Plus output hints `FORCE_COLOR=1`, `CLICOLOR_FORCE=1`, `PYTHONUNBUFFERED=1`.
- **Output**: stdout and stderr via separate pipes (ADR-0005). Viewer renders ANSI colour; stderr lines get a subtle marker.
- **Logs**: in-memory ring buffer per Run for the viewer, plus each Run's output written to `~/Library/Application Support/localhost/logs/…`. Keep last 5 Runs per Service, 10 MB cap each (rotate). The combined project stream is a merged read-time view.
- **Service States**: Stopped / Starting / Running / Stopping / Exited / Failed, exactly as defined in `GLOSSARY.md`. A requested stop always ends in **Stopped** regardless of exit code. A Service has at most one active Run.
- **Starting → Running**: no Port configured → alive after a ~1.5s grace period; Port configured (M1) → the Port accepts connections (global 30s timeout → Running with a warning that clears once the Port opens). Dying during Starting → **Failed** with output visible; exiting with code 0 during Starting → **Exited** (one-shot commands such as migrations).
- **Configuration changes** (variables, active Profile, command line, …) apply on the next start. A Run whose launch configuration differs from what would be built now shows "Restart to apply" (computed in the core). Nothing restarts automatically.
- **Failed** shows the typed reason (or exit code/signal) plus the last 20 lines of both streams. A Failed Run whose output contains `Cannot connect to the Docker daemon` (or, from current Docker CLIs, `failed to connect to the docker API`) reports `dockerDaemonUnavailable` instead (diagnosed after the fact, never a pre-check).
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
- Shortcuts: ⌘R start/restart, ⌘. stop, ⇧⌘R Start All, ⇧⌘. Stop All, ⌘F find, ⌃L clear logs, ⇧⌘O open URL, ⌥⌘T Terminal, ⌥⌘E editor, ⌥⌘F Finder, ⌘1–9 Projects, ⌘0 Ports, ⌥⌘1–9 Services, ⌘[ / ⌘] previous/next Service. Holding ⌘ (or ⌥⌘) ~0.4s shows number badges. **⌘K is reserved** for the M2 command palette.

## UI direction

"Graded glass" (#26; mockups in `docs/design/ui-refresh/`): dense, calm, dark and light. **No Liquid Glass.**

- **Glass steps down from left to right**: the sidebar is most translucent, the services column (and Ports) is lighter glass, the log is always opaque. Under Reduce Transparency every surface is opaque.
- **Colour has a job**: one accent (pink) for the primary action and selection only; green / orange / red / grey only for Service State; each Service has a stable colour by its position in the Project, used wherever that Service appears (row, log prefix, menu bar, Ports).
- **State glyphs carry state by shape**: a dot with a halo (Running), a pulsing ring (Starting/Stopping, still under Reduce Motion), a diamond (Failed), a hollow ring (Stopped/Exited).
- **Monospace is a voice**: Keys, ports, commands and paths are always SF Mono; everything else SF Pro.
- **Primary actions are labelled**; icon-only controls name their shortcut in the tooltip; rare actions live in `⋯` and context menus.
- Colours and surfaces live in `Theme` (App target); views don't hard-code colours.

## Workflow

- One GitHub issue per milestone (M0–M3). Read the issue, `GLOSSARY.md`, this file, and the ADRs before starting.
- Work in chunks of the milestone checklist: branch per chunk (e.g. `m0/process-runner`), PR referencing the milestone issue, merged by the maintainer. Tick the checklist item in the issue when merged.
- Don't expand scope beyond the milestone issue without asking. Resolve new terms into `GLOSSARY.md`; record hard-to-reverse trade-offs as ADRs.
