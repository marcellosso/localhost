#!/bin/bash
# Builds the Rust core (core/localhost-ffi) for the macOS app and generates
# its Swift bindings, and the `localhost` command the app bundles. Xcode and
# CI both call this.
#
#   scripts/build-core.sh [debug|release]   build (default: release)
#   scripts/build-core.sh --check           fail if the output is missing or
#                                           older than core/'s sources
#   scripts/build-core.sh --xcode           the scheme's build pre-action: the
#                                           profile from $CONFIGURATION, output
#                                           in Generated/build.log
#
# debug builds this Mac's architecture only, for quick local builds.
# release builds a universal (arm64 + x86_64) library, as shipped.
#
# Output, inside the Swift package that wraps it (git-ignored):
#   apps/macos/LocalhostCoreFFI/Generated/LocalhostCoreFFI.xcframework
#   apps/macos/LocalhostCoreFFI/Sources/LocalhostCore/Generated/LocalhostCore+FFI.swift
#   apps/macos/LocalhostCoreFFI/Generated/bin/localhost   (copied into the app's Contents/Helpers)
#
# Needs a Rust toolchain (rustup); see CONTRIBUTING.md.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CORE="$ROOT/core"
PACKAGE="$ROOT/apps/macos/LocalhostCoreFFI"
GENERATED="$PACKAGE/Generated"
SWIFT_OUT="$PACKAGE/Sources/LocalhostCore/Generated"
STAMP="$GENERATED/stamp"

# Xcode runs scripts without the login shell's PATH.
if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
fi

# A hash of everything the output is built from.
sources_hash() {
    (
        cd "$ROOT"
        { find core -path core/target -prune -o -type f \( -name '*.rs' -o -name '*.toml' -o -name '*.c' -o -name Cargo.lock \) -print
          find skills -type f; } |
            LC_ALL=C sort | xargs shasum
        shasum scripts/build-core.sh
        # The version the bundled command reports (scripts/build-release.sh).
        echo "${LOCALHOST_VERSION:-}"
    ) | shasum | cut -d' ' -f1
}

if [ "${1:-}" = "--check" ]; then
    if [ ! -f "$STAMP" ] || [ "$(sed -n 1p "$STAMP")" != "$(sources_hash)" ]; then
        echo "error: The Rust core is missing or out of date. Run scripts/build-core.sh." >&2
        exit 1
    fi
    exit 0
fi

PROFILE="${1:-release}"
if [ "$PROFILE" = "--xcode" ]; then
    # Xcode doesn't show what a pre-action prints, so it goes to a log; the
    # "Check the Rust core" build phase reads the status written below and
    # points there.
    mkdir -p "$GENERATED"
    exec >"$GENERATED/build.log" 2>&1
    if [ "${CONFIGURATION:-Debug}" = Release ]; then PROFILE=release; else PROFILE=debug; fi
fi

# What the "Check the Rust core" build phase reads: `ok`, or why not.
STATUS="$GENERATED/status"
mkdir -p "$GENERATED"
rm -f "$STATUS"
WORK=""
finish() {
    local code=$?
    if [ "$code" -eq 0 ]; then
        echo ok >"$STATUS"
    else
        echo "scripts/build-core.sh $PROFILE failed (exit $code)" >"$STATUS"
    fi
    if [ -n "$WORK" ]; then rm -rf "$WORK"; fi
    return "$code"
}
trap finish EXIT

case "$PROFILE" in
    release) TARGETS=(aarch64-apple-darwin x86_64-apple-darwin) ;;
    debug)
        case "$(uname -m)" in
            arm64) TARGETS=(aarch64-apple-darwin) ;;
            *) TARGETS=(x86_64-apple-darwin) ;;
        esac
        ;;
    *)
        echo "usage: $0 [debug|release] | --check | --xcode" >&2
        exit 2
        ;;
esac

if ! command -v cargo >/dev/null; then
    echo "error: cargo not found. Install Rust with rustup (see CONTRIBUTING.md)." >&2
    exit 1
fi

HASH="$(sources_hash)"
if [ -d "$GENERATED/LocalhostCoreFFI.xcframework" ] && [ -x "$GENERATED/bin/localhost" ] && [ -f "$STAMP" ] && [ "$(cat "$STAMP")" = "$HASH"$'\n'"$PROFILE" ]; then
    echo "Rust core is up to date ($PROFILE)."
    exit 0
fi

if command -v rustup >/dev/null; then
    rustup target add "${TARGETS[@]}" >/dev/null 2>&1 || rustup target add "${TARGETS[@]}"
fi

# Matches the app's deployment target, so the linker doesn't warn about
# objects built for a newer macOS.
export MACOSX_DEPLOYMENT_TARGET=15.0

CARGO_PROFILE_FLAG=()
[ "$PROFILE" = release ] && CARGO_PROFILE_FLAG=(--release)

LIBRARIES=()
COMMANDS=()
for target in "${TARGETS[@]}"; do
    echo "Building localhost-ffi and the localhost command for $target ($PROFILE)…"
    cargo build --manifest-path "$CORE/Cargo.toml" -p localhost-ffi --lib --target "$target" ${CARGO_PROFILE_FLAG[@]+"${CARGO_PROFILE_FLAG[@]}"}
    cargo build --manifest-path "$CORE/Cargo.toml" -p localhost-cli --bin localhost --target "$target" ${CARGO_PROFILE_FLAG[@]+"${CARGO_PROFILE_FLAG[@]}"}
    LIBRARIES+=("$CORE/target/$target/$PROFILE/liblocalhost_ffi.a")
    COMMANDS+=("$CORE/target/$target/$PROFILE/localhost")
done

WORK="$(mktemp -d)"

lipo -create "${LIBRARIES[@]}" -output "$WORK/liblocalhost_ffi.a"
lipo -create "${COMMANDS[@]}" -output "$WORK/localhost"

echo "Generating the Swift bindings…"
# It reads the crate's uniffi.toml through `cargo metadata`, run from core/.
(cd "$CORE" && cargo run -p localhost-ffi --bin uniffi-bindgen --quiet -- \
    generate --library "${LIBRARIES[0]}" --language swift --out-dir "$WORK/bindings")

mkdir -p "$WORK/headers"
cp "$WORK/bindings/LocalhostCoreFFI.h" "$WORK/headers/"
cp "$WORK/bindings/LocalhostCoreFFI.modulemap" "$WORK/headers/module.modulemap"

rm -rf "$GENERATED/LocalhostCoreFFI.xcframework"
mkdir -p "$GENERATED" "$SWIFT_OUT"
xcodebuild -create-xcframework \
    -library "$WORK/liblocalhost_ffi.a" -headers "$WORK/headers" \
    -output "$GENERATED/LocalhostCoreFFI.xcframework" >/dev/null

# Only replaced when it changed, so Xcode doesn't recompile the package.
if ! cmp -s "$WORK/bindings/localhost_core.swift" "$SWIFT_OUT/LocalhostCore+FFI.swift"; then
    cp "$WORK/bindings/localhost_core.swift" "$SWIFT_OUT/LocalhostCore+FFI.swift"
fi

mkdir -p "$GENERATED/bin"
mv "$WORK/localhost" "$GENERATED/bin/localhost"

printf '%s\n%s\n' "$HASH" "$PROFILE" >"$STAMP"
echo "Rust core built: $GENERATED/LocalhostCoreFFI.xcframework ($(lipo -archs "$WORK/liblocalhost_ffi.a"))."
