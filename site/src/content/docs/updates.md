localhost checks GitHub Releases for a new version at launch and then every 6 hours. It offers a new version; it never installs one without asking.

## The update notice

When there is a new version, a notice at the bottom of the sidebar says “localhost X.Y.Z is available”, with a link to its **Release Notes**:

- **Update Now** downloads it in the background, with its progress in the notice.
- **Restart to Update** appears once it has downloaded, and installs it and opens the new version.
- **Later** hides the notice until the next version, or until you check again.

On macOS, an update that has downloaded also installs when you quit localhost.

## Running Services stop first

Installing restarts localhost, so, as when you quit, **every running Service stops first**: localhost lists the ones that are running and asks before it goes on. Start them again once the new version opens. A coding agent's Services stop too, since they run in the same [background core](/docs/mcp#the-background-core).

## Which packages update themselves

| | How it updates |
|---|---|
| macOS | The app updates itself. |
| Windows | The app updates itself. The `localhostd` it copies into WSL distros is replaced with it. |
| Linux AppImage | The AppImage updates itself. |
| Linux `.deb`, `.rpm` | Your package manager installs updates, so the notice says “Install it with your package manager.” and its **Download** opens the release. Install it as in [Install](/docs/install). |

Each update is signed, and localhost checks the signature against a key built into the app before installing it.

## Turn checks off, or check now

Turn off **Check for updates automatically** in **Settings → General**. **Check for Updates…** checks at any time: in the localhost menu on macOS, in **Settings → General** (**Check Now**) and in the command palette.

Checking is the only thing localhost asks the network for. Builds you make yourself never check.

The `localhost` command, built from source, doesn't update itself: build it again from a newer checkout.
