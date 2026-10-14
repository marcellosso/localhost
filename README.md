<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset=".github/assets/wordmark-dark.png">
    <img src=".github/assets/wordmark-light.png" alt="localhost" width="280">
  </picture>
</h1>

<p align="center">Every local project, one place to run it. A native macOS app, and an app for Linux.</p>

![The localhost window: the Acme Shop Project with its Services (Migrations exited, API, Web and Worker running, Storybook manual) and their merged log](.github/assets/main-window-dark.png)

A native macOS app that remembers how each of your local development projects runs, and lets you start, stop and watch all of its Services from one place.

A **Project** is a folder on your Mac. A **Service** is one long-running command in it: `pnpm dev`, `uv run fastapi dev`, `docker compose up postgres`, `cargo run`. localhost doesn't care what the command is; it runs it in your login shell, keeps its output, knows when it is up, and stops its whole process tree when you ask.

- **One place for every project**: Start All / Stop All, Dependencies between Services (the API waits for the database), a merged log of every Service.
- **Ports that make sense**: each Service can have a Port; it counts as Running once the Port accepts connections. If the Port is taken, localhost tells you by whom and can start the Service on the next free port. A **Ports** view lists everything listening on your Mac, whoever started it.
- **Environment without `.env` juggling**: variables on the Project, on each Service, and in Environment Profiles (Development, Testing, …). Any variable can be a Secret, kept in your Keychain.
- **Set up in seconds**: point it at a folder and it suggests Services from `package.json` scripts, Compose files, Python apps and Cargo binaries. You review every suggestion; nothing is applied silently.
- **Native**: SwiftUI, keyboard first (⌘K command palette), a menu bar companion, VoiceOver, light and dark.

localhost stores its configuration in `~/Library/Application Support/localhost`, never in your project folders. To move a Project to another Mac or share it, export it as a YAML Project file (see [docs/configuration.md](docs/configuration.md)).

