# localhost — V1 Product Specification

> **Status:** Implementation specification for V1  
> **Platform:** macOS  
> **Primary technology:** Swift + SwiftUI  
> **Product type:** Local-first desktop developer tool

## 1. Product Summary

**localhost** is a native macOS application that remembers how each local development project runs and gives the developer one place to start, stop, inspect, and return to it.

Developers often work across several projects at once. Each project may require a different collection of long-running commands: a frontend development server, a Python API, a worker, Docker Compose services, databases, queues, or local utilities.

The usual result is a desktop full of terminal tabs whose main purpose is simply to keep processes alive.

`localhost` replaces that repetitive setup with a project-oriented interface.

A project is configured once. From then on, the developer opens `localhost`, selects the project, and starts the services they need. The app shows what is running, where it is available, and what it is doing.

### Core promise

> **Open any project and immediately know how to run it.**

### Example

Instead of manually opening terminals and remembering:

```bash
pnpm dev
uv run fastapi dev
docker compose up
bun run worker
```

the developer sees a single project containing those services and can start or stop them individually or together.

---

## 2. Product Positioning

`localhost` is **not**:

- a replacement for Docker Desktop;
- a container-management platform;
- a Kubernetes tool;
- a production infrastructure dashboard;
- a remote server orchestrator;
- an IDE;
- a terminal replacement;
- a cloud platform.

Docker is simply one way a local service may run.

Node, Python, Bun, npm, pnpm, Yarn, Docker Compose, shell commands, and future runtimes should all feel equally natural.

The product should be thought of as:

> **The executable memory of the developer's local projects.**

---

## 3. Product Principles

### 3.1 Project-first, not process-first

Users think in terms of:

- "My SaaS"
- "Game Server"
- "Client Website"

They do not primarily think in terms of PIDs or shell sessions.

Processes are implementation details beneath a project and its services.

### 3.2 Local-first

For V1:

- configuration lives locally;
- logs live locally;
- project metadata lives locally;
- process state is managed locally;
- no account is required;
- no cloud backend is required;
- no organizations or teams are required.

The app should be useful completely offline.

### 3.3 Native and immediate

The application should feel like a first-class macOS app:

- fast launch;
- responsive state changes;
- native controls where appropriate;
- native keyboard behavior;
- native menus and window behavior;
- good accessibility;
- minimal visual noise.

### 3.4 Commands are universal

The core service abstraction should remain simple:

```text
working directory + command + arguments + environment
```

Runtime-specific integrations should enhance this model rather than replace it.

### 3.5 Useful before clever

V1 should solve the terminal-clutter and forgotten-command problem extremely well before adding AI, remote orchestration, collaboration, or deep observability.

### 3.6 Beautiful is functional

Visual quality is part of the product.

Status, hierarchy, navigation, logs, errors, and transitions should make the system understandable at a glance.

---

## 4. V1 Scope

V1 is a **macOS-native local project runner and service manager**.

It should allow a developer to:

1. add a project from a local directory;
2. define one or more services for that project;
3. start, stop, and restart services;
4. start or stop all services in a project;
5. see service state;
6. see live stdout/stderr;
7. see ports and local URLs;
8. open URLs in the browser;
9. work with ordinary shell commands and Docker Compose;
10. preserve enough project configuration that the developer does not need to remember startup instructions;
11. control common actions from a macOS menu bar companion.

---

## 5. Core User Flows

### 5.1 First-time project setup

Target flow:

```text
Add Project
    ↓
Choose local folder
    ↓
Scan for common project files
    ↓
Suggest services and commands
    ↓
Review / edit configuration
    ↓
Save
    ↓
Start All
```

Detection should provide **editable suggestions**, never opaque magic.

Useful project detection includes:

- `package.json` scripts;
- npm / Yarn / pnpm / Bun lockfiles;
- `docker-compose.yml`;
- `docker-compose.yaml`;
- `compose.yml`;
- `compose.yaml`;
- `pyproject.toml`;
- common Python project files;
- `Cargo.toml`;
- monorepo/workspace structures;
- common framework ports where practical.

### 5.2 Daily use

1. Open `localhost`.
2. Select a project from the sidebar.
3. Press **Start All**, or start only the required services.
4. Immediately see running state, ports, URLs, and logs.
5. Work on the project.
6. Stop the services when finished.

### 5.3 Returning to an old project

The app should eliminate:

> "How did I run this again?"

