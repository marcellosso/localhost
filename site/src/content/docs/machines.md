A **Machine** is another computer whose localhost this app shows and controls: a dev box under the desk, a home server, a cloud VM. localhost reaches it with your system's `ssh`, as you already do from a terminal. Its Projects show in the sidebar below this computer's, and you add, edit, start, stop and watch them from here.

Everything stays on the Machine: its Project Configuration, its Secrets and its Runs. Its Services run there, in its login shell, with its files, and read its Secrets from its own secret store. A Secret you type here is sent to the Machine and stored there only; nothing of a Machine's Projects is saved on this computer.

## Add a Machine

1. Open **Settings → Machines** and click **Add Machine…**.
2. In **SSH host**, type what you would type after `ssh`: a `Host` from `~/.ssh/config` (the field lists them), `user@host`, or `user@host:port`.
3. Give it a **Name** if you like (it is the destination otherwise), and click **Add**.

localhost connects first and keeps the Machine only once it is reached. If it can't be, it says why and what to do next: see [When it can't connect](#when-it-cant-connect). Each Machine in Settings shows its destination and how it is (`Connected · localhost 0.5.0`, `Connecting…`, or what is wrong), with **Reconnect**, **Rename…** and **Remove…** in its `⋯` menu. Removing a Machine changes nothing on it: its Projects and Services stay as they are.

## What it needs

- **`ssh <destination>` works from a terminal without asking anything.** localhost runs `ssh` with `BatchMode=yes`, so it never types a password or a passphrase. Use a key, and if it has a passphrase, add it to the SSH agent with `ssh-add`.
- **Its host key is already trusted.** Connect once from a terminal and answer `yes`. localhost checks host keys as your configuration says, and never trusts one for you.
- **localhost is installed on the Machine, and on the `PATH` that ssh's commands get.** localhost runs `ssh <destination> localhost remote --stdio`. A command run over ssh gets a non-interactive shell, which doesn't read every profile: with zsh, `~/.zshrc` and `~/.zprofile` aren't read, only `~/.zshenv`. Check it from this computer:

  ```sh
  ssh devbox localhost --version
  ```

  If that says `command not found`, put the folder holding `localhost` on the `PATH` in `~/.zshenv` (zsh) or `~/.bashrc` (bash, before any line that returns early for non-interactive shells). For example, after [Add to PATH](/docs/mcp#use-it-in-a-terminal), which links `~/.local/bin/localhost`:

  ```sh
  export PATH="$HOME/.local/bin:$PATH"
  ```

  Homebrew's `bin` (`/opt/homebrew/bin` on Apple silicon) is often set only in `~/.zprofile`, so a Mac Machine with the [Homebrew cask](/docs/install#homebrew-macos) needs it in `~/.zshenv` too. The Linux `.deb` and `.rpm` put `localhost` in `/usr/bin`, which every shell finds.
- **A localhost with Machines on both computers.** They don't have to be the same version; one from before Machines can't be viewed, and says so.

## Any network

localhost uses whatever `ssh` uses: your keys and agent, `~/.ssh/config`, `ProxyJump`, a LAN, a VPN, or Tailscale. It adds no sign-in, no account and no port of its own: nothing new listens on either computer's network. On the Machine, `localhost remote --stdio` is one more client of its background core, as `localhost mcp` is.

One `ssh` connection per Machine carries everything, and is also an SSH ControlMaster. Its socket is in a folder only you can open, in localhost's storage. Forwards from your own `~/.ssh/config` are cleared on it, so only the ones localhost asks for are made.

## Open in Browser and Forwards

A Machine's Service opens in your browser here as a local one does. **Open in Browser** first forwards its Port to the same port on this computer, through the SSH connection, then opens the same URL: `http://localhost:5173` on the Machine is `http://localhost:5173` here. Same number, because `*.localhost` names, OAuth callbacks and cookies expect it.

A **Forward** is bound to this computer's loopback addresses only (`127.0.0.1`, and `::1` where it exists), so nothing else on your network reaches it. It reaches the Service on the Machine whether it listens on its loopback or on every address. Forwards end with the connection.

If that port is taken on this computer, Open in Browser says so and names who holds it, rather than using another port. Stop whatever holds it here (the [Ports view](/docs/ports#the-ports-view) shows it), then try again.

## In the sidebar

Each Machine has its own section below this computer's Projects, under its name, with a `+` to add a Project there. Its Projects look and act like local ones: Start, Stop, Restart, Start All, Stop All, logs, Open in Browser, and [their edits](#manage-its-projects). While the Machine isn't connected, its Projects are dimmed and show what they showed last; the section says why, with **Reconnect** when it waits for you. A connected Machine with no Projects says so.

## Manage its Projects

A connected Machine's Projects are managed from here as this computer's are, with the same sheets, menus, command palette entries and shortcuts. Each change is made on the Machine, by its localhost, and shows at once in an app open there.

- **Add a Project**: the `+` next to the Machine's name in the sidebar, then **New Project on devbox…**. With one of its Projects selected, the usual shortcut adds there too. Its folder is picked on the Machine, scanned there, and the Suggestions are reviewed as for a local folder.
- **Import a Project** from a [Project file](/docs/configuration) on the Machine: **Import Project on devbox…**.
- **Edit** a Project, its Environment and Profiles, its Services (add, edit, remove, Detect Services), and **Favorite** it, from its context menu, its `⋯` menu, the Services column or the command palette.
- **Remove** a Project or Service: as here, its running Services are stopped first, on the Machine, and their logs there are deleted. The Project's folder is not touched.

Folders of a Machine are picked in localhost's own folder picker, since the system's can't browse another computer: a path you can type, its folders in a list, a **Show hidden** switch, and **Choose**. It starts in the Machine's home folder. Paths are checked on the Machine: a folder that isn't there, or that you can't open, says so.

A Secret's value is set from here like any variable, and stored only in the Machine's secret store. **Reveal** reads it from the Machine to show it, without keeping it here.

These need a connection: while the Machine isn't connected, they are disabled and say why; nothing is queued for later. A Machine running a localhost from before this was possible shows its Projects and runs them, and says **Update localhost on devbox to manage its Projects.**

## Connections, sleep and quitting

- **A lost connection is made again on its own**, after 1 second, then 2, 5, 15, and every 30 seconds, and its Projects and statuses are read again. On a Mac, also as soon as it wakes from sleep. Reconnecting never starts or stops a Service.
- **A problem waiting won't fix** (a host key, a sign-in that needs a prompt, no localhost there, one too old, or anything SSH says that localhost doesn't recognize) is shown once, and waits for **Reconnect**.
- **Services you started from here** stop on the Machine once the connection ends, unless localhost is open on it: an app open there keeps everything it shows running. A Machine counts a connection as ended after 45 seconds without hearing from it, so closing a laptop's lid ends it too.
- **Quitting** asks first when Services started from here on another Machine are active, and says that they stop too, unless localhost is open on them.

## When it can't connect

localhost reads what `ssh` says and puts it in plain words, with what to do next. These are its messages, with `devbox` as the Machine:

| It says | What to do |
|---|---|
| devbox’s host key isn’t trusted yet. Connect once from a terminal with `ssh devbox`, then try again. | Run `ssh devbox`, check the fingerprint, answer `yes`, then **Reconnect**. |
| devbox’s host key changed. If you expected that, update ~/.ssh/known_hosts from a terminal; otherwise don’t connect. | If the Machine was reinstalled, run `ssh-keygen -R <host>`, then connect once from a terminal. If not, someone may be in between: don't connect. |
| SSH can’t sign in to devbox without asking for a password or passphrase. Add your key to the SSH agent with ssh-add, then try again. | `ssh-add`, or set up a key with `ssh-copy-id devbox` if you sign in with a password. Check that `ssh devbox true` asks nothing. |
| devbox wasn’t found. Check its name in ~/.ssh/config, and that you’re on its network. | Check the `Host` and `HostName`, and that Tailscale or the VPN is on. Retried on its own. |
| localhost can’t reach devbox: … | The network refused or timed out, in SSH's words: is the Machine on and awake, and is `sshd` running? Retried on its own. |
| localhost isn’t installed on devbox, or isn’t on its PATH. Install it there, then try again. | Install it, then check `ssh devbox localhost --version`. With zsh, set `PATH` in `~/.zshenv`: see [What it needs](#what-it-needs). |
| devbox runs a localhost that can’t be viewed from another Machine. Update localhost there. | Update localhost on the Machine, then **Reconnect**. |
| localhost on devbox couldn’t start: … | localhost is there, but couldn't serve this app, in its own words. Most often: see below. |
| The connection to devbox was lost. | Nothing: it tries again on its own. |
| SSH couldn’t connect to devbox: … | Anything else SSH said, often a mistake in `~/.ssh/config`. Check that `ssh devbox` works from a terminal, then **Reconnect**. |
| There is no ssh command on this computer. Install OpenSSH, then try again. | Install OpenSSH's client. |

### “localhost 0.4.0 is still running in the background”

> localhost on devbox couldn’t start: localhost 0.4.0 is still running in the background, perhaps for a coding agent, and can’t work with this version. Quit it and any agent using `localhost mcp`, then try again.

The Machine's localhost was updated, but its background core is still the old version's, kept alive by something using it, most often a coding agent's `localhost mcp` there. The new version can't share it. On the Machine, quit the localhost app and end the agent sessions using `localhost mcp` (or restart them, so they start the new version), then **Reconnect** here.
