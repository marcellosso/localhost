#!/bin/bash
# Builds localhost.app in Release, signs it and packs it into a DMG.
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

echo "==> Building localhost $version ($build_number)" >&2
rm -rf "$output"
xcodebuild -project localhost.xcodeproj -scheme localhost -configuration Release \
    -destination 'generic/platform=macOS' -derivedDataPath "$derived" \
    CODE_SIGNING_ALLOWED=NO \
    MARKETING_VERSION="$version" CURRENT_PROJECT_VERSION="$build_number" \
    -quiet clean build >&2

echo "==> Signing with ${identity/#-/an ad hoc signature}" >&2
# Hardened runtime as in the Xcode project; the app has no nested code to sign first.
codesign --force --sign "$identity" "${timestamp[@]}" \
    --options runtime --entitlements App/Localhost.entitlements "$app" >&2
codesign --verify --strict --verbose=2 "$app" >&2

echo "==> Packing $dmg" >&2
staging="$output/dmg"
mkdir -p "$staging"
ditto "$app" "$staging/localhost.app"
ln -s /Applications "$staging/Applications"
hdiutil create -volname localhost -srcfolder "$staging" -fs HFS+ -format UDZO -ov "$dmg" >&2
rm -rf "$staging"
if [[ $identity != - ]]; then
    codesign --force --sign "$identity" "${timestamp[@]}" "$dmg" >&2
fi

echo "$dmg"
