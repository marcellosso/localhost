#!/bin/bash
# Prints the public half of a Sparkle (EdDSA) private key, the SUPublicEDKey
# that apps verify updates with. The release workflow checks with it that
# the SPARKLE_PRIVATE_KEY secret and the SPARKLE_PUBLIC_KEY variable are a
# pair, before signing anything.
#
#   scripts/sparkle-public-key.sh < <private key file>
#
# The key is what Sparkle's `generate_keys -x` exports and `sign_update
# --ed-key-file` reads: base64 of the 32-byte seed, or of the 96 bytes of
# older keys, which end with the public key. Needs OpenSSL 3 for a seed
# (Homebrew's `openssl@3` on macOS; the system LibreSSL has no Ed25519).
set -euo pipefail

openssl=openssl
for candidate in "${OPENSSL:-}" /opt/homebrew/opt/openssl@3/bin/openssl /usr/local/opt/openssl@3/bin/openssl; do
    if [[ -n $candidate && -x $candidate ]]; then
        openssl=$candidate
        break
    fi
done

key=$(mktemp)
trap 'rm -f "$key"' EXIT
tr -d ' \n\r' | base64 --decode > "$key" 2> /dev/null || {
    echo "error: the private key isn't base64" >&2
    exit 1
}

case $(wc -c < "$key" | tr -d ' ') in
    32)
        # The seed as a PKCS#8 Ed25519 key: a fixed 16-byte header, then the seed.
        { printf '\x30\x2e\x02\x01\x00\x30\x05\x06\x03\x2b\x65\x70\x04\x22\x04\x20'; cat "$key"; } |
            "$openssl" pkey -inform DER -pubout -outform DER | tail -c 32 | base64
        ;;
    96) tail -c 32 "$key" | base64 ;;
    *)
        echo "error: not a Sparkle private key (32 or 96 bytes once decoded)" >&2
        exit 1
        ;;
esac
