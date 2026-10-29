#!/bin/bash
# Writes the Sparkle appcast for one release: one item, the DMG, which the
# macOS app checks for updates (apps/macos/App/Updates.swift).
#
#   scripts/appcast.sh <dmg> <version> <build number> <ed signature> <release url>
#
# <ed signature> is what `sign_update -p <dmg>` prints; <release url> the
# release's page, e.g. https://github.com/marcellosso/localhost/releases/tag/v0.5.0,
# whose download folder holds the DMG. Sparkle compares the build number
# with the app's CFBundleVersion, and shows the version. Prints the XML;
# the release workflow then signs it (`sign_update`), as apps require.
set -euo pipefail

dmg=${1:?usage: scripts/appcast.sh <dmg> <version> <build number> <ed signature> <release url>}
version=${2:?missing the version}
build_number=${3:?missing the build number}
signature=${4:?missing the EdDSA signature}
release_url=${5:?missing the release url}

if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ || ! $build_number =~ ^[0-9]+$ ]]; then
    echo "error: version must be MAJOR.MINOR.PATCH and the build number a number" >&2
    exit 1
fi
if [[ ! $signature =~ ^[A-Za-z0-9+/=]+$ || ! $release_url =~ ^https://[^\"\<\>\&]+/releases/tag/v$version$ ]]; then
    echo "error: the signature must be base64 and the release url https://…/releases/tag/v$version" >&2
    exit 1
fi

name=$(basename "$dmg")
length=$(wc -c < "$dmg" | tr -d ' ')
download_url="${release_url%/tag/v"$version"}/download/v$version/$name"
date=$(LC_ALL=C date -u '+%a, %d %b %Y %H:%M:%S +0000')

cat <<EOF
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>localhost</title>
    <item>
      <title>localhost $version</title>
      <pubDate>$date</pubDate>
      <sparkle:version>$build_number</sparkle:version>
      <sparkle:shortVersionString>$version</sparkle:shortVersionString>
      <sparkle:minimumSystemVersion>15.0</sparkle:minimumSystemVersion>
      <sparkle:fullReleaseNotesLink>$release_url</sparkle:fullReleaseNotesLink>
      <enclosure url="$download_url" length="$length" type="application/octet-stream" sparkle:edSignature="$signature"/>
    </item>
  </channel>
</rss>
EOF
