# dmgbuild settings for localhost-<version>.dmg (scripts/build-release.sh).
# dmgbuild writes the window's layout straight into .DS_Store, without
# scripting Finder, so the release runner needs no GUI session.
# The positions are icon centres, in the points of background.svg. Paths are
# relative to the repo root, where build-release.sh runs it.
import os.path

app = defines["app"]  # noqa: F821 (dmgbuild passes it in)

format = "UDZO"
filesystem = "HFS+"
files = [app]
symlinks = {"Applications": "/Applications"}
icon = os.path.join(app, "Contents/Resources/AppIcon.icns")

# background@2x.png beside it is picked up for Retina screens.
background = "apps/macos/DMG/background.png"
window_rect = ((200, 120), (660, 400))
default_view = "icon-view"
show_status_bar = show_tab_view = show_toolbar = show_pathbar = show_sidebar = False
icon_size = 128
text_size = 13
icon_locations = {
    os.path.basename(app): (170, 190),
    "Applications": (490, 190),
}
