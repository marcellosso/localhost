The quickest way is one SSH connection that forwards the dev server's port to your laptop, and the server started in it:

```sh
ssh -L 5173:localhost:5173 devbox
cd ~/code/acme-shop && pnpm dev
```

Now `http://localhost:5173` on your laptop is the dev server on the dev box. Add one `-L` per port (`-L 8000:localhost:8000` for the API), and run it inside `tmux` on the dev box so closing the laptop doesn't stop it.

## Why run dev servers on another computer?

A desktop under the desk, a home server or a cloud VM often has more cores and memory than a laptop, stays on, and keeps the fans quiet. Builds, databases and Docker run there; the laptop only runs the browser and the editor. With VS Code's or Cursor's SSH remote, the editor already works on the dev box, and only the dev servers are left.

## How do I reach the dev box from anywhere?

Any network SSH reaches works. [Tailscale](https://tailscale.com) is the easiest from outside the house: install it on both computers and sign in with the same account, and the dev box gets a name such as `devbox.tail1234.ts.net` that works from any network, without opening a port on the router. A LAN, a VPN or a jump host (`ProxyJump`) work the same way.

Then give it a short name in `~/.ssh/config` on the laptop:

```
Host devbox
  HostName devbox.tail1234.ts.net
  User you
  IdentityFile ~/.ssh/id_ed25519
```

and a key, so SSH never asks for a password:

```sh
ssh-keygen -t ed25519      # if you don't have a key yet
ssh-copy-id devbox
ssh devbox true            # should ask nothing, and print nothing
```

If the key has a passphrase, `ssh-add` keeps it in the SSH agent. The first `ssh devbox` asks whether to trust the dev box's host key: check it, and answer `yes`.

## Where does `ssh -L` fall short?

- **One port per flag, decided in advance.** A Service that moves to another port, or a new one, means a new connection.
- **It stops with the terminal.** The forward lasts as long as that `ssh`; the dev server as long as its shell, unless tmux holds it.
- **You can't see what runs.** Which servers are up, which crashed and why, is in a tmux pane on another computer.
- **Starting them is still by hand.** The order, the commands and the environment are in your head, or in a script on the dev box.

## How does localhost run them from your laptop?

[localhost](/) is a free, open-source desktop app that remembers how each Project runs. A dev box you reach with `ssh` can be one of its [Machines](/docs/machines): its Projects show in the window on your laptop, below the laptop's own, and you start and stop them there.

1. [Install localhost](/docs/install) on both computers, and add your Projects on the dev box, in its app as on any computer ([Quick start](/docs/quick-start)), or through a coding agent there with [`localhost mcp`](/docs/mcp). Projects on the dev box are edited on the dev box; the app there doesn't have to stay open.
2. Check that the laptop's `ssh` finds it. A command run over SSH doesn't read your interactive profile, so with zsh, set `PATH` in `~/.zshenv`, not `~/.zshrc`:

   ```sh
   ssh devbox localhost --version
   ```

3. On the laptop, open **Settings → Machines → Add Machine…**, pick `devbox` from the hosts of `~/.ssh/config`, and click **Add**. localhost connects first; if it can't, it says why and what to do next.
4. The dev box's Projects show in the sidebar under its name. **Start All**, Stop, Restart and logs work as for a Project on the laptop, and the state of each Service is there at a glance.
5. **Open in Browser** forwards the Service's Port to the same port on your laptop, through the same SSH connection, then opens it: `http://localhost:5173` there is `http://localhost:5173` here, so OAuth callbacks and cookies keep working.

The Services run on the dev box, with its shell, its files and its Secrets; no Secret value reaches the laptop. Nothing new listens on either computer's network: Forwards are bound to the laptop's loopback only. If the connection drops, localhost makes it again on its own, without starting or stopping anything. Services you started from the laptop stop when the connection ends, unless localhost is open on the dev box.

When something doesn't connect, [Machines](/docs/machines#when-it-cant-connect) lists every message localhost shows and what to do.
