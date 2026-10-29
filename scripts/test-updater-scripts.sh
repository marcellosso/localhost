#!/bin/bash
# Checks the scripts the release workflow writes the update feeds with, using
# throwaway keys: latest.json (scripts/updater-manifest.sh), appcast.xml
# (scripts/appcast.sh) and the Sparkle key check (scripts/sparkle-public-key.sh).
# Needs pnpm install in apps/desktop, jq, python3 and OpenSSL 3. CI runs it.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail() {
    echo "FAIL: $*" >&2
    exit 1
}
tauri() { (cd "$root/apps/desktop" && pnpm -s tauri "$@" > /dev/null 2>&1); }

release=https://github.com/marcellosso/localhost/releases/tag/v0.5.0
download=https://github.com/marcellosso/localhost/releases/download/v0.5.0

echo "latest.json"
tauri signer generate --ci -w "$work/updater.key" -p secret
tauri signer generate --ci -w "$work/other.key" -p secret
mkdir "$work/packages"
for name in localhost-app_0.5.0_amd64.AppImage localhost-app_0.5.0_amd64.deb localhost-app-0.5.0-1.x86_64.rpm \
    localhost_0.5.0_x64-setup.exe; do
    echo "$name" > "$work/packages/$name"
    TAURI_SIGNING_PRIVATE_KEY_PATH=$work/updater.key TAURI_SIGNING_PRIVATE_KEY_PASSWORD=secret \
        tauri signer sign --app-version 0.5.0 "$work/packages/$name"
done
manifest=$("$root/scripts/updater-manifest.sh" "$work/packages" 0.5.0 "$release" "$(cat "$work/updater.key.pub")")
[[ $(jq -r .version <<< "$manifest") == 0.5.0 ]] || fail "the version"
[[ $(jq -r .releaseNotesUrl <<< "$manifest") == "$release" ]] || fail "the release notes"
[[ $(jq -r '.platforms | keys | join(" ")' <<< "$manifest") == \
    "linux-x86_64-appimage linux-x86_64-deb linux-x86_64-rpm windows-x86_64-nsis" ]] || fail "the platforms"
[[ $(jq -r '.platforms["windows-x86_64-nsis"].url' <<< "$manifest") == "$download/localhost_0.5.0_x64-setup.exe" ]] ||
    fail "the installer's url"
[[ $(jq -r '.platforms["linux-x86_64-appimage"].signature' <<< "$manifest") == \
    "$(tr -d '\n' < "$work/packages/localhost-app_0.5.0_amd64.AppImage.sig")" ]] || fail "the AppImage's signature"
if "$root/scripts/updater-manifest.sh" "$work/packages" 0.5.0 "$release" "$(cat "$work/other.key.pub")" 2> /dev/null; then
    fail "packages signed with another key are accepted"
fi
rm "$work/packages/localhost-app_0.5.0_amd64.deb.sig"
if "$root/scripts/updater-manifest.sh" "$work/packages" 0.5.0 "$release" "$(cat "$work/updater.key.pub")" 2> /dev/null; then
    fail "a package without a signature is accepted"
fi

echo "Sparkle's key"
openssl genpkey -algorithm ed25519 -out "$work/sparkle.pem"
openssl pkey -in "$work/sparkle.pem" -outform DER | tail -c 32 | base64 > "$work/sparkle-private-key"
expected=$(openssl pkey -in "$work/sparkle.pem" -pubout -outform DER | tail -c 32 | base64)
[[ $("$root/scripts/sparkle-public-key.sh" < "$work/sparkle-private-key") == "$expected" ]] || fail "the derived public key"
if echo "not a key" | "$root/scripts/sparkle-public-key.sh" 2> /dev/null; then
    fail "a wrong private key is accepted"
fi

echo "appcast.xml"
head -c 1000 /dev/zero > "$work/localhost-0.5.0.dmg"
"$root/scripts/appcast.sh" "$work/localhost-0.5.0.dmg" 0.5.0 42 c2lnbmF0dXJl "$release" > "$work/appcast.xml"
python3 -I - "$work/appcast.xml" "$download/localhost-0.5.0.dmg" "$release" <<'EOF'
import sys
import xml.etree.ElementTree as tree

sparkle = "{http://www.andymatuschak.org/xml-namespaces/sparkle}"
item = tree.parse(sys.argv[1]).find("channel/item")
enclosure = item.find("enclosure")
assert item.findtext(sparkle + "version") == "42", "the build number"
assert item.findtext(sparkle + "shortVersionString") == "0.5.0", "the version"
assert item.findtext(sparkle + "fullReleaseNotesLink") == sys.argv[3], "the release notes"
assert enclosure.get("url") == sys.argv[2], "the DMG's url"
assert enclosure.get("length") == "1000", "the DMG's length"
assert enclosure.get(sparkle + "edSignature") == "c2lnbmF0dXJl", "the signature"
EOF
if "$root/scripts/appcast.sh" "$work/localhost-0.5.0.dmg" 0.5.0 42 c2lnbmF0dXJl "${release/0.5.0/0.4.0}" > /dev/null 2>&1; then
    fail "another version's release url is accepted"
fi

echo "ok"
