#!/bin/bash
# Creates the two keypairs that sign updates, and optionally stores them as
# the repository's secrets and variables. Separate from code signing: these
# prove an update comes from the release workflow, on every platform.
#
#   scripts/create-updater-keys.sh <new folder> [--upload]
#   scripts/create-updater-keys.sh <existing folder> --upload-only
#
# Writes into <new folder>:
#   tauri-updater.key          the Linux and Windows app's private key (minisign), encrypted
#   tauri-updater.key.pub      its public key
#   tauri-updater.password     the private key's password
#   sparkle-private-key        the macOS app's private key (EdDSA), as Sparkle's tools read it
#   sparkle-public-key         its public key
#
# --upload stores them with `gh`: the secrets TAURI_SIGNING_PRIVATE_KEY,
# TAURI_SIGNING_PRIVATE_KEY_PASSWORD and SPARKLE_PRIVATE_KEY, and the
# variables TAURI_UPDATER_PUBLIC_KEY and SPARKLE_PUBLIC_KEY, in the repository
# LOCALHOST_REPO (default marcellosso/localhost). --upload-only stores the
# keys of a folder this script made before, without generating any.
#
# Do this once. Every installed app trusts only the public keys it was built
# with: losing a private key, or replacing a pair, means users must download
# the next version by hand. Back the folder up somewhere safe, outside the repo.
# Needs pnpm install in apps/desktop (for `tauri signer`) and OpenSSL 3.
set -euo pipefail

usage="usage: scripts/create-updater-keys.sh <new folder> [--upload] | <existing folder> --upload-only"
folder=${1:?$usage}
mode=${2:-}
if [[ -n $mode && $mode != --upload && $mode != --upload-only ]]; then
    echo "$usage" >&2
    exit 1
fi
# Explicit, so a clone with several remotes doesn't make `gh` ask which one.
repo=${LOCALHOST_REPO:-marcellosso/localhost}
root=$(cd "$(dirname "$0")/.." && pwd)
files=(tauri-updater.key tauri-updater.key.pub tauri-updater.password sparkle-private-key sparkle-public-key)

upload() {
    echo "==> Storing them in $(gh repo view "$repo" --json nameWithOwner -q .nameWithOwner)" >&2
    gh secret set TAURI_SIGNING_PRIVATE_KEY -R "$repo" < "$folder/tauri-updater.key"
    gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD -R "$repo" < "$folder/tauri-updater.password"
    gh secret set SPARKLE_PRIVATE_KEY -R "$repo" < "$folder/sparkle-private-key"
    gh variable set TAURI_UPDATER_PUBLIC_KEY -R "$repo" < "$folder/tauri-updater.key.pub"
    gh variable set SPARKLE_PUBLIC_KEY -R "$repo" < "$folder/sparkle-public-key"
}

if [[ $mode == --upload-only ]]; then
    for file in "${files[@]}"; do
        if [[ ! -s $folder/$file ]]; then
            echo "error: $folder/$file is missing; --upload-only needs a folder this script made" >&2
            exit 1
        fi
    done
    # The pair the release workflow checks, before anything is stored.
    if [[ $("$root/scripts/sparkle-public-key.sh" < "$folder/sparkle-private-key") != "$(tr -d ' \n\r' < "$folder/sparkle-public-key")" ]]; then
        echo "error: $folder/sparkle-public-key isn't the pair of sparkle-private-key" >&2
        exit 1
    fi
    upload
    exit 0
fi

if [[ -e $folder ]]; then
    echo "error: $folder already exists; pick a new folder, or store its keys with --upload-only" >&2
    exit 1
fi
mkdir -p "$folder"
folder=$(cd "$folder" && pwd)
chmod 700 "$folder"

echo "==> The Linux and Windows app's key (Tauri)" >&2
openssl rand -base64 24 | tr -d '\n' > "$folder/tauri-updater.password"
(cd "$root/apps/desktop" && pnpm -s tauri signer generate --ci \
    -w "$folder/tauri-updater.key" -p "$(cat "$folder/tauri-updater.password")" > /dev/null)

echo "==> The macOS app's key (Sparkle)" >&2
# A raw Ed25519 seed, base64: what Sparkle's `generate_keys -x` exports,
# made without touching the login keychain.
openssl=openssl
for candidate in /opt/homebrew/opt/openssl@3/bin/openssl /usr/local/opt/openssl@3/bin/openssl; do
    [[ -x $candidate ]] && openssl=$candidate && break
done
"$openssl" genpkey -algorithm ed25519 -outform DER | tail -c 32 | base64 > "$folder/sparkle-private-key"
OPENSSL=$openssl "$root/scripts/sparkle-public-key.sh" < "$folder/sparkle-private-key" > "$folder/sparkle-public-key"
chmod 600 "$folder"/*

echo "Tauri public key:   $(cat "$folder/tauri-updater.key.pub")" >&2
echo "Sparkle public key: $(cat "$folder/sparkle-public-key")" >&2

if [[ $mode == --upload ]]; then
    upload
fi
echo "Back up $folder (a password manager or an encrypted disk), never in the repository." >&2
