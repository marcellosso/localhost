# On native Windows, Service commands run through cmd.exe with a fresh user environment

On Windows, a Service on this machine runs as `cmd.exe /s /c "<command line>"`. The command line is passed to `CreateProcess` verbatim, never re-quoted. `cmd.exe` is the counterpart of `$SHELL -l -c` (ADR-0003): it is on every Windows install, starts fast, supports `&&` (Windows PowerShell 5.1 does not), runs the `.cmd` shims that npm, pnpm and Yarn install, and is the shell npm itself uses for scripts on Windows. Its AutoRun commands are kept (no `/d`), as the login profile is kept on Unix. PowerShell was rejected: pwsh 7 isn't installed by default, Windows PowerShell 5.1 takes about half a second to start and lacks `&&`, and a choice of shell would mean every Suggestion and every `$PORT` reference comes in two syntaxes.

A GUI app's inherited environment is a snapshot from when Explorer (or a terminal) started it, so a toolchain installed since then is missing from `PATH`. The base environment of every Run is therefore built fresh at each start from the registry with `CreateEnvironmentBlock` (user and system variables, `PATH` merged and expanded, as a new logon would see them). Then the usual layers apply on top (`docs/v1-decisions.md`, Running Services).

## Consequences

- Command lines reference the Port as `%PORT%` on Windows (ADR-0006 still holds: it is the `PORT` variable, in the shell's syntax). Detection emits `%PORT%` there.
- Stdin is `NUL`, so cmd's "Terminate batch job (Y/N)?" prompt reads end-of-file instead of waiting for an answer.
- Version managers that only work through a PowerShell profile hook (fnm and mise in `activate` mode) aren't seen. Their shim modes and nvm-windows and Volta (which edit `PATH`) work.
- A Project file exported on macOS or Linux keeps `$PORT`. Projects aren't expected to move between operating systems (their paths don't either).
