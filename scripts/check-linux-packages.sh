#!/bin/bash
# Checks that every Linux package carries the `localhost` command coding
# agents run: usr/bin/localhost (on the PATH as /usr/bin/localhost once the
# .deb or .rpm is installed; inside the AppImage, which the app copies out
# for agents), executable, reporting the expected version. Each package is
# unpacked, so this doesn't depend on how a tool lists its paths. Every
# package is checked, and every failure says what failed. Needs dpkg-deb and
# bsdtar (libarchive-tools), which reads an .rpm whatever its compression.
#
#   scripts/check-linux-packages.sh <bundle folder> <version>
#
# <bundle folder> is Tauri's output (src-tauri/target/release/bundle).
set -uo pipefail

usage="usage: scripts/check-linux-packages.sh <bundle folder> <version>"
bundle=$(cd "${1:?$usage}" && pwd) || exit 1
version=${2:?$usage}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
# Anything else failing still says so.
trap 'echo "error: line $LINENO: \`$BASH_COMMAND\` exited $?" >&2' ERR

for tool in dpkg-deb bsdtar; do
    command -v "$tool" > /dev/null || { echo "error: $tool isn't installed" >&2; exit 1; }
done

# The one file matching a pattern under the bundle folder.
one() {
    local files
    # shellcheck disable=SC2206 # $1 is a pattern, expanded here on purpose
    files=("$bundle"/$1)
    if [[ ! -f ${files[0]} ]]; then
        echo "error: no $1 in $bundle" >&2
        return 1
    fi
    echo "${files[0]}"
}

# Unpacks a package into a folder, saying why if it can't.
unpack() {
    local kind=$1 file=$2 into=$3 output
    mkdir -p "$into"
    case $kind in
        .deb) output=$(dpkg-deb -x "$file" "$into" 2>&1) ;;
        .rpm) output=$(bsdtar -xf "$file" -C "$into" 2>&1) ;;
        AppImage) output=$(cd "$into" && "$file" --appimage-extract 2>&1 > /dev/null) ;;
    esac
    local status=$?
    if (( status != 0 )); then
        echo "error: couldn't unpack the $kind ($(basename "$file")), exit $status: $output" >&2
        return 1
    fi
}

check() {
    local package=$1 root=$2 command=$2/usr/bin/localhost said
    if [[ ! -x $command ]]; then
        echo "error: $package has no executable usr/bin/localhost. Its usr/bin holds:" >&2
        ls -l "$root/usr/bin" >&2 2>&1 || echo "(no usr/bin; it holds: $(ls "$root"))" >&2
        return 1
    fi
    if ! said=$("$command" --version 2>&1); then
        echo "error: $package's usr/bin/localhost --version failed: $said" >&2
        return 1
    fi
    if [[ $said != "localhost $version" ]]; then
        echo "error: $package's usr/bin/localhost says '$said', not 'localhost $version'" >&2
        return 1
    fi
    echo "$package: usr/bin/localhost, $said"
}

failed=0
for package in ".deb:deb/*.deb" ".rpm:rpm/*.rpm" "AppImage:appimage/*.AppImage"; do
    kind=${package%%:*} pattern=${package#*:}
    root=$work/${kind#.}
    if file=$(one "$pattern") && unpack "$kind" "$file" "$root"; then
        [[ $kind == AppImage ]] && root=$root/squashfs-root
        check "$kind" "$root" || failed=1
    else
        failed=1
    fi
done
(( failed == 0 )) && echo "Every package carries localhost $version." || echo "error: see above" >&2
exit $failed
