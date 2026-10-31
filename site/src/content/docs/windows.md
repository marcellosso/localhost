localhost on Windows installs for your user only, on Windows 10 or 11 (x64); see [Install](/docs/install). A Project runs on its **Execution Target**: Windows itself (the default), or a WSL distro such as Ubuntu. Add Project asks which.

## On Windows itself

Commands run in `cmd.exe` with your user environment read fresh at every start, so a toolchain you just installed is found without restarting localhost. Version managers that only work through a PowerShell profile (fnm or mise in `activate` mode) aren't seen; their shim modes, nvm-windows and Volta are.

## In WSL

Pick the folder under **Linux** in the folder picker (`\\wsl.localhost\<distro>\…`). Commands, detection and the Ports view run inside the distro, as on Linux. A folder on a Windows drive also works, through `/mnt/c/…`, but is much slower.

The first time a distro is used, localhost copies a small helper, `localhostd`, into `~/.local/share/localhost/windows/` there; it is replaced when localhost updates, and stops every Service it started when localhost quits.

Open in Browser uses `localhost:<port>`, which WSL forwards to Windows by default. If a Service in WSL is Running but the browser can't reach it, check that `%UserProfile%\.wslconfig` doesn't turn `localhostForwarding` off, then run `wsl --shutdown` and start it again.

## Coding agents

[`localhost mcp`](/docs/mcp) works on Windows for Projects on Windows itself: the app and the agent share one background core, reached through a named pipe (`\\.\pipe\localhostd-<number>`) that only your user can open. It runs without a window, as a second `localhost-app.exe` or `localhost.exe` in Task Manager.

A Project in a WSL distro runs only from the app, which starts the `localhostd` inside the distro. An agent asked to start one is told so.

## Updates

The app installed by `localhost_<version>_x64-setup.exe` updates itself: **Restart to Update** stops your running Services, after asking, then installs the new version and opens it. The `localhostd` in each distro is replaced with it. See [Updates](/docs/updates).

## Terminal, editors and the tray

- Open in Terminal uses Windows Terminal (with the distro's profile for WSL Projects). VS Code, Cursor, VSCodium and Windsurf open WSL folders through their WSL remote.
- Windows puts a new app's tray icon in the overflow (the `^` next to the clock). To keep it on the taskbar, turn localhost on in **Settings › Personalization › Taskbar › Other system tray icons**. Left-click it to open the window, right-click it for the menu.

## Uninstall

Uninstall from **Settings › Apps › Installed apps**, then remove the copies in each distro with `rm -rf ~/.local/share/localhost/windows`.