The saved project should effectively become the executable version of the project's README startup instructions.

---

## 6. Information Model

### Project

A project represents one local development project.

Suggested fields:

```swift
Project
- id
- name
- path
- icon
- services
- sortOrder
- isFavorite
```

### Service

A service represents one independently manageable long-running command.

Conceptually:

```swift
Service
- id
- name
- workingDirectory
- command
- arguments
- environment
- port
- url
- dependencies
- status
```

The important rule is that a service is **not inherently a Node service, Python service, or Docker service**.

Those are optional runtime/integration layers.

Examples:

```text
Frontend
cwd: ~/Code/app/web
command: pnpm dev
port: 3000
```

```text
API
cwd: ~/Code/app/api
command: uv run fastapi dev
port: 8000
```

```text
Database
cwd: ~/Code/app
command: docker compose up postgres
port: 5432
```

---

## 7. Feature Requirements

### 7.1 Project Library

V1 should support:

- add local projects;
- remove projects;
- edit project name;
- project icon;
- project path;
- pin/favorite projects;
- reorder projects;
- remember recent state where useful;
- **Start All**;
- **Stop All**.

### 7.2 Service Management

Each service should support:

- start;
- stop;
- restart;
- working directory;
- command;
- arguments;
- environment variables;
- optional environment profiles;
- optional dependencies;
- optional startup order;
- optional configured port;
- optional URL.

Required service states should include at least:

```text
Stopped
Starting
Running
Stopping
Exited
Failed
```

State should never be ambiguous to the user.

### 7.3 Logs

The app should provide:

- live stdout;
- live stderr;
- per-service log streams;
- combined project log stream;
- search/filter;
- selection;
- copy;
- clear;
- bounded recent log history stored locally.

Logs should remain performant for long-running development processes.

The log viewer should feel like a high-quality terminal surface, but it does not need to become a full terminal emulator.

### 7.4 Ports and URLs

Support:

- manually configured port;
- local URL associated with a service;
- detect common localhost URLs from process output where practical;
- click to open URL in the default browser;
- warn if a configured port is already occupied;
- where feasible, identify the local process/service that owns a conflicting port.

### 7.5 Project Detection

When adding a project, detection may suggest services based on:

- `package.json`;
- package-manager lockfiles;
- Compose files;
- Python project files;
- monorepo/workspace files;
- known development scripts.

Generated suggestions must always be editable before saving.

### 7.6 Developer Ergonomics

Support early:

- command palette;
- keyboard shortcuts;
- open folder in Finder;
- open project in configured editor;
- open Terminal at project directory;
- open Terminal at service directory;
- contextual actions from project/service rows.

---

## 8. Menu Bar Companion

The menu bar companion is an **early V1 feature**.

It should be a lightweight quick-control surface, not a miniature duplicate of the full application.

### Goals

Without bringing the main window forward, the user should be able to:

- see which projects are running;
- see which services are running;
- start a service;
- stop a service;
- restart a service;
- Start All for a project;
- Stop All for a project;
- open a detected localhost URL;
- see a subtle crash/failure indication;
- jump directly to a project/service in the main app.

### Do not put in the menu bar

Keep these in the main app:

- full configuration;
- environment editing;
- full log viewer;
- project onboarding;
- advanced diagnostics.

### Example

```text
localhost                       4 ●

MY SAAS
● Web                      :3000 ↗
● API                      :8000 ↗
● Postgres
                         [Stop All]

GAME SERVER
○ Server
                        [Start All]

──────────────────────────────
Open localhost
Quit
```

The menu bar and main application must share the same underlying application state and service-management layer.

---

## 9. UI / UX Direction

The interface should feel closer to a **premium developer workspace** than an infrastructure dashboard.

### Inspirations

#### MonoCode

Primary visual and interaction inspiration:

- dense but calm developer UI;
- clear sidebar hierarchy;
- panes;
- compact controls;
- keyboard navigation;
- sophisticated dark UI;
- subtle macOS-native feeling;
- restrained translucency.

#### T3 Code and modern AI orchestrators

Useful inspiration for:

- project/session organization;
- fast command-driven interactions;
- making complex background work understandable;
- compact status presentation.

#### macOS system apps

Follow native macOS conventions where they improve usability:

- sidebars;
- toolbar placement;
- sheets;
- menus;
- focus states;
- keyboard shortcuts;
- context menus;
- window behavior;
- accessibility.

