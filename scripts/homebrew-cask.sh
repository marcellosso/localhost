#!/bin/bash
# Writes the Homebrew cask for one release, from packaging/homebrew/localhost.rb:
# the tap's Casks/localhost.rb, which `brew install --cask marcellosso/tap/localhost`
# reads. The release workflow runs it once the release is published.
#
#   scripts/homebrew-cask.sh <version> <sha256 of localhost-<version>.dmg>
#
# Prints the cask.
set -euo pipefail

version=${1:?usage: scripts/homebrew-cask.sh <version> <sha256 of the DMG>}
sha256=${2:?missing the sha256 of the DMG}

if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ || ! $sha256 =~ ^[0-9a-f]{64}$ ]]; then
    echo "error: version must be MAJOR.MINOR.PATCH and the sha256 64 lowercase hex digits" >&2
    exit 1
fi

sed -e "s/@VERSION@/$version/" -e "s/@SHA256@/$sha256/" "$(dirname "$0")/../packaging/homebrew/localhost.rb"
