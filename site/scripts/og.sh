#!/bin/bash
# Renders a social image from its HTML template in scripts/og/ with headless
# Chrome: scripts/og.sh agents writes public/og-agents.png, 1280×640 like og.png.
# Run from site/. CHROME overrides where Chrome is.
set -euo pipefail
name=${1:?usage: scripts/og.sh <name>}
chrome=${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}
"$chrome" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=1 \
  --allow-file-access-from-files --window-size=1280,640 \
  --screenshot="$PWD/public/og-$name.png" "file://$PWD/scripts/og/$name.html"
