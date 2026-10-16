# On native Windows, a Run lives in a kill-on-close Job Object and is stopped with Ctrl-Break

Windows has no process-group signals, and only a process sharing a console can send it Ctrl-C. So each Run's `cmd.exe` is created suspended in a new process group (`CREATE_NEW_PROCESS_GROUP`), assigned to its own Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and without breakaway, then resumed. Everything it spawns is in the job. The job is what the process group is on Unix: membership attributes Listeners to the Run, and the tree has ended once the job has no process left.

Stop sends `CTRL_BREAK_EVENT` to the group, waits ~5s, and then terminates the job. There is no separate SIGTERM step on Windows, so the second wait (~3s) is skipped. Go turns Ctrl-Break into `os.Interrupt`, so `docker compose up` still shuts its containers down cleanly. Node and Python take the default action and exit without running their Ctrl-C handlers (no `KeyboardInterrupt`); uvicorn handles `SIGBREAK` itself. A real Ctrl-C would need a hidden console per Run and a helper process to attach to it and send it. That was rejected for now because it costs a conhost per Run and another binary to ship and keep at the app's version.

Only a process on the same console can send Ctrl-Break, and a GUI app has no console. So the core attaches once to a console without a window. It starts `cmd.exe` with `CREATE_NO_WINDOW`, attaches to that console and ends the `cmd.exe`. Every Run inherits the console. That is one console for the whole app, not one per Run, and it needs no helper. The CLI keeps its terminal's console. Ctrl-Break is only sent while the Run's shell is alive, because a group id whose leader has ended can reach every process on the console. What a shell leaves behind when it exits is terminated with the job.

## Consequences

- When localhost's process ends, crash and force-kill included, the job handles close and Windows kills every Run's tree. Nothing is stopped gracefully then.
- Orphan records are still written (pid and creation time). The launch flow still runs for anything that escaped the job, e.g. a process started through a Windows service or COM. On Windows that is the only way to get an Orphan.
- The log line for a forced stop names the job termination, not a signal. Failed Runs report their exit code. Windows has no terminating signal.
