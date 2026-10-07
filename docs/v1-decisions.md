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
- **Starting → Running**: no port configured → alive after a ~1.5s grace period; port configured (M1) → port accepts connections (timeout → Running with a warning). Dying during Starting → **Failed** with output visible.
- **Stop escalation**: SIGINT to the process group → wait ~5s → SIGTERM → wait ~3s → SIGKILL. State stays **Stopping** until the whole tree is gone; log a line if force-killed. Global defaults for now.
- **Orphans**: each active Run's (pid, pgid, process start time) is recorded on disk. On launch, survivors are detected (start time guards against PID reuse) and the user is asked "N services from the last session are still running — Stop them / Leave them" (default Stop). Never re-adopt them as Running.
- **App lifetime**: closing the main window does not quit; Services keep running. ⌘Q / menu-bar Quit asks for confirmation if anything is running, then stops everything in parallel. Dock icon only while the main window is open (`.regular` ↔ `.accessory`), once the menu bar exists.

## Dependencies between Services (M1)

- Start All starts in dependency order and waits for each Dependency to reach Running.
- Starting one Service also starts its not-yet-running Dependencies.
- Stopping one Service does **not** stop its dependents. Stop All stops in reverse dependency order.
- If a Dependency fails, dependents don't start and show the reason. Cycles are rejected at edit time.

## Docker Compose (M1)

No adapter in V1. A Compose service is just a command line run in the foreground (`docker compose up postgres`); Stop's SIGINT makes Compose shut down cleanly. Detection suggests the foreground form (the `-d` form would just end as Exited). Only extra: recognise "Docker daemon not running" and surface it as a typed error.

## UI direction

MonoCode-like: dense, calm, dark-friendly, restrained native materials (sidebar most translucent, logs fully opaque). **No Liquid Glass.**

## Workflow

- One GitHub issue per milestone (M0–M3). Read the issue, `GLOSSARY.md`, this file, and the ADRs before starting.
- Work in chunks of the milestone checklist: branch per chunk (e.g. `m0/process-runner`), PR referencing the milestone issue, merged by the maintainer. Tick the checklist item in the issue when merged.
- Don't expand scope beyond the milestone issue without asking. Resolve new terms into `GLOSSARY.md`; record hard-to-reverse trade-offs as ADRs.
