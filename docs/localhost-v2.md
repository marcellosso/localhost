# localhost — V2 Product Direction

> **Status:** Future product direction — do not implement as part of V1  
> **Primary theme:** MCP, agent control, richer diagnostics, and reusable local control-plane capabilities

## 1. V2 Summary

V2 extends `localhost` from a local project runner into a **local development control plane**.

The core insight is that once `localhost` already knows:

- which projects exist;
- which services belong to them;
- how each service starts;
- which services are running;
- which ports and URLs they use;
- which environment profile is active;
- what their logs contain;
- why they failed;

then an AI coding agent should not need to guess shell commands.

Instead, agents can ask `localhost` to perform well-defined project operations.

The GUI remains important, but it becomes one of several interfaces over the same core application layer.

---

## 2. Architectural Goal

V1 should leave room for this architecture:

```text
                    ┌── SwiftUI application
                    │
                    ├── Menu bar companion
Application / Core ─┼── CLI
                    │
                    └── MCP server
```

All interfaces should call the same project/service APIs.

The MCP layer should **not** reimplement process management.

The GUI should **not** be required for core actions to function.

---

## 3. MCP Positioning

MCP should make `localhost` the authoritative source of truth for how local projects run.

Instead of an agent doing this:

```text
Inspect repository
Guess package manager
Read README
Guess startup command
Execute shell command
Try to infer whether it worked
```

it should be able to do this:

```text
Ask localhost which services exist
Start required services
Read their status
Inspect logs if one fails
Open or report the relevant URL
```

This is safer, more deterministic, and more reusable across coding agents.

---

## 4. Potential MCP Capabilities

### Project discovery

An agent may be able to:

- list configured projects;
- get a project's metadata;
- list services belonging to a project;
- inspect service dependencies;
- inspect configured ports and URLs.

### Service state

An agent may be able to:

- inspect whether a service is stopped, starting, running, exited, or failed;
- inspect health information;
- inspect exit codes;
- inspect startup failures.

### Lifecycle actions

An agent may be able to:

- start a service;
- stop a service;
- restart a service;
- start an entire project stack;
- stop an entire project stack.

### Logs

An agent may be able to:

- read recent logs for a service;
- read recent combined project logs;
- request error-only or filtered recent output;
- inspect startup output after a failure.

It should not require unbounded historical log access by default.

### Ports and URLs

An agent may be able to:

- discover configured ports;
- discover detected localhost URLs;
- identify port conflicts;
- determine whether a required local endpoint appears available.

### Environment state

An agent may be able to:

- inspect which environment profile is active;
- list names of available environment profiles;
- determine whether required variables exist.

Raw secret values should not be exposed by default.

---

## 5. Agent Use Cases

### Start the development stack

User:

> Start the frontend and API for My SaaS.

Agent:

1. finds `My SaaS`;
2. finds the relevant services;
3. starts required dependencies;
4. starts frontend and API;
5. confirms both reached Running;
6. reports URLs.

### Investigate a broken frontend

User:

> The frontend says connection refused. Check the local stack.

Agent:

1. checks project service state;
2. sees API is not running;
3. inspects API's recent logs;
4. discovers port 8000 is already occupied;
5. reports the cause;
6. offers or performs a permitted corrective action.

### Verify a coding change

A coding agent modifies the API and then:

1. restarts the API;
2. waits for startup;
3. checks exit/health state;
4. reads recent logs;
5. verifies the expected local URL is available.

The coding agent no longer needs project-specific shell knowledge.

---

## 6. Security and Permission Model

MCP introduces a meaningful trust boundary.

V2 should distinguish classes of operations.

### Read-only

Generally low-risk operations:

```text
list_projects
list_services
get_service_status
get_project_status
get_ports
get_urls
read_recent_logs
get_active_environment_profile
list_environment_variable_names
```

### Operational actions

Require intentional permission but are normal product capabilities:

```text
start_service
stop_service
restart_service
start_project
stop_project
```

### Sensitive actions

Should be more restricted and may require explicit confirmation:

```text
change_environment_profile
modify_environment_variables
reveal_secret_value
execute_arbitrary_command
delete_project_configuration
kill_unmanaged_process
```

V2 should avoid turning MCP into an unrestricted shell simply because doing so is technically easy.

---

## 7. Secret Handling

Environment variables and secrets need explicit design.

An agent should usually be able to see:

```text
Environment: Development

DATABASE_URL        present
REDIS_URL           present
STRIPE_SECRET_KEY   present
```

without receiving:

```text
STRIPE_SECRET_KEY=sk_live_...
```

Potential rules:

- secret values are hidden by default;
- MCP can query whether a variable exists;
- MCP can query whether a variable is empty/missing where safe;
- revealing raw secret values requires a stronger permission;
- secret values should never accidentally appear in ordinary tool responses;
- logs may also contain secrets, so future filtering/redaction may be required.

---