#### Terminal tools

Preserve the speed, clarity, and transparency developers expect from terminal workflows while removing the need to manually organize terminal tabs.

---

## 10. Visual Language

### 10.1 Overall aesthetic

The application should be:

- minimal;
- dark-mode-friendly;
- compact;
- calm;
- native-looking;
- low-noise;
- highly readable.

Avoid turning the app into a traditional DevOps dashboard.

### 10.2 Materials and translucency

Use subtle native macOS translucency/material effects.

The goal is **not heavy glassmorphism**.

The effect should be restrained enough that the user perceives depth without the background interfering with content.

Recommended visual hierarchy:

```text
Window background
    subtle native material

Sidebar
    strongest translucency

Main content
    more opaque

Logs / inputs / dense text surfaces
    nearly or completely opaque
```

The MonoCode reference is a good target: the desktop background may be faintly perceptible, but content still feels stable and readable.

Where possible, use native SwiftUI/macOS materials instead of simulated CSS-like blur.

Users may eventually be given appearance controls such as:

```text
Theme
- System
- Dark
- Light

Window material
- Subtle
- Solid
- More transparent
```

The default should be carefully tuned and should not require customization.

### 10.3 Color

Decorative color should be restrained.

Color should primarily communicate meaningful state:

```text
● Running
○ Stopped
◐ Starting
! Failed
```

Project icons, status, failures, and actions may carry color. Most surrounding chrome should remain neutral.

### 10.4 Logs

Log surfaces should be more opaque than sidebars and general UI panels.

Thousands of moving monospace characters over a translucent background would reduce readability.

### 10.5 Motion

Use subtle animation for:

- state transitions;
- start/stop transitions;
- panels opening/closing;
- errors appearing;
- selections.

Do not animate simply for decoration.

### 10.6 Keyboard interaction

The app should be excellent with a mouse, but experienced users should be able to operate most frequent actions from the keyboard.

---

## 11. Proposed Main Layout

Primary mental model:

```text
Sidebar → Project Overview → Service Details
```

Example:

```text
PROJECTS                 MY SAAS
● My SaaS               ┌─────────────────────────────────────────┐
○ Game Tools            │ ● Frontend   :3000   Open   Restart   │
○ Scraper               │ ● API        :8000   Open   Restart   │
                        │ ● Postgres    :5432   Docker           │
                        └─────────────────────────────────────────┘

                        LOGS
                        api > listening on localhost:8000
                        web > ready in 412ms
```

The sidebar should make it possible to understand the current state of all projects quickly.

The project view should prioritize:

1. overall project state;
2. Start All / Stop All;
3. service state;
4. ports/URLs;
5. logs;
6. deeper configuration.

---

## 12. Technical Direction

### 12.1 Platform

V1 is macOS-only.

Recommended stack:

- Swift;
- SwiftUI;
- native macOS APIs where needed.

Do not spend V1 engineering time creating premature cross-platform abstractions.

### 12.2 Architectural rule

**SwiftUI views must not own process execution.**

Application/core logic should be separated from presentation so the same service-management layer can later be used by other interfaces.

Suggested architecture:

```text
SwiftUI UI
    │
    ▼
Application / Core
    ├── ProjectManager
    ├── ServiceManager
    ├── ProcessController
    ├── LogStore
    ├── EnvironmentManager
    └── HealthMonitor
          │
          ├── Local process runner
          └── Docker Compose adapter
```

Both the main window and menu bar should use this same layer.

### 12.3 Generic command execution

The generic local process runner should be the foundation.

Runtime-specific integrations can be layered above it.

Possible later adapters:

```text
Generic Command
├── Node / package manager
├── Python
├── Docker Compose
├── Bun
└── other runtimes
```

Adapters may provide:

- discovery;
- validation;
- richer status;
- convenience actions.

They should not make the core model runtime-specific.

---

## 13. Process Lifecycle Requirements

Process ownership is a critical engineering concern.

Starting a command is easy. Reliably stopping everything it spawned is not.

V1 must explicitly account for:

- child processes;
- process groups;
- graceful signals;
- termination escalation;
- application shutdown;
- service crashes;
- orphan prevention;
- exit codes.

Example problem:

```text
pnpm dev
    └── node
         └── framework dev server
```

Stopping only the shell or initial command is insufficient if children remain alive.

The process-management layer should reliably own and terminate the full process tree associated with a service.

---

