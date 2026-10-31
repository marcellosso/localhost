Coding agents tend to start dev servers in a shell you can't see: a `pnpm dev` that holds a port, prints nowhere you can read, and lives on after the chat. `localhost mcp` lets them go through localhost instead. The agent starts and stops your Services, reads their status, logs and ports, adds the Projects and Services it needs, and you see the same Projects and Services in the app.

It is a [Model Context Protocol](https://modelcontextprotocol.io) server on standard input and output, so it works with Claude Code, Cursor, Codex, Claude Desktop and any other MCP client. The app comes with it and connects Claude Code, Cursor and Codex in one click, or from a terminal with `npx @getlocalhost/cli mcp install`.

## Connect from the app

The app comes with the `localhost` command and connects your agents for you, no terminal needed. It opens **Connect your agents** by itself the first time it starts; after that, open **Settings → Agents**, or **Connect Agents…** in the command palette (also **Connect Your Agents** before you have any Projects), and click **Connect** next to Claude Code, Cursor or Codex. **What changes** shows exactly what localhost writes before you click:

| Agent | MCP server added to | Skill installed in |
|---|---|---|
| Claude Code | `~/.claude.json`, as a user-scope server for every project | `~/.claude/skills/localhost-mcp/` |
| Cursor | `~/.cursor/mcp.json` | `~/.cursor/skills/localhost-mcp/`, unless Claude Code's or Codex's folder has it: Cursor reads those too |
| Codex | `~/.codex/config.toml`, shared by the Codex CLI, IDE extension and app | `~/.agents/skills/localhost-mcp/` |

The entry is a server named `localhost` that runs the command inside the app with `mcp`, and the skill is the [localhost-mcp skill](/docs/skills). Then start a new Claude Code session, check **Cursor Settings → MCP**, or restart Codex.

- **Only its own entries.** Every other server and setting in the file stays as it was, comments and order included.
- **Claude Code's own command when there is one.** If `claude` is installed, localhost runs `claude mcp add-json --scope user localhost` and `claude mcp remove --scope user localhost` instead of editing `~/.claude.json`, which Claude Code also writes while it runs. Without it, localhost edits the file.
- **One copy of the skill.** Cursor [also reads](https://cursor.com/docs/skills) `~/.claude/skills` and `~/.agents/skills`, so when Claude Code or Codex is connected, Cursor uses their copy, and gets its own again if they disconnect.
- **A backup first.** Before it first changes a file, localhost keeps a copy in the `agent-backups` folder of its [data folder](/docs/data), and shows where.
- **Nothing until you click**, and connecting again changes nothing.
- **Disconnect** removes the `localhost` entry and the skill, nothing else.

An agent that isn't installed shows as not found. If its file can't be read, or sets up `localhost` in a way localhost doesn't edit, it says so and leaves the file alone: [set it up by hand](#set-it-up-by-hand).

### Keeping it current

The command updates with the app, so agents always run the same version as the app and its background core. Restart to Update asks first when agents are connected ("Stop 2 services and disconnect 1 agent and update?") and disconnects them as it stops your Services: their `localhost mcp` ends, so the update can replace the command (on Windows a running one can't be), and the agent's next session starts the new version. If an agent's `localhost` server runs another command, such as one built with `cargo install` in `~/.cargo/bin`, or one that's gone because the app moved, **Settings → Agents** shows **Update**: it points the server at the app's command and keeps the rest of its entry, such as `env`.

Where the command is:

| | |
|---|---|
| macOS | `/Applications/localhost.app/Contents/Helpers/localhost` |
| Windows | `%LOCALAPPDATA%\localhost\localhost.exe` |
| Linux (.deb, .rpm) | `/usr/bin/localhost` |
| Linux (AppImage) | `~/.local/share/localhost/bin/localhost`, copied out of the AppImage, which only exists while it runs |

On macOS, move localhost to Applications before connecting: opened from the disk image or a quarantined download, it runs from a temporary location, and localhost asks you to move it first.

Agents that run inside a WSL distro, such as Claude Code installed in Ubuntu, keep their configuration there: the app connects the ones on Windows.

## Connect from a terminal

With the app installed, one command connects every agent it finds on this computer:

```sh
npx @getlocalhost/cli mcp install
```

It writes what **Connect** writes, to the same files and with the same rules (only its own entries, Claude Code's `claude mcp` when it is installed, one copy of the skill, a backup first, nothing new the second time), so the app shows those agents as connected. It also connects **Claude Desktop** on macOS and Windows: a `localhost` server in `claude_desktop_config.json`, without the skill, which Claude Desktop has no folder for. Then start a new Claude Code session, quit and reopen Claude Desktop, check **Cursor Settings → MCP**, or restart Codex.

| | |
|---|---|
| `npx @getlocalhost/cli mcp install` | Adds the `localhost` server and the skill. |
| `npx @getlocalhost/cli mcp uninstall` | Removes them, and nothing else. |
| `npx @getlocalhost/cli mcp status` | Says where each agent stands, as **Settings → Agents** does. |
| `--client <agent>` | Only `claude-code`, `claude-desktop`, `cursor` or `codex` (`claude` for both Claude apps). Repeat it for more than one. |
| `--dry-run` | Prints what would change, as a diff, and changes nothing. |

The package doesn't contain the app or the `localhost` command: agents run the one inside the app, [where it is](#keeping-it-current) on each OS. Without the app it changes nothing and says how to install it: Homebrew or the disk image on macOS, the installer on Windows, the packages on Linux (see [Install](/docs/install)). It needs Node 18 or later, and only edits files: no network, apart from `npx` downloading it.

## Use it in a terminal

**Settings → Agents → Use localhost in a terminal → Add to PATH** links `~/.local/bin/localhost` to the app's command on macOS and Linux, and tells you the line to add to your shell profile if `~/.local/bin` isn't on your `PATH`. On Windows it adds the app's folder to your user `PATH`; new terminals find `localhost`. **Remove from PATH** undoes it. The `.deb` and `.rpm` put `localhost` in `/usr/bin` already, and the [Homebrew cask](/docs/install#homebrew-macos) links it into Homebrew's `bin`. Agents don't need it: their configuration names the full path.

## Set it up by hand

For another MCP client, or if you'd rather edit the files yourself, point it at the command above with the argument `mcp`.

**Claude Code**:

```sh
claude mcp add --scope user localhost -- /Applications/localhost.app/Contents/Helpers/localhost mcp
```

**Cursor**: in `~/.cursor/mcp.json`, or `.cursor/mcp.json` in one project:

```json
{
  "mcpServers": {
    "localhost": {
      "type": "stdio",
      "command": "/Applications/localhost.app/Contents/Helpers/localhost",
      "args": ["mcp"]
    }
  }
}
```

On Windows: `"command": "C:\\Users\\you\\AppData\\Local\\localhost\\localhost.exe"`. Open **Cursor Settings → MCP** to check that **localhost** is connected and lists its tools.

**Codex**: `codex mcp add localhost -- /Applications/localhost.app/Contents/Helpers/localhost mcp`, or in `~/.codex/config.toml` (`.codex/config.toml` for one trusted project):

```toml
[mcp_servers.localhost]
command = "/Applications/localhost.app/Contents/Helpers/localhost"
args = ["mcp"]
```

**Other clients**, such as Claude Desktop, take the same `command` and `args` in their own configuration file. Use the full path: apps started from the Dock or a launcher don't always get your shell's `PATH`.

Building localhost yourself? `cargo install --path core/localhost-cli` in a clone of the repository puts `localhost` in `~/.cargo/bin`. It must come from the same version as the app, which otherwise says the two speak to the core differently.

## Tools

Projects and Services are named by their **Key**, as `list_projects` and `list_services` show them. A name works too.

| Tool | What it does |
|---|---|
| `list_projects` | Every Project: Key, name, folder and Service Keys. |
| `list_services` | A Project's Services as configured: command line, working directory, Port, URL, Dependencies. |
| `status` | Each Service's state, pid, Port, URL and uptime; for a Failed one, why, with its last output. |
| `start_service` | Starts one Service after its Dependencies, and waits until it is Running, Exited or Failed (up to 45 seconds). Optionally on another port for this Run. |
| `stop_service` | Stops one Service and waits until its whole process tree has ended. Dependents keep running. |
| `start_project` | Start All: every Service in dependency order, then waits for them the same way. |
| `stop_project` | Stops every Service of the Project, dependents first. |
| `logs` | The latest Run's output of one Service or the whole Project, merged by time: the last `lines` lines (200 by default, at most 2,000), optionally only from the last `since_seconds` or `contains` some text. |
| `ports` | Who listens on which port: one of your Services, a process in a Project's folder, or something unmanaged. With a Project, also whoever holds one of its Ports. |
| `create_project` | Adds a Project: a `name` and the absolute `path` of an existing folder, optionally a `key`, `favorite` and plain variables (`env`). |
| `update_project` | Changes the given fields of a Project: `name`, `key`, `path`, `favorite`, `env`. |
| `delete_project` | Removes a Project from localhost, after stopping its running Services; deletes their logs. Needs `confirm: true`. |
| `create_service` | Adds a Service: `name` and `command_line`, optionally `key`, `working_directory`, `port`, `url`, `port_can_change`, `in_start_all`, `dependencies` (Keys) and `env`. |
| `update_service` | Changes the given fields of a Service; `port` or `url` set to `null` removes them. |
| `delete_service` | Removes a Service, stopping it first if it runs, and drops it from other Services' Dependencies. Needs `confirm: true`. |

Results are JSON. The tools that add or edit return the Project or Service as saved, with its Key and `id`; those that remove return what they removed. Failures name what went wrong with a `kind` (`portConflict`, `commandNotFound`, `notFound`, …) and a message saying what to do next.

## Read-only

Run `localhost mcp --read-only` to leave out the six tools that add, edit and remove Projects and Services: the agent can still list, start, stop and inspect them. Add the option to the server's `args` (`["mcp", "--read-only"]`) in the agent's configuration, or to the `claude mcp add` and `codex mcp add` commands above. The app recognizes a read-only server as connected, and **Update** keeps it read-only.

## How it runs your Services

- **The same way the app does.** Services run in your login shell (`cmd.exe` on Windows) with their Project's variables, Environment Profile and Secrets, start after their Dependencies, and stop SIGINT → SIGTERM → SIGKILL (Ctrl-Break, then ending every process they started, on Windows). Their logs go where the app keeps them.
- **In the app too.** The agent and the app drive the same localhost core, so a Service the agent starts is Running in the app, with its logs, and one you start in the app is running for the agent. A stop from either stops the same Run. `status` says who started each one (`"startedBy": "app"` or `"agent"`).
- **For as long as someone needs it.** When the agent disconnects or quits, the Services it started stop, unless the app is open: then they keep running there, and you stop them when you like. Nothing keeps running unseen. Quitting the app stops every Service after asking, whoever started it.
- **Never twice.** If a `localhost up` in a terminal already runs a Service, `status` shows it running (`"session": "other"`), with its pid and uptime. The agent won't start a second copy, and won't stop it: stop it where it runs.
- **Changes show in the app at once.** What the agent adds, edits or removes is saved where the app keeps your Projects, checked the way the app's editors check it (Keys, ports, URLs, working directories, Dependency cycles), and the open app shows it without a restart: the agent tells the background core, which tells the app to read the Projects again. A change to a running Service applies when it restarts, as one made in the app does. Changes you make in the app reach the agent at its next call.
- **Your files stay yours.** A command is stored as given, never run when it's added. Removing a Project never touches its folder. The agent sets plain variables only: Secrets are set in the app, and the agent's `env` keeps every Secret there is.
- **Removing needs your yes.** `delete_project` and `delete_service` fail unless the agent passes `confirm: true`, which it should do only once you agreed. Without it, nothing is removed and the agent is told to ask.
- **On macOS**, starting a Service that uses a Secret may ask for access to the Keychain: `localhost mcp` is another program than the app, so macOS asks it separately, once for each Secret, as described in [macOS](/docs/macos). It reads only the Secrets of what it starts: the Service, its Dependencies, or for `start_project` the Services that start. Listing, `status`, `logs` and `ports` read none. A Secret you change in the app reaches a running Service when it next starts.

## The background core

The app and `localhost mcp` share one localhost core that runs in the background. Whichever of them needs it first starts it, from its own program: the app, or `localhost daemon`. It listens on a socket in localhost's data folder (`~/Library/Application Support/localhost/localhostd.sock` on macOS, `~/.local/share/localhost/localhostd.sock` on Linux) that only your user can open, and writes its diagnostics to `localhostd.log` next to it. On Windows it listens on a named pipe instead, `\\.\pipe\localhostd-<number>`, that only your user can open, and writes `localhostd.log` to `%APPDATA%\localhost`; it runs without a window, as a second `localhost-app.exe` or `localhost.exe` in Task Manager. About 10 seconds after the last of them disconnects, it stops what is left and exits.

When an agent disconnects, the core stops the Services that only it needed: those it started, and the Dependencies it started them with, unless the app is connected or another agent started them too. When the app quits, it stops everything first, as it always has.

If the app and the `localhost` command come from versions that speak to the core differently, the second one to connect says so. That happens with a command from elsewhere, not the one the app comes with: **Update** the agent in **Settings → Agents**, quit the app and your agents, then open the app again.

On Windows this covers Projects on Windows itself. A Project in a WSL distro runs only from the app, which starts the `localhostd` inside the distro: asked to start one, the agent is told so.

## Teach your agent

The server tells the agent how to use it when it connects. Connecting from the app also installs the [localhost-mcp skill](/docs/skills), which makes it the default. For other agents, add these lines to your project's `AGENTS.md`:

```md
## Running the project
Use the localhost MCP server to run this project's Services, never a shell:
start_service / start_project to run, status before saying anything is running,
logs and ports when something fails or a port is taken.
```

Why this matters, and what agents do with it: [localhost for AI coding agents](/agents).
