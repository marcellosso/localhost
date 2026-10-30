localhost remembers how each of your local development projects runs, and lets you start, stop and watch all of its Services from one place, instead of a row of terminal tabs.

A **Project** is a folder. A **Service** is one long-running command in it: `pnpm dev`, `uv run fastapi dev`, `docker compose up postgres`, `cargo run`. localhost doesn't care what the command is: it runs it in your shell, keeps its output, knows when it is up, and stops its whole process tree when you ask.

![The localhost window: the Acme Shop Project with its Services (Migrations exited, API, Web and Worker running, Storybook manual) and their merged log](../../../../.github/assets/main-window-dark.png)

## What it does

- **One place for every project**: Start All / Stop All, Dependencies between Services (the API waits for the database), one merged log of every Service.
- **Ports that make sense**: a Service counts as Running once its Port accepts connections. If the Port is taken, localhost says by whom and can start the Service on the next free one. The **Ports** view lists everything listening on your machine, whoever started it.
- **Environment without `.env` juggling**: variables on the Project, on each Service and in Environment Profiles (Development, Testing, …). Any variable can be a Secret, kept in your system's secret store.
- **Set up in seconds**: point it at a folder and it suggests Services from `package.json` scripts, Compose files, Python apps and Cargo binaries. You review every suggestion; nothing is applied silently.
- **Keyboard first**: a command palette, a shortcut for every action, arrow keys across the window. Screen readers, light and dark, reduced motion.
- **Coding agents included**: Claude Code, Cursor, Codex and other MCP clients start, stop and inspect your Services through [`localhost mcp`](/docs/mcp) instead of a hidden shell. The agent and the app share one background core, so what the agent starts shows in the app, with its logs, and the reverse. See [localhost for AI coding agents](/agents).
- **Local**: no account, no cloud, works offline. Configuration lives in localhost's own storage, never in your project folders. Checking for [updates](/docs/updates) is the only thing it asks the network for, and you can turn it off.

A native SwiftUI app on macOS with a menu bar companion; one app for Linux and Windows with a tray icon. On Windows, Projects run on Windows itself or inside a WSL distro. The app updates itself, after asking, everywhere but the `.deb` and `.rpm` packages, which your package manager updates.

## The words it uses

| Word | Meaning |
|---|---|
| **Project** | One local project, rooted at a folder, grouping the Services you need to work on it. |
| **Service** | One long-running command of a Project: working directory, command line, environment. |
| **Key** | A Project's or Service's short, stable handle (`my-saas`, `api`), used in logs and by the command line. |
| **Port** | The one TCP port a Service is expected to listen on. It decides when the Service is Running. |
| **Dependency** | A Service that must be Running, or have Exited, before another one starts. |
| **Environment Profile** | A named set of variables (Development, Testing) for a whole Project; at most one is active. |
| **Secret** | A variable whose value lives in your system's secret store. |
| **Run** | One execution of a Service, from launch until every process it started has ended. |
| **Execution Target** | Where a Project's Services run: this machine, or a WSL distro. |

Next: [install localhost](/docs/install), then [add your first Project](/docs/quick-start).
