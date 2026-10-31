List everything listening on a TCP port, then stop what shouldn't be there. On macOS and Linux:

```sh
lsof -nP -iTCP -sTCP:LISTEN               # every listener, with its PID
ps -o pid,ppid,pgid,command -p <PID>      # what it is, and its process group
kill -- -<PGID>                           # stop the whole group, not just one process
```

On Linux, `ss -ltnp` lists the listeners too. On Windows:

```bat
netstat -ano | findstr LISTENING
tasklist /FI "PID eq <PID>"
taskkill /PID <PID> /T /F                 :: /T takes its child processes with it
```

## Why do dev servers keep running after I close the terminal?

Closing a terminal tab hangs up its shell, and the shell passes that on to the jobs it started. Some don't get the message, or ignore it:

- **Background jobs**: anything started with `&`, `nohup` or `disown`, or by a tool that detaches on purpose.
- **Children of the process you stopped.** `npm run dev` starts `node`, which may start a compiler or a worker. Stop only `npm`, especially with `kill -9`, and the children carry on, adopted by the system.
- **Coding agents.** Claude Code, Cursor and other agents run commands in their own shells. A dev server never exits, so an agent runs it in the background, and it stays there after the chat unless the agent stops it.

The symptoms are the same each time: a port that is [already in use](/guides/port-already-in-use), a laptop fan that won't settle, or an old version of your app answering in the browser.

## How do I find every leftover dev server?

Look for listeners, not process names: a dev server that matters holds a port. `lsof -nP -iTCP -sTCP:LISTEN` on macOS, `ss -ltnp` on Linux and `netstat -ano | findstr LISTENING` on Windows list them all, with PIDs. To see which project one belongs to, check its working directory: `lsof -a -d cwd -p <PID>` on macOS, `ls -l /proc/<PID>/cwd` on Linux.

Matching by name (`pkill -f vite`, `taskkill /IM node.exe /F`) is quick, but it also stops the editor extension, language server or other project that happens to run the same program.

## How do I kill the whole process tree?

On macOS and Linux, a command started from a shell usually gets its own process group, and `kill -- -<PGID>` signals every process in it. `ps -o pgid= -p <PID>` prints the group. Send SIGTERM first; wait a few seconds; only then SIGKILL (`kill -9 -- -<PGID>`).

On Windows, `taskkill /PID <PID> /T` stops the process and everything it started. Add `/F` to force it.

## How does localhost stop dev servers for good?

[localhost](/) runs each of your Services from one window, so the server you want is never in a tab you forgot.

- **Stop ends the whole tree.** localhost sends SIGINT (Ctrl-C) to the Service's process group, SIGTERM after 5 seconds and SIGKILL after 3 more. On Windows it sends Ctrl-Break, then ends the Job Object that holds every process the Service started. The Service shows Stopping until the last one has ended, and the log says when it had to force it.
- **The Ports view finds the rest.** **Ports** (⌘0, or Ctrl+0) lists every listener and whose it is: one of your Services, a process running inside one of your Projects' folders, or nothing localhost knows. A dev server you started in a terminal shows under its Project, by its working directory. Select it and press ⌘. (Ctrl+.) to stop it; localhost asks first for anything it didn't start, and won't stop itself, system services or Docker's backend.
- **Nothing outlives localhost unseen.** Closing the window keeps your Services running, shown in the menu bar or tray. Quitting asks first, then stops all of them. If localhost ever crashes, the next launch finds what was left running and asks whether to stop it.
- **Agents clean up too.** Services a coding agent starts through [`localhost mcp`](/docs/mcp) stop when the agent disconnects, unless the app is open, where you see them.

More on states and stopping: [Services and states](/docs/services).
