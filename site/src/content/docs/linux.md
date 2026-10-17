localhost on Linux comes as a `.deb`, an `.rpm` and an AppImage, for x86_64. See [Install](/docs/install).

## A Secret Service

Secret values are kept by your desktop's **Secret Service**: GNOME Keyring on GNOME, KWallet on KDE Plasma, or another provider such as KeePassXC with Secret Service enabled. Most desktops run one already. Without one, Services that use a Secret fail to start with “The secret store refused: …”, and saving a Secret fails the same way; everything else works.

## The tray icon on GNOME

The tray icon uses AppIndicator. KDE Plasma, Cinnamon, Xfce and Ubuntu's GNOME show it as is. Stock GNOME, as on Fedora, needs the [AppIndicator and KStatusNotifierItem Support](https://extensions.gnome.org/extension/615/appindicator-support/) extension (`gnome-shell-extension-appindicator` on Fedora).

Without a tray nothing is lost: closing the window still leaves your Services running, and launching localhost again brings the window back. Quit with Ctrl+Q.
