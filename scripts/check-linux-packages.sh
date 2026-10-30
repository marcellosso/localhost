#!/bin/bash
# Checks that every Linux package carries the `localhost` command coding
# agents run: usr/bin/localhost (on the PATH as /usr/bin/localhost once the
# .deb or .rpm is installed; inside the AppImage, which the app copies out
# for agents), executable, reporting the expected version. Each package is
# unpacked, so this doesn't depend on how a tool lists its paths. Needs
# dpkg-deb, rpm2cpio and cpio.
#
#   scripts/check-linux-packages.sh <bundle folder> <version>
#
# <bundle folder> is Tauri's output (src-tauri/target/release/bundle).
set -euo pipefail

bundle=$(cd "${1:?usage: scripts/check-linux-packages.sh <bundle folder> <version>}" && pwd)
version=${2:?usage: scripts/check-linux-packages.sh <bundle folder> <version>}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

check() {
    local package=$1 root=$2 command=$2/usr/bin/localhost said
    if [[ ! -x $command ]]; then
        echo "error: $package has no executable usr/bin/localhost. Its usr/bin holds:" >&2
        ls -l "$root/usr/bin" >&2 2>&1 || true
        return 1
    fi
    said=$("$command" --version)
    if [[ $said != "localhost $version" ]]; then
        echo "error: $package's usr/bin/localhost says '$said', not 'localhost $version'" >&2
        return 1
    fi
    echo "$package: usr/bin/localhost, $said"
}

one() {
    # shellcheck disable=SC2206 # $1 is a pattern, expanded here on purpose
    local files=("$bundle"/$1)
    if [[ ! -f ${files[0]} ]]; then
        echo "error: no $1 in $bundle" >&2
        return 1
    fi
    echo "${files[0]}"
}

failed=0
mkdir "$work/deb" "$work/rpm" "$work/appimage"
if deb=$(one 'deb/*.deb'); then
    dpkg-deb -x "$deb" "$work/deb"
    check .deb "$work/deb" || failed=1
else failed=1; fi
if rpm=$(one 'rpm/*.rpm'); then
    (cd "$work/rpm" && rpm2cpio "$rpm" | cpio -idm --quiet)
    check .rpm "$work/rpm" || failed=1
else failed=1; fi
if appimage=$(one 'appimage/*.AppImage'); then
    # Unpacking needs no FUSE.
    (cd "$work/appimage" && "$appimage" --appimage-extract > /dev/null)
    check AppImage "$work/appimage/squashfs-root" || failed=1
else failed=1; fi
exit $failed