## 14. Reliability Requirements

V1 should:

- launch processes predictably;
- stop full child process trees;
- surface exit codes;
- surface failures clearly;
- avoid silently hiding stdout/stderr;
- restore relevant UI state after restarting the app;
- never show a service as Running when the owned process has exited;
- handle failed starts without leaving the UI in an indeterminate state;
- avoid orphaning processes where technically possible.

---

## 15. Environment Variables

V1 may support environment configuration and profiles.

Examples:

```text
Development
Testing
Local Staging
```

Environment values remain local.

No cloud synchronization or organization-level environment management is required.

Secret handling should be designed carefully enough that a future automation/agent layer can be added without exposing raw secrets by default, but no agent functionality is required in V1.

---

## 16. Explicit Non-Goals for V1

Do **not** implement these as part of the initial product unless a foundational dependency requires minor preparatory architecture:

- Docker Desktop replacement;
- Docker image administration;
- Docker volume administration;
- Docker network administration;
- Kubernetes;
- production infrastructure;
- remote servers;
- SSH orchestration;
- cloud accounts;
- organizations;
- teams;
- collaboration;
- shared cloud configuration;
- full system observability/APM;
- IDE functionality;
- terminal emulator replacement;
- MCP;
- AI agents;
- arbitrary agent command execution;
- Linux support;
- Windows support;
- cross-platform parity;
- perfect support for every runtime.

V1 should remain focused.

---

## 17. Suggested Implementation Roadmap

### Milestone 0 — Dogfood

Build the smallest version that can replace personal terminal clutter.

Required:

- project persistence;
- generic local command runner;
- reliable start/stop;
- live logs;
- basic sidebar;
- basic project view.

### Milestone 1 — Useful V1

Add:

- multiple services;
- Start All / Stop All;
- ports and URLs;
- environment configuration;
- Docker Compose commands;
- polished logs;
- clear error states;
- Finder/editor/Terminal actions;
- menu bar companion.

### Milestone 2 — Great Onboarding

Add:

- project scanning;
- command suggestions;
- package-manager detection;
- Compose discovery;
- Python project detection;
- monorepo support;
- command palette;
- stronger keyboard navigation.

### Milestone 3 — Public Open-Source Release

Prepare:

- documentation;
- configuration import/export;
- stability hardening;
- accessibility;
- crash recovery;
- contribution guide;
- signed macOS build;
- notarized macOS release.

---

## 18. V1 Success Criteria

The first release does not need a huge feature list.

It succeeds if it changes the developer's default behavior.

Success means:

- developers stop keeping terminal windows open purely to host long-running development commands;
- returning to a project after weeks or months requires no memory of its startup sequence;
- users can understand what is running and where it is available within seconds;
- adding a normal project takes roughly the effort of selecting a folder and accepting/editing a few suggestions;
- opening `localhost` feels faster and easier than opening and arranging terminals;
- the app remains fully useful without AI or MCP.

---

## 19. Example Project Definition

The exact persistence format can evolve. The mental model should remain simple.

```yaml
name: My SaaS
path: ~/Code/my-saas

services:
  web:
    command: pnpm dev
    cwd: ./web
    port: 3000

  api:
    command: uv run fastapi dev
    cwd: ./api
    port: 8000
    depends_on:
      - database

  database:
    command: docker compose up postgres
    cwd: .
    port: 5432
```

---

## 20. Implementation Guidance for an AI Coding Agent

When implementing V1:

1. **Do not expand the product scope without an explicit request.**
2. Prefer simple, native Swift/SwiftUI solutions over unnecessary dependencies.
3. Keep process management and project state outside SwiftUI views.
4. Treat the generic command runner as the foundation.
5. Do not deeply couple the data model to Docker, Node, or Python.
6. Design reusable core APIs so the menu bar and main window share the same state.
7. Prioritize correct process lifecycle behavior over visual polish when the two conflict.
8. Once lifecycle behavior is reliable, prioritize UI polish heavily.
9. Keep all user data local.
10. Do not add accounts, telemetry, cloud services, organizations, MCP, or AI behavior unless explicitly requested.
11. Keep interfaces small and composable so future control surfaces can call the same application layer.
12. Favor macOS conventions and native SwiftUI behavior unless there is a clear UX reason not to.

---

## Product Statement

> **localhost gives developers one beautiful, native place to run the projects on their machine. Configure a project once, then start working — no terminal archaeology required.**
