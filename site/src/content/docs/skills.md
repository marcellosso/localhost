The localhost repository ships an **agent skill**, [`skills/localhost-mcp/SKILL.md`](https://github.com/marcellosso/localhost/blob/main/skills/localhost-mcp/SKILL.md): a short Markdown file that teaches a coding agent to run your Services through [`localhost mcp`](/docs/mcp) instead of a shell. It needs the MCP server connected; the skill only tells the agent how to use it.

## What it tells the agent

- **Use the MCP tools, not a shell, for long-running processes.** No `pnpm dev`, `npm start`, `cargo run` or `docker compose up` of its own. One-shot commands (tests, a build, a migration) are still fine in a shell.
- **Find the Project, then reuse its Services**: `list_projects`, pick the one whose folder holds the code it works on, then `list_services`, naming everything by Key.
- **Check `status` before saying something is running.** Only `running` means running.
- **Read before retrying**: `status` and `logs` when a start fails, `ports` on a `portConflict`, then tell you or start on a free port for that Run.
- **Respect what you started**: a Service you started in the app shows `"startedBy": "app"`, and the agent asks before stopping it. One a `localhost up` runs in a terminal isn't started twice.
- **Don't invent Projects.** The MCP server can't add or edit them, so the agent asks you to add a missing one in the app.

## When an agent uses it

The skill's description tells the agent when to load it: whenever it needs a project's app running (to test a change, call an endpoint, open a page), when a dev server fails or a port is taken, or before saying anything is running. Agents that support skills read that description and load the rest when a task matches. You can also ask for it by name.

The MCP server itself also sends the agent short instructions when it connects, so an agent without the skill gets the essentials. The skill makes them the default and adds the rules above.

## Install it

**Connecting an agent from the app installs it**, and Disconnect removes it (see [Coding agents](/docs/mcp#connect-from-the-app)). It goes where each agent documents its skills for you, in every project:

| Agent | For you, in every project | For one project |
|---|---|---|
| Claude Code | `~/.claude/skills/localhost-mcp/SKILL.md` | `.claude/skills/localhost-mcp/SKILL.md` |
| Cursor | `~/.cursor/skills/localhost-mcp/SKILL.md` | `.cursor/skills/localhost-mcp/SKILL.md` |
| Codex | `~/.agents/skills/localhost-mcp/SKILL.md` | `.agents/skills/localhost-mcp/SKILL.md` |

The app installs the version it comes with, and **Settings → Agents** offers **Update** when the installed one differs. It removes a `localhost-mcp` skill only if it is localhost's (its `name` is `localhost-mcp`).

To install it by hand, copy the `localhost-mcp` folder (with its `SKILL.md`) there. For example, for Claude Code:

```sh
mkdir -p ~/.claude/skills/localhost-mcp
curl -fL -o ~/.claude/skills/localhost-mcp/SKILL.md \
  https://raw.githubusercontent.com/marcellosso/localhost/main/skills/localhost-mcp/SKILL.md
```

A skill in a project folder is shared with everyone who works on it once you commit it. Other agents with their own skill or rules formats can take the same text; or add these lines to your project's `AGENTS.md`:

```md
## Running the project
Use the localhost MCP server to run this project's Services, never a shell:
start_service / start_project to run, status before saying anything is running,
logs and ports when something fails or a port is taken.
```

Why agents should run dev servers through localhost, and what they can do with it: [localhost for AI coding agents](/agents).
