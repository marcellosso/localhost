#!/bin/bash
# Writes `latest.json`, which the Linux and Windows app checks for updates
# (apps/desktop/src-tauri/src/updates.rs), for one release.
#
#   scripts/updater-manifest.sh <folder> <version> <release url> <public key>
#
# <folder> holds the packages and, beside each, the `.sig` that `tauri build`
# (or `tauri signer sign`) wrote: the AppImage, the .deb, the .rpm and the
# Windows installer. <release url> is the release's page, e.g.
# https://github.com/marcellosso/localhost/releases/tag/v0.5.0, whose
# download folder holds the packages. <public key> is the updater's public
# key: every signature must be by its key, so a mismatched secret fails the
# release here rather than every user's update. Prints the JSON.
#
# Each package is named by its bundle (`linux-x86_64-deb`), never by its
# platform alone, so a .deb never finds the AppImage. A .deb or .rpm is
# only offered as a link to the release; the AppImage and the installer
# install themselves.
set -euo pipefail

folder=${1:?usage: scripts/updater-manifest.sh <folder> <version> <release url> <public key>}
version=${2:?missing the version}
release_url=${3:?missing the release url}
public_key=${4:?missing the public key}

if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ || ! $release_url =~ ^https://.+/releases/tag/v$version$ ]]; then
    echo "error: version must be MAJOR.MINOR.PATCH and the release url https://…/releases/tag/v$version" >&2
    exit 1
fi
download_url="${release_url%/tag/v"$version"}/download/v$version"

# A minisign key or signature: base64 of a comment line, then a base64 line
# whose bytes 2–9 are the key's id.
key_id() {
    base64 --decode 2> /dev/null | sed -n 2p | base64 --decode 2> /dev/null | head -c 10 | tail -c 8 | od -An -tx1 | tr -d ' \n'
}
expected_id=$(printf '%s' "$public_key" | key_id)
if [[ ${#expected_id} != 16 ]]; then
    echo "error: the updater's public key isn't a minisign public key" >&2
    exit 1
fi

platforms='{}'
for entry in 'linux-x86_64-appimage:*.AppImage' 'linux-x86_64-deb:*.deb' 'linux-x86_64-rpm:*.rpm' \
    'windows-x86_64-nsis:*-setup.exe'; do
    target=${entry%%:*}
    pattern=${entry#*:}
    # shellcheck disable=SC2206 # the pattern is meant to glob
    packages=("$folder"/$pattern)
    if [[ ${#packages[@]} != 1 || ! -f ${packages[0]} ]]; then
        echo "error: expected one $pattern in $folder" >&2
        exit 1
    fi
    package=${packages[0]}
    if [[ ! -f $package.sig ]]; then
        echo "error: $(basename "$package") has no .sig: the updater's private key wasn't used" >&2
        exit 1
    fi
    signature=$(tr -d '\n\r' < "$package.sig")
    if [[ $(printf '%s' "$signature" | key_id) != "$expected_id" ]]; then
        echo "error: $(basename "$package") is signed with another key than the updater's public key" >&2
        exit 1
    fi
    platforms=$(jq -c --arg target "$target" --arg url "$download_url/$(basename "$package")" --arg signature "$signature" \
        '.[$target] = {signature: $signature, url: $url}' <<< "$platforms")
done

jq -n --arg version "$version" --arg url "$release_url" --arg date "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" \
    --argjson platforms "$platforms" \
    '{version: $version, notes: "localhost \($version)", pub_date: $date, releaseNotesUrl: $url, platforms: $platforms}'
