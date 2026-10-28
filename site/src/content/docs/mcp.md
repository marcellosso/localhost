Coding agents tend to start dev servers in a shell you can't see: a `pnpm dev` that holds a port, prints nowhere you can read, and lives on after the chat. `localhost mcp` lets them go through localhost instead. The agent starts and stops your Services, reads their status, logs and ports, and you see the same Projects and Services you configured.

It is a [Model Context Protocol](https://modelcontextprotocol.io) server on standard input and output, so it works with Cursor, Claude Code, Claude Desktop and any other MCP client.

## Install the command

`localhost mcp` is part of the `localhost` command, which isn't in the release downloads yet. Build it from source with Rust installed:

```sh
git clone https://github.com/marcellosso/localhost
cargo install --path localhost/core/localhost-cli
```

That puts `localhost` in `~/.cargo/bin`. Add your Projects in the app first; the command reads the same Projects.

## Connect Cursor

Add the server to `.cursor/mcp.json` in your project, or to `~/.cursor/mcp.json` for every project:

```json
{
  "mcpServers": {
    "localhost": {
      "command": "localhost",
      "args": ["mcp"]
    }
  }
}
```

If Cursor says it can't find `localhost`, use the full path instead, e.g. `"command": "/Users/you/.cargo/bin/localhost"`. Apps started from the Dock don't always get your shell's `PATH`.

Open **Cursor Settings → MCP** to check that **localhost** is connected and lists its tools.

## Other clients

Claude Code:

```sh
claude mcp add localhost -- localhost mcp
```

Claude Desktop and most other clients take the same `command` and `args` as Cursor, in their own configuration file.

## Tools

Projects and Services are named by their **Key**, as `list_projects` and `list_services` show them. A name works too.

| Tool | What it does |
|---|---|
| `list_projects` | Every Project: Key, name, folder and Service Keys. |
| `list_services` | A Project's Services as configured: command line, working directory, Port, URL, Dependencies. |
| `status` | Each Service's state, pid, Port, URL and uptime; for a Failed one, why, with its last output. |
| `start_service` | Starts one Service after its Dependencies, and waits until it is Running, Exited or Failed. Optionally on another port for this Run. |
| `stop_service` | Stops one Service and waits until its whole process tree has ended. Dependents keep running. |
| `start_project` | Start All: every Service in dependency order, then waits for them the same way. |
| `stop_project` | Stops every Service of the Project, dependents first. |
| `logs` | The latest Run's output of one Service or the whole Project, merged by time: the last `lines` lines (200 by default), optionally only from the last `since_seconds` or `contains` some text. |
| `ports` | Who listens on which port: one of your Services, a process in a Project's folder, or something unmanaged. With a Project, also whoever holds one of its Ports. |

Results are JSON. Failures name what went wrong with a `kind` (`portConflict`, `commandNotFound`, `notFound`, …) and a message saying what to do next.

## How it runs your Services

- **The same way the app does.** Services run in your login shell with their Project's variables, Environment Profile and Secrets, start after their Dependencies, and stop SIGINT → SIGTERM → SIGKILL. Their logs go where the app keeps them.
- **For as long as the agent is connected.** The Services an agent starts stop when its client disconnects or quits. Nothing keeps running unseen.
- **Never twice.** If the app or a `localhost up` already runs a Service, `status` shows it running, with its pid and uptime. The agent won't start a second copy, and won't stop it: stop it where it runs. For now the app doesn't show the Services an agent started; `localhost status` and the Ports view do.
- **Configuration stays yours.** The agent can't add or change Projects or Services. Changes you make in the app apply to its next start.
- **On macOS**, the first Service with a Secret that an agent starts may ask for access to the Keychain, as described in [macOS](/docs/macos).

## Teach your agent

The server tells the agent how to use it when it connects. To make it the default, give your agent the skill in the repository, [`skills/localhost-mcp/SKILL.md`](https://github.com/marcellosso/localhost/blob/main/skills/localhost-mcp/SKILL.md), or add these lines to your project's `AGENTS.md`:

```md
## Running the project
Use the localhost MCP server to run this project's Services, never a shell:
start_service / start_project to run, status before saying anything is running,
logs and ports when something fails or a port is taken.
```