## 8. MCP API Design Principles

### Prefer semantic actions

Good:

```text
start_service(project: "my-saas", service: "api")
```

Less desirable:

```text
execute_shell("cd ~/Code/my-saas/api && uv run fastapi dev")
```

The whole value of `localhost` is that it already knows how the service runs.

### Stable identifiers

Projects and services should have stable IDs independent of display names.

### Structured errors

An MCP client should receive useful structured failures.

Example:

```json
{
  "status": "failed",
  "reason": "port_conflict",
  "port": 8000,
  "service": "api"
}
```

rather than merely:

```text
Command failed.
```

### Bounded output

Log operations should use:

- recent line limits;
- time windows;
- search terms;
- severity filters where possible.

Avoid returning unlimited stdout.

### No hidden side effects

Read actions should remain read-only.

Actions that modify state should be explicit.

---

## 9. Richer Health Diagnostics

V2 can go beyond process-running/not-running without turning into full observability software.

Potential checks:

- process alive;
- configured port is accepting connections;
- HTTP endpoint responds;
- dependency service is available;
- repeated crash detection;
- startup timeout;
- obvious port conflict;
- missing executable;
- missing environment variable;
- Docker daemon unavailable;
- Compose service failure.

The product should answer practical local-development questions such as:

> Why isn't my API starting?

Example result:

```text
API failed to start.

Port 8000 is already in use.
Postgres is running.
Redis is running.

Recent output:
EADDRINUSE :8000
```

This is more useful than attempting to become an APM platform.

---

## 10. Optional CLI

A CLI may become useful in V2 once the core application layer is stable.

Possible examples:

```bash
localhost projects
localhost status my-saas
localhost start my-saas
localhost start my-saas api
localhost restart my-saas api
localhost logs my-saas api
localhost stop my-saas
```

The CLI should call the same underlying core/service layer rather than inventing a parallel configuration format.

Whether a CLI requires a background daemon should be decided from actual implementation needs, not assumed upfront.

---

## 11. Possible Daemon / Core Separation

V1 does not need to start with a daemon architecture.

V2 may justify separating a long-running core process if requirements emerge such as:

- services remaining managed while the GUI is closed;
- CLI access without launching the main window;
- MCP access from external agents;
- multiple local clients sharing process state;
- stronger crash recovery.

Possible future structure:

```text
localhost-core
    ├── project state
    ├── service lifecycle
    ├── logs
    ├── environment management
    └── health

        ▲
        │ local IPC
        │

    ┌───┼────────────┐
    │   │            │
SwiftUI CLI         MCP
```

Do not introduce this complexity until a real V2 requirement needs it.

---

## 12. UX for Agent Activity

If agents can control services, the human should always be able to understand what happened.

Potential UI features:

- subtle indicator when an agent started/stopped a service;
- activity history;
- source label such as `You`, `Menu Bar`, `CLI`, or `Agent`;
- notification when an agent action fails;
- visible pending confirmation for sensitive operations.

The app should never feel like processes are changing state mysteriously.

---

## 13. V2 Non-Goals

Even after MCP is added, V2 does not automatically become:

- a cloud orchestration product;
- a team collaboration platform;
- Kubernetes;
- remote production management;
- Docker Desktop;
- a generic autonomous agent runtime;
- a secrets manager;
- a CI/CD platform;
- an unrestricted remote shell.

The focus remains the **developer's local machine and local development environment**.

---

## 14. V2 Success Criteria

V2 succeeds if:

- coding agents stop guessing how configured local projects run;
- agents can safely start and inspect local development stacks;
- a human can always see and override what an agent did;
- raw secrets remain protected by default;
- the MCP API is semantic and stable rather than a wrapper around arbitrary shell execution;
- GUI, menu bar, CLI, and MCP all rely on the same source of truth;
- V1 remains useful for users who never enable agent functionality.

---

## 15. Possible Later Directions

Only pursue these when there is real user demand:

- Linux client;
- Windows client;
- remote execution;
- shared project configuration;
- optional configuration sync;
- collaboration;
- teams/organizations;
- remote development machines.

These are not implied by V2 and should not be allowed to dilute the local-first product without evidence.

---

## 16. Guidance for a Future AI Coding Agent

When V2 implementation begins:

1. Reuse the existing application/core APIs.
2. Do not let MCP directly manage OS processes if the core already owns them.
3. Prefer semantic tools to arbitrary shell execution.
4. Treat environment values and logs as potentially sensitive.
5. Require explicit permission for sensitive actions.
6. Keep output structured and bounded.
7. Make every state-changing action visible to the user.
8. Preserve local-first operation.
9. Do not add cloud infrastructure merely to support MCP.
10. Keep V1 behavior fully usable when MCP is disabled.

---

## V2 Product Statement

> **localhost becomes the trusted local control plane that both developers and their coding agents can use to understand, run, and diagnose local projects without guessing how those projects work.**
