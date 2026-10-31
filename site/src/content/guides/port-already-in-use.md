Find the process that holds the port, then stop it. On macOS and Linux:

```sh
lsof -nP -iTCP:3000 -sTCP:LISTEN   # the PID is the second column
kill <PID>                         # kill -9 <PID> only if it won't stop
```

On Windows, in Command Prompt or PowerShell:

```bat
netstat -ano | findstr :3000       :: the PID is the last column
taskkill /PID <PID> /T /F          :: /T stops its child processes too
```

Or, anywhere Node is installed: `npx kill-port 3000`.

## What does “address already in use” mean?

Only one process can listen on a TCP port at a time. When your dev server starts and something already listens on its port, the operating system refuses, and the server stops with an error such as:

- Node.js: `Error: listen EADDRINUSE: address already in use :::3000`
- Python: `OSError: [Errno 48] Address already in use` (98 on Linux, `WinError 10048` on Windows)
- Rails: `Address already in use - bind(2) for "127.0.0.1" port 3000 (Errno::EADDRINUSE)`

Next.js and Vite don't stop: they print `Port 3000 is in use, trying 3001 instead.` and move on. The app runs, but on a port your browser tab, your proxy or your other Service doesn't expect.

## How do I find what is using a port?

**macOS.** `lsof -nP -iTCP:3000 -sTCP:LISTEN` prints the command and PID of whatever listens on port 3000. Without `-sTCP:LISTEN` it also lists clients connected to it, such as your browser. Ports 5000 and 7000 are often taken by AirPlay Receiver, part of macOS: turn it off in System Settings → General → AirDrop & Handoff, or use another port.

**Linux.** `ss -ltnp 'sport = :3000'` shows the listener and, in `users:(("node",pid=1234,…))`, its PID. Add `sudo` to see processes of other users. `fuser -k -TERM 3000/tcp` finds and stops it in one step; without `-TERM`, `fuser -k` sends SIGKILL.

**Windows.** `netstat -ano | findstr :3000` lists connections on the port; the listener is the line that says `LISTENING`, and its PID is the last column. `tasklist /FI "PID eq 1234"` names it. In PowerShell, `Get-NetTCPConnection -LocalPort 3000 -State Listen` gives the `OwningProcess`, and `Stop-Process -Id 1234` stops it. A dev server running inside WSL lives in the distro: run the Linux commands there.

## Should I use kill -9?

Try a plain `kill <PID>` first. It sends SIGTERM, which lets the server close its connections and clean up. `kill -9` (SIGKILL) can't be refused, but nothing runs on the way out, and a parent such as `npm` or `pnpm` killed that way leaves its child, the real server, still listening. If the port is still taken after a kill, run the lookup again: the PID has probably changed to the child's.

## Why does the port keep getting taken?

Usually by your own dev server, started earlier and forgotten:

- in a terminal tab in another window, or a tab you closed while the server ran in the background;
- by a coding agent, in a shell you can't see (see [Claude Code started a second dev server](/guides/claude-code-dev-server));
- as a child process that outlived the parent you stopped (see [Kill leftover dev servers](/guides/kill-leftover-dev-servers)).

Killing it fixes today. The next time, you look up the PID again.

## How does localhost keep ports from colliding?

[localhost](/) is a free, open-source desktop app that runs your dev servers from one window. Each Service is your own command (`pnpm dev`, `uv run fastapi dev`, `cargo run`) with a **Port**, the TCP port it is expected to listen on, passed to the command as `$PORT`.

- **It checks before it starts.** Before starting a Service with a Port, localhost looks at who holds it. If something does, it says so, by name and PID ("Port 3000 is in use by node (PID 1234)."), and offers **Stop it and start**, **Start on 3001** or **Cancel**.
- **Or it moves, for that Run only.** If you allow it, the Service starts on the next free port and gets it as `$PORT`; the configured Port doesn't change. A Service that others depend on never moves, and neither does one whose port is held by something in the same Project.
- **The Ports view shows every listener.** **Ports** (⌘0, or Ctrl+0 on Linux and Windows) lists every process listening on a TCP port and whose it is: one of your Services, a process running inside one of your Projects' folders, or nothing localhost knows. Select one and press ⌘. (Ctrl+.) to stop it; localhost asks first for anything it didn't start.
- **Stop means stopped.** Stopping a Service ends its whole process tree, so no child is left holding the port.

How it decides a Service is up, and every rule for moving: [Ports](/docs/ports).
