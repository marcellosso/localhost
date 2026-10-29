A Service has:

- a **command**: one command line, run in your login shell (`$SHELL -l -c "…"`), so your `PATH`, version managers and profile apply. Pipes, `&&` and `$VARIABLES` work as in a terminal. On Windows itself it runs in `cmd.exe`;
- a **working directory** inside the Project (the Project folder by default);
- an optional **Port**. The command gets the actual port as `$PORT` (`%PORT%` in cmd), e.g. `next dev -p $PORT`, and the Service counts as Running once that port accepts connections. Without a Port it counts as Running after about 1.5 seconds;
- an optional **URL** for Open in Browser (⇧⌘O). Its port follows the actual port;
- **Starts after**: its Dependencies. Starting the Service starts them first and waits until they are Running, or have Exited for one-off steps such as migrations;
- **Manual**: Start All skips it, unless something being started depends on it.

Changes apply the next time a Service starts. A running Service whose configuration changed shows “Restart to apply”; nothing restarts on its own.

## States

A Service is always in one of six states:

| State | Meaning |
|---|---|
| Stopped | Not running: never started, or you stopped it. |
| Starting | Waiting for its Dependencies, or launched and not up yet. |
| Running | Up: its Port accepts connections (or it has been alive ~1.5 s). |
| Stopping | Asked to stop; waiting for every process it started to end. |
| Exited | Ended on its own with exit code 0. |
| Failed | Ended on its own with an error, or couldn't start. The reason and the last lines of output are shown. |

## Dependencies

Starting a Service starts its Dependencies first. **Start All** goes in dependency order, **Stop All** in reverse. Stopping or restarting one Service leaves the Services that depend on it alone. If a Dependency fails, the Services waiting for it don't start, and say why.

## Stopping

Stopping sends Ctrl-C (SIGINT) to the Service's whole process group, then SIGTERM after 5 seconds, then SIGKILL, so nothing is left running in the background. On Windows itself it sends Ctrl-Break, then ends every process the Service started. The log says when a stop had to be forced.

If localhost's background core ever crashes, the next launch finds the Services still running from it and asks whether to stop them.
