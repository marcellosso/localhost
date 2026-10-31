# @getlocalhost/cli

Connects your coding agents to `localhost mcp`, the MCP server of [localhost app](https://www.getlocalhost.app), from a terminal:

```sh
npx @getlocalhost/cli mcp install
```

It adds a server named `localhost` to Claude Code, Claude Desktop, Cursor and Codex, whichever are on this computer, and installs the [localhost-mcp skill](https://www.getlocalhost.app/docs/skills) where each agent reads skills. The server runs the `localhost` command that comes with the app, so the app must be installed: without it, this prints how to install it for your OS and changes nothing.

It writes what **Settings → Agents** in the app writes, and the app shows these agents as connected:

| Agent | MCP server added to | Skill installed in |
|---|---|---|
| Claude Code | `~/.claude.json`, through `claude mcp add-json --scope user` when `claude` is installed | `~/.claude/skills/localhost-mcp/` |
| Claude Desktop (macOS, Windows) | `claude_desktop_config.json` in Claude's folder (`~/Library/Application Support/Claude`, `%APPDATA%\Claude`) | none: it has no skills folder |
| Cursor | `~/.cursor/mcp.json` | `~/.cursor/skills/localhost-mcp/`, unless Claude Code's or Codex's folder has it: Cursor reads those too |
| Codex | `~/.codex/config.toml` | `~/.agents/skills/localhost-mcp/` |

- **Only its own entries.** Every other server and setting in the file stays as it was, comments and order included. A file it can't read is reported and left alone.
- **A backup first**, in the `agent-backups` folder of localhost's data folder, before it first changes a file.
- **Running it again changes nothing.**

## Commands

```sh
npx @getlocalhost/cli mcp install     # add the server and the skill
npx @getlocalhost/cli mcp uninstall   # remove them, and nothing else
npx @getlocalhost/cli mcp status      # where each agent stands
```

| Option | |
|---|---|
| `--client <agent>` | `claude-code`, `claude-desktop`, `cursor` or `codex`; `claude` for both Claude apps. Repeat it, or separate with commas, for more than one. Without it: every agent found on this computer. |
| `--read-only` | The server runs `localhost mcp --read-only`, without the tools that add, edit or remove Projects and Services. Needs a localhost app that has it. Installing again without it keeps a read-only server read-only. |
| `--dry-run` | Prints what would change, as a diff, and changes nothing. |

Then start a new Claude Code session, quit and reopen Claude Desktop, check **Cursor Settings → MCP**, or restart Codex.

More in the [MCP docs](https://www.getlocalhost.app/docs/mcp). Node 18 or later; no dependencies.

## Development

```sh
cd packages/cli
npm test
node bin/getlocalhost.js mcp status
```

The JSON and TOML editors are the same as the core's (`core/localhost-core/src/agents/`), and their tests too: change both together. `npm pack` copies the skill from `skills/localhost-mcp` into the package.
