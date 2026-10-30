#!/bin/bash
# Builds localhost.app in Release, signs it and packs it into a DMG. Needs
# Rust with both macOS targets (CONTRIBUTING.md), and python3 for dmgbuild.
#
#   scripts/build-release.sh <version> [build number]
#
# <version> is MAJOR.MINOR.PATCH (the release tag without its "v"). The app is
# built unsigned, then signed here, so every signing mode takes the same path:
#
#   SIGNING_IDENTITY   a code signing identity (name or SHA-1) in a keychain on the
#                      search list; unset or "-" signs ad hoc
#   DEVELOPER_ID=1     add a secure timestamp, which notarization requires
#   OUTPUT_DIR         where the DMG goes (default: build/release)
#   SPARKLE_PUBLIC_KEY the updater's public key (RELEASING.md); without it the app
#                      doesn't check for updates
#   FEED_URL           where the app looks for updates, if not the Info.plist's appcast
#
# Prints the DMG's path on the last line.
set -euo pipefail

version=${1:?usage: scripts/build-release.sh <version> [build number]}
build_number=${2:-1}
if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "error: version must be MAJOR.MINOR.PATCH, got '$version'" >&2
    exit 1
fi

cd "$(dirname "$0")/.."
identity=${SIGNING_IDENTITY:--}
output=${OUTPUT_DIR:-build/release}
derived="$output/DerivedData"
app="$derived/Build/Products/Release/localhost.app"
dmg="$output/localhost-$version.dmg"

# Apple's timestamp server is only needed (and only worth the network call) for notarization.
timestamp=(--timestamp=none)
[[ ${DEVELOPER_ID:-} == 1 ]] && timestamp=(--timestamp)

echo "==> Building the Rust core (universal)" >&2
# The scheme's pre-action would build it too, but hides the output. The
# `localhost` command it builds reports the app's version.
export LOCALHOST_VERSION=$version
scripts/build-core.sh release >&2

echo "==> Building localhost $version ($build_number)" >&2
rm -rf "$output"
xcodebuild -project apps/macos/localhost.xcodeproj -scheme localhost -configuration Release \
    -destination 'generic/platform=macOS' -derivedDataPath "$derived" \
    CODE_SIGNING_ALLOWED=NO \
    MARKETING_VERSION="$version" CURRENT_PROJECT_VERSION="$build_number" LOCALHOST_VERSION="$version" \
    -quiet clean build >&2

# The Rust core is linked in statically, so the app binary carries both; the
# `localhost` command agents run (Contents/Helpers) too.
command="$app/Contents/Helpers/localhost"
for binary in "$app/Contents/MacOS/localhost" "$command"; do
    for arch in arm64 x86_64; do
        if ! lipo "$binary" -verify_arch "$arch"; then
            echo "error: $binary has no $arch slice" >&2
            exit 1
        fi
    done
done

# Updates (apps/macos/App/Updates.swift): the key apps verify them with, and where they look.
plist="$app/Contents/Info.plist"
if [[ -n ${SPARKLE_PUBLIC_KEY:-} ]]; then
    /usr/libexec/PlistBuddy -c "Add :SUPublicEDKey string $SPARKLE_PUBLIC_KEY" "$plist"
else
    echo "warning: no SPARKLE_PUBLIC_KEY: this build won't check for updates" >&2
fi
if [[ -n ${FEED_URL:-} ]]; then
    /usr/libexec/PlistBuddy -c "Set :SUFeedURL $FEED_URL" "$plist"
fi

echo "==> Signing with ${identity/#-/an ad hoc signature}" >&2
# Hardened runtime as in the Xcode project. Sparkle's helpers first, inside
# out, then the `localhost` command, then the app; Sparkle's own
# documentation lists its helpers.
sparkle="$app/Contents/Frameworks/Sparkle.framework/Versions/B"
sign() { codesign --force --sign "$identity" "${timestamp[@]}" --options runtime "$@" >&2; }
sign "$sparkle/XPCServices/Installer.xpc"
sign --preserve-metadata=entitlements "$sparkle/XPCServices/Downloader.xpc"
sign "$sparkle/Autoupdate"
sign "$sparkle/Updater.app"
sign "$app/Contents/Frameworks/Sparkle.framework"
sign "$command"
sign --entitlements apps/macos/App/Localhost.entitlements "$app"
codesign --verify --strict --verbose=2 "$app" >&2
codesign --verify --strict --verbose=2 "$command" >&2
# What agents run: the same version as the app, and it starts.
if [[ $("$command" --version) != "localhost $version" ]]; then
    echo "error: the bundled command says $("$command" --version), not localhost $version" >&2
    exit 1
fi

echo "==> Packing $dmg" >&2
# dmgbuild lays out the DMG's window (apps/macos/DMG), in a venv of its own.
dmgbuild_venv=build/dmgbuild-venv
if [[ ! -x $dmgbuild_venv/bin/dmgbuild ]]; then
    python3 -m venv "$dmgbuild_venv" >&2
    "$dmgbuild_venv/bin/pip" install --quiet --disable-pip-version-check dmgbuild==1.6.7 >&2
fi
"$dmgbuild_venv/bin/dmgbuild" -s apps/macos/DMG/dmg-settings.py -D app="$app" localhost "$dmg" >&2
if [[ $identity != - ]]; then
    codesign --force --sign "$identity" "${timestamp[@]}" "$dmg" >&2
fi

echo "$dmg"