Requires macOS 15 or later. There is also an app for Linux; see [Linux](#linux).

## Install

1. Download the latest `localhost-<version>.dmg` from [GitHub Releases](https://github.com/sixpetals/localhost/releases).
2. Open it and drag **localhost** to **Applications**.
3. Open localhost from Applications. The first launch needs one extra step while releases aren't notarized; see below.

### First launch of a build that isn't notarized

localhost isn't notarized by Apple yet, so macOS 15 refuses to open it the first time ("Apple could not verify “localhost” is free of malware…"). To open it anyway:

1. Click **Done** in that dialog.
2. Open **System Settings → Privacy & Security** and scroll down to **Security**. It says “localhost” was blocked to protect your Mac.
3. Click **Open Anyway**, confirm with your password or Touch ID, then click **Open Anyway** once more in the dialog that follows.

macOS remembers this; later launches open normally. Each new version you download asks once again.

Or, from Terminal, remove the quarantine flag the download added and open the app as usual:

```sh
xattr -dr com.apple.quarantine /Applications/localhost.app
```

Only do this for a copy you downloaded from this project's releases.

### Keychain prompts for Secrets

Secret values live in your login Keychain. macOS lets the app that saved them read them without asking, and recognises "the same app" by its code signature:

- **Release builds** are signed with the same certificate every time, so updating keeps access: no prompts.
- **Builds signed ad hoc** (including ones you build yourself without a signing identity) look like a new app after every update. The first time each Secret is used, macOS asks “localhost wants to use your confidential information stored in “com.sixpetals.localhost.secret” in your keychain”. Enter your login password and click **Always Allow**.
- **When the signing identity changes** (for example once releases become notarized), macOS asks once for each Secret on that one update, the same way.

**Deny** isn't remembered: Services that use that Secret fail to start, and the next start asks again.

## Linux

The Linux app does what the macOS app does, with Ctrl in place of ⌘ (Ctrl+K for the command palette) and a tray icon in place of the menu bar. It runs on x86_64 with a glibc at least as new as Ubuntu 22.04's.

### Install

Download one of these from [GitHub Releases](https://github.com/sixpetals/localhost/releases):

- **Ubuntu, Debian** (Ubuntu 22.04 or later, Debian 12): `localhost-app_<version>_amd64.deb`, then `sudo apt install ./localhost-app_<version>_amd64.deb`.
- **Fedora**: `localhost-app-<version>-1.x86_64.rpm`, then `sudo dnf install ./localhost-app-<version>-1.x86_64.rpm`.
- **Anything else**: `localhost-app_<version>_amd64.AppImage`. Make it executable (`chmod +x`) and run it.

The packages install the app as `localhost-app`, with a **localhost** entry in your applications. They aren't signed yet. Not available as a Flatpak or Snap: their sandboxes can't run your project's commands on the host.

### Secrets need a Secret Service

Secret values are kept by your desktop's **Secret Service**: GNOME Keyring on GNOME, KWallet on KDE Plasma (or another provider such as KeePassXC with Secret Service enabled). Most desktops run one already. Without one, Services that use a Secret fail to start with “The secret store refused: …”, and saving a Secret fails the same way; everything else works.

### The tray icon on GNOME

The tray icon (each favourite or running Project's Services, with Start, Stop, Restart and Open in Browser) uses AppIndicator. KDE Plasma, Cinnamon, Xfce and Ubuntu's GNOME show it as is. Stock GNOME, as on Fedora, needs the [AppIndicator and KStatusNotifierItem Support](https://extensions.gnome.org/extension/615/appindicator-support/) extension (`gnome-shell-extension-appindicator` on Fedora).

Without a tray, nothing is lost: closing the window still leaves your Services running, and launching localhost again brings the window back. Quit with Ctrl+Q; it asks first when something is running.

### Data

Everything is in `~/.local/share/localhost` (or `$XDG_DATA_HOME/localhost`), laid out as in [Data](#data) below. Secret values are in the Secret Service, under the attributes `service` = `com.sixpetals.localhost.secret` and `account`.

## Getting started

### 1. Add a Project

Press **⌘N** (or the **+** in the sidebar) and choose your project's folder. localhost fills in a name and a **Key** (a short handle such as `my-saas`, used in logs), then scans the folder and lists what it found:

- **Node**: `package.json` scripts, run with your package manager (`pnpm run dev`, …). Known dev servers get their default port, passed as `$PORT`.
- **Docker Compose**: one Service per Compose service, `docker compose up <service>` in the foreground.
- **Python**: Django, FastAPI and Flask apps, with uv, Poetry, Pipenv or a virtualenv.
- **Cargo**: `cargo run` for each binary.

The likely Services are checked; the rest are folded under “N more found”. Click a row to edit it before adding. Click **Add Project**.

Nothing found, or a command it doesn't know? Add the Project anyway and add Services yourself with **⇧⌘N** (File → New Service…). **File → Detect Services…** scans again later.

### 2. Describe a Service

A Service has:

- a **Command**: one command line, run in your login shell (`$SHELL -l -c "…"`), so your `PATH`, version managers and aliases from your profile apply. Pipes, `&&` and `$VARIABLES` work as in Terminal;
- a **working directory** inside the Project (the Project folder by default);
- an optional **Port**. The command receives the actual port as `$PORT` (e.g. `next dev -p $PORT`), and the Service counts as Running once that port accepts connections. Without a Port it counts as Running after about 1.5 seconds;
- an optional **URL**, for Open in Browser (⇧⌘O). Its port follows the actual port;
- **Starts after**: Dependencies. Starting the Service starts them first and waits until they are Running (or have Exited, for one-off steps such as migrations);
- **Manual**: Start All skips it, unless something being started depends on it.

### 3. Run it

Select a Service and press **⌘R** to start (or restart), **⌘.** to stop. **⇧⌘R** starts every Service of the Project in dependency order, **⇧⌘.** stops them all.

A Service is always in one of six states:

| State | Meaning |
|---|---|
| Stopped | Not running: never started, or you stopped it. |
| Starting | Waiting for its Dependencies, or launched and not up yet. |
| Running | Up: its Port accepts connections (or it has been alive ~1.5s). |
| Stopping | Asked to stop; waiting for every process it started to end. |
| Exited | Ended on its own with exit code 0. |
| Failed | Ended on its own with an error, or couldn't start. The reason and the last lines of output are shown. |

Stopping sends Ctrl-C (SIGINT) to the Service's whole process group, then SIGTERM after 5 seconds, then SIGKILL, so nothing is left running in the background.

Closing the window doesn't stop anything: Services keep running and the **menu bar** icon shows them (a red dot while one has Failed). Quitting asks first, then stops everything.

<p align="center"><img src=".github/assets/menu-bar-light.png" alt="The menu bar panel: Acme Shop's Services with their states and Start, Restart, Stop and Open in Browser buttons" width="340"></p>

If localhost itself crashes, the next launch finds the Services still running from that session and asks whether to stop them.

### 4. Read the logs

Select a Service to see its output, with colours. **All services** shows every Service of the Project in one stream, each line prefixed with its Key. **⌘F** filters, **⌃L** clears the view (log files are kept). URLs in the output are clickable.

![One Service selected in light mode: Web is Running on port 5173, with Open, Restart and Stop buttons and its Vite output](.github/assets/service-light.png)

Each Run's output is also written to `~/Library/Application Support/localhost/logs/`; the last 5 Runs of each Service are kept, up to 10 MB each.

### 5. Environment variables and Secrets

**Project → Edit Environment…** holds the Project's variables and its **Environment Profiles**; each Service's own variables are in its editor. A Service gets, later ones winning:

1. your login shell's environment,
2. the Project's variables,
3. the Service's variables,
4. the active Environment Profile's variables (picked in the Project's toolbar),

plus `PORT` when it has a Port, and `FORCE_COLOR=1`, `CLICOLOR_FORCE=1`, `PYTHONUNBUFFERED=1` so tools keep colours and don't buffer output.

Click the lock next to a variable to make it a **Secret**: its value goes to your Keychain and is hidden until you click the eye. Changes apply the next time a Service starts; a running Service shows “Restart to apply”.

### 6. When a port is taken

Before starting a Service with a Port, localhost checks who holds it. If it is something else, the Service can start on the next free port (it gets that port as `$PORT` for this Run; the configured Port is unchanged), or you choose: stop the other process and start, start on another port, or cancel. Settings → Ports turns the automatic move off; a Service's **Port can change** switch turns it off for that Service.

The **Ports** view (**⌘0**) lists every process listening on a TCP port and whom it belongs to: one of your Services, a process running inside one of your Projects' folders, or nothing localhost knows (other apps and macOS are folded under "Apps & macOS"). Select one and press ⌘. to stop it; localhost asks first for anything it didn't start.

### 7. Move a Project to another Mac

**File → Export Project…** saves the selected Project as a YAML Project file. **File → Import Project…** reads one back and shows what it contains before adding anything; choose the folder if it is somewhere else on this Mac. Secret values are never exported: the file lists Secret names, and the import shows which ones to fill in. The format is documented in [docs/configuration.md](docs/configuration.md).

## Keyboard shortcuts

![The ⌘K command palette searching “web”: jump to the Web Service, or restart, stop or open it](.github/assets/command-palette-dark.png)

| | |
|---|---|
| ⌘K | Command palette: jump to anything, run any action |
| ⌘N / ⇧⌘N | New Project / New Service |
| ⌘R / ⌘. | Start or restart / stop the selected Service |
| ⇧⌘R / ⇧⌘. | Start All / Stop All in the Project |
| ⇧⌘O | Open the Service's URL |
| ⌘F / ⌃L | Filter / clear the log |
| ⌥⌘T / ⌥⌘E / ⌥⌘F | Open the folder in Terminal / your editor / Finder |
| ⌘1–9, ⌘0 | Projects, Ports |
| ⌥⌘1–9, ⌘[ / ⌘] | Services; previous / next Service |
| ← → ↑ ↓ | Move between the sidebar, the Services and the log |

Hold ⌘ (or ⌥⌘) for a moment to see the numbers. Every action is also in the menu bar menus, so menu search (⇧⌘/) finds it.

## Data

Everything localhost knows is in `~/Library/Application Support/localhost`:

- `projects/`: one JSON file per Project. A file that can't be read is renamed to `….unreadable` and reported at launch, never deleted.
- `logs/`: each Run's output.
- `runs/`: the Runs active right now, to find leftovers after a crash.

Secret values are in your login Keychain, under the service `com.sixpetals.localhost.secret`.

## Building from source and contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT, see [LICENSE](LICENSE).
