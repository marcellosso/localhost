First, find and stop the copy the agent started. It is the second process listening for your project, often on the next port up:

```sh
lsof -nP -iTCP -sTCP:LISTEN | grep -Ei 'node|bun|python'
kill <PID>
```

On Windows: `netstat -ano | findstr LISTENING`, then `taskkill /PID <PID> /T /F`. Then, so it doesn't happen again, give the agent a way to run your dev servers that you can see:

```sh
npx @getlocalhost/cli mcp install
```

That connects Claude Code, Cursor, Codex and Claude Desktop to `localhost mcp`, the MCP server of the localhost app, which must be installed.

## Why does Claude Code start a second dev server?

It can't see your terminal. When it needs the app running to test a change, it has no way to know that `pnpm dev` already runs in your other tab, so it starts its own. The second copy then either fails with `EADDRINUSE`, or, with Next.js and Vite, quietly takes port 3001, and the agent tests a server you aren't looking at. Cursor, Codex and other agents do the same, for the same reason.

## Why does it hang on npm run dev?

A dev server never exits. An agent that runs `npm run dev` as an ordinary command waits for it to finish, until its tool call times out. To avoid that, agents run it in the background, where its output goes somewhere you can't read, and where it can keep running after the chat, holding its port and memory (see [Kill leftover dev servers](/guides/kill-leftover-dev-servers)).

## How do I stop Claude Code from starting duplicate dev servers?

Give it something better than a shell. [localhost](/) is a free, open-source desktop app that runs your Projects' Services, and `localhost mcp` is its MCP server: the agent lists your Projects, starts and stops Services, and reads their status, logs and ports through it, while you watch the same Services in the app.

1. [Install the app](/docs/install) and add your project: localhost suggests its Services from `package.json`, Compose files, Python apps and Cargo binaries.
2. Connect the agent. With `npx @getlocalhost/cli mcp install`, or **Settings → Agents → Connect** in the app, or by hand for Claude Code:

   ```sh
   claude mcp add --scope user localhost -- /Applications/localhost.app/Contents/Helpers/localhost mcp
   ```

   That path is macOS's; [Coding agents (MCP)](/docs/mcp#keeping-it-current) lists Windows' and Linux's. `npx @getlocalhost/cli mcp status` shows which agents are connected.
3. Start a new Claude Code session. Connecting also installs the [localhost-mcp skill](/docs/skills), which tells the agent to use these tools instead of a shell for anything long-running.

## What changes once the agent uses localhost?

- **Never twice.** `status` shows what already runs, whoever started it: you in the app, the agent, or a `localhost up` in a terminal. The agent doesn't start a second copy, and `start_service` on a running Service changes nothing.
- **No hanging.** `start_service` waits until the Service is Running, Exited or Failed, up to 45 seconds, and returns. The server keeps running in localhost, not in the agent's shell.
- **Logs both of you can read.** The output goes to the app, and the agent reads it with `logs`, bounded to the lines it asks for.
- **Port conflicts with a name.** A taken port fails with `portConflict`, and `ports` says who holds it, so the agent can tell you, or start on a free port for that Run only.
- **Nothing left behind.** When the agent disconnects, the Services it started stop with their whole process tree, unless the app is open: then they keep running there, until you stop them.

The skill and the server's instructions steer the agent; they don't take its shell away. To make it a project rule, add this to `CLAUDE.md` or `AGENTS.md`:

```md
## Running the project
Use the localhost MCP server to run this project's Services, never a shell:
start_service / start_project to run, status before saying anything is running,
logs and ports when something fails or a port is taken.
```

Every tool, and what an agent may change: [localhost for AI coding agents](/agents).
