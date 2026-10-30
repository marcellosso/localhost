localhost on Linux comes as a `.deb`, an `.rpm` and an AppImage, for x86_64. See [Install](/docs/install).

## Updates

The AppImage updates itself, after asking: **Restart to Update** stops your running Services first. The `.deb` and `.rpm` belong to your package manager, so the update notice's **Download** opens the release; install the new package as in [Install](/docs/install). See [Updates](/docs/updates).

## A Secret Service

Secret values are kept by your desktop's **Secret Service**: GNOME Keyring on GNOME, KWallet on KDE Plasma, or another provider such as KeePassXC with Secret Service enabled. Most desktops run one already. Without one, Services that use a Secret fail to start with “The secret store refused: …”, and saving a Secret fails the same way; everything else works.

## The tray icon on GNOME

The tray icon uses AppIndicator. KDE Plasma, Cinnamon, Xfce and Ubuntu's GNOME show it as is. Stock GNOME, as on Fedora, needs the [AppIndicator and KStatusNotifierItem Support](https://extensions.gnome.org/extension/615/appindicator-support/) extension (`gnome-shell-extension-appindicator` on Fedora).

Without a tray nothing is lost: closing the window still leaves your Services running, and launching localhost again brings the window back. Quit with Ctrl+Q.

## Coding agents

[`localhost mcp`](/docs/mcp) and the app share one background core, on a socket only your user can open, `localhostd.sock` in localhost's data folder (`~/.local/share/localhost`, or `$XDG_DATA_HOME/localhost`). What an agent starts shows in the app, and the reverse.
