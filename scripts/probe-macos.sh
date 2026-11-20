#!/bin/bash
# Runs the macOS Wi-Fi probe (crates/wifi-macos/examples/probe.rs) as a small
# app bundle. macOS only shows the Location prompt to real apps: a plain
# command-line tool started from Terminal is never asked, so it never sees
# Wi-Fi network names.
#
# Usage: scripts/probe-macos.sh <command> [args]
#        scripts/probe-macos.sh help      (lists the commands)
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --quiet -p wifi-macos --example probe

APP="target/probe/Thunder WiFi Probe.app"
mkdir -p "$APP/Contents/MacOS"
cp target/debug/examples/probe "$APP/Contents/MacOS/probe"
cp crates/wifi-macos/examples/probe-Info.plist "$APP/Contents/Info.plist"
codesign --force --sign - "$APP" >/dev/null 2>&1

if ! TTY=$(tty 2>/dev/null); then
  # No terminal (for example in CI): run the binary directly. It works, but
  # macOS won't grant it Location permission this way.
  exec "$APP/Contents/MacOS/probe" "$@"
fi

# Launch it like a double-clicked app, with this terminal as its input and
# output. Ctrl-C stops the probe as well as this script.
stop_probe() { pkill -f "$APP/Contents/MacOS/probe" 2>/dev/null || true; }
trap stop_probe INT TERM
# (open prints "Unable to block on application" when the probe finishes
# before it can wait on it; that's harmless, so it's filtered out.)
open -W -n --stdin "$TTY" --stdout "$TTY" --stderr "$TTY" "$APP" --args "$@" \
  2> >(grep -v "Unable to block on application" >&2)
