# A Service's Port reaches its command as `$PORT`, and automatic moves last for one Run

When a Service has a Port, every Run gets `PORT=<actual port>` in its environment, and command lines reference it through the login shell (`next dev -p $PORT`). There is no `{port}` placeholder syntax and no per-framework flag knowledge: `$PORT` is already the Node/Rails/Heroku convention, needs no parser, and works identically when the CLI or an agent starts a Service later. When the Port is taken, localhost may start the Service on the next free port, but only for that Run: the Project Configuration keeps the configured Port, so the next start tries it again and the user's configuration is never rewritten behind their back.

Update (cross-platform): on native Windows, commands run through `cmd.exe` (ADR-0010) and reference the variable as `%PORT%`.

## Consequences

- `PORT` set explicitly in Service or Profile variables is overridden by the actual port.
- A Service whose command ignores `$PORT` (e.g. `docker compose up postgres`, port fixed in the compose file) cannot really move; the per-Service "Port can change" toggle exists for those.
- Dependents may reference a Dependency's port in variables or code, so a Service with dependents is never moved automatically (#15).
