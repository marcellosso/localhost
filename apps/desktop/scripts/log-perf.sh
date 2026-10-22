#!/bin/bash
# Measures the log view under a very chatty Project: builds the app,
# runs it in its measurement mode (src-tauri/src/perf.rs), and prints the
# frame times the UI recorded with the memory of the app's process tree.
#
#   scripts/log-perf.sh [services] [lines per second per service] [seconds]
#
# Defaults: 4 services, 1000 lines/s each, 30 s. Run it on Linux in a desktop
# session: WebKitGTK is the webview to measure. On Linux the web process is a
# child of the app, so the memory includes it; on macOS WKWebView's web
# process is not, so the memory there is the app's alone.
set -euo pipefail

cd "$(dirname "$0")/.."
services=${1:-4}
rate=${2:-1000}
seconds=${3:-30}
binary=src-tauri/target/release/localhost-app

if [[ ${SKIP_BUILD:-} != 1 ]]; then
    echo "==> Building the app (release)" >&2
    pnpm install --frozen-lockfile >&2
    pnpm tauri build --no-bundle >&2
fi

# Every process under $1, itself included.
tree() {
    echo "$1"
    for child in $(pgrep -P "$1" || true); do tree "$child"; done
}

echo "==> Measuring $services services × $rate lines/s for ${seconds}s" >&2
output=$(mktemp)
LOCALHOST_PERF=1 LOCALHOST_PERF_SERVICES=$services LOCALHOST_PERF_RATE=$rate LOCALHOST_PERF_SECONDS=$seconds \
    "$binary" >"$output" 2>&1 &
app=$!

peak=0
total=0
samples=0
while kill -0 "$app" 2>/dev/null; do
    # The Services' shells are children too: leave out everything below them.
    pids=$(tree "$app" | while read -r pid; do
        name=$(ps -o comm= -p "$pid" 2>/dev/null || true)
        [[ $name == *sh ]] || echo "$pid"
    done | paste -sd, -)
    rss=$(ps -o rss= -p "$pids" 2>/dev/null | awk '{ sum += $1 } END { print sum + 0 }')
    if (( rss > 0 )); then
        (( rss > peak )) && peak=$rss
        total=$(( total + rss ))
        samples=$(( samples + 1 ))
    fi
    sleep 1
done
wait "$app" || true

report=$(grep '^LOCALHOST_PERF_REPORT ' "$output" | sed 's/^LOCALHOST_PERF_REPORT //' || true)
if [[ -z $report ]]; then
    echo "error: the app ended without a report. Its output:" >&2
    cat "$output" >&2
    exit 1
fi
rm -f "$output"
echo "$report"
echo "memory: peak $(( peak / 1024 )) MB, average $(( total / (samples > 0 ? samples : 1) / 1024 )) MB (app process tree, without the Services)"
