---
name: localhost-mcp
description: Run, stop and inspect local dev servers, databases and workers through the localhost MCP server instead of a shell. Use whenever you need a project's app running (to test a change, hit an endpoint, open a page), when a dev server fails or a port is taken, or before saying anything is running.
---

# Running local Services through localhost

localhost knows how each of the developer's Projects runs: its Services (dev server, API, database, worker), their Ports, environment and logs. Its MCP server (`localhost mcp`) lets you start and inspect them, and the developer sees and controls what you start.

## Rules

1. **Use the MCP tools, not a shell, for long-running processes.** Never run `pnpm dev`, `npm start`, `cargo run`, `docker compose up`, `python manage.py runserver` or similar yourself. A one-shot command (tests, a build, a migration) is fine in a shell.
2. **Find the Project, then reuse its Services.** Call `list_projects` and pick the one whose `path` holds the folder you work in, then `list_services`. Name Projects and Services by Key.
3. **Check `status` before saying something is running.** Only `"state": "running"` means running. `starting` is not up yet; `exited` ended on its own; `failed` didn't work.
4. **When a start fails, read before retrying.** `status` gives the failure and the last output; `logs` gives more (use `contains` or `since_seconds` to keep it short). On `portConflict`, call `ports` to see who holds the port, then tell the developer, or start on a free port with `start_service`'s `port`.
5. **Respect what the developer started.** You and the localhost app share the same Services: one the developer started shows `"startedBy": "app"`. Use it as it is, and ask before stopping it. One a `localhost up` in a terminal runs shows `"session": "other"`: don't try to start a second copy, and ask the developer if it needs stopping.

## Typical flow

```text
list_projects                                    → find the Project for this folder
status        { project: "my-saas" }             → what is already running
start_service { project: "my-saas", service: "web" }   (starts its Dependencies too)
status        { project: "my-saas", service: "web" }   → running, url http://localhost:3000
logs          { project: "my-saas", service: "web", lines: 50 }
stop_service  { project: "my-saas", service: "web" }   when you are done
```

## When there is no Project or Service for it

If the folder has no Project, add it with `create_project` (`name`, and the folder's absolute `path`), then each Service it needs with `create_service` (`name`, `command_line`, and a `port` if it listens on one; the command gets it as `$PORT`). Fix a wrong command or Port with `update_service`. Tell the developer what you added: it shows in their localhost app at once. Adding stores the command without running it; start it as usual.

Never remove a Project or Service unless the developer asks: `delete_project` and `delete_service` need `confirm: true`, which you pass only once they agreed. Removing never touches the Project's files.

If these tools aren't there, the server is read-only: tell the developer what to add in the localhost app (it can scan the folder and suggest Services). Only fall back to a shell if they say so.

Services you started stop when your session ends, unless the localhost app is open: then they keep running there. Tell the developer what is still running when you finish.
