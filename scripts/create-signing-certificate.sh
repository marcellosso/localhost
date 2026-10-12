#!/bin/bash
# Creates the self-signed certificate that release builds are signed with, and
# optionally stores it as the repository's signing secrets.
#
#   scripts/create-signing-certificate.sh <new folder> [--upload]
#
# Writes into <new folder>:
#   localhost-signing.p12           the certificate and its private key
#   localhost-signing.password      the .p12's password
#   localhost-signing.cer           the certificate alone, to compare later
#
# --upload stores the .p12 (base64) and its password as the GitHub secrets
# SIGNING_CERTIFICATE_P12 and SIGNING_CERTIFICATE_PASSWORD, with `gh`.
#
# Do this once. macOS's Keychain trusts a release to read Secrets because it is
# signed with this exact certificate: a new certificate makes every user's Mac
# ask again for each Secret. Back the folder up somewhere safe, outside the repo.
set -euo pipefail

folder=${1:?usage: scripts/create-signing-certificate.sh <new folder> [--upload]}
upload=${2:-}
name="localhost Release Signing"

if [[ -e $folder ]]; then
    echo "error: $folder already exists; pick a new folder so nothing is overwritten" >&2
    exit 1
fi
mkdir -p "$folder"
chmod 700 "$folder"
cd "$folder"

# 20 years: users' Keychains keep trusting releases for as long as this lasts.
openssl req -x509 -newkey rsa:3072 -sha256 -days 7305 -nodes \
    -subj "/CN=$name" \
    -addext "basicConstraints=critical,CA:false" \
    -addext "keyUsage=critical,digitalSignature" \
    -addext "extendedKeyUsage=critical,codeSigning" \
    -keyout key.pem -out cert.pem 2>/dev/null

password=$(openssl rand -base64 24)
# SHA-1/3DES encryption, which macOS's `security import` reads (OpenSSL 3's default doesn't).
openssl pkcs12 -export -name "$name" -inkey key.pem -in cert.pem \
    -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg sha1 \
    -passout "pass:$password" -out localhost-signing.p12
openssl x509 -in cert.pem -outform der -out localhost-signing.cer
printf '%s' "$password" > localhost-signing.password
rm key.pem cert.pem
chmod 600 localhost-signing.*

echo "Created $(pwd)/localhost-signing.p12"
openssl x509 -inform der -in localhost-signing.cer -noout -subject -enddate -fingerprint -sha1

if [[ $upload == --upload ]]; then
    base64 -i localhost-signing.p12 | gh secret set SIGNING_CERTIFICATE_P12
    gh secret set SIGNING_CERTIFICATE_PASSWORD < localhost-signing.password
    echo "Stored SIGNING_CERTIFICATE_P12 and SIGNING_CERTIFICATE_PASSWORD in $(gh repo view --json nameWithOwner --jq .nameWithOwner)"
fi
