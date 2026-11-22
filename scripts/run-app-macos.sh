#!/bin/bash
# Builds a debug copy of the app as a real .app bundle and opens it.
#
# macOS only gives Location permission (and so Wi-Fi network names) to apps
# opened like this, not to `npm run tauri dev`, so use this script to try the
# app on real Wi-Fi. Each rebuild is a "new" app to macOS: answer the Location
# prompt again, and click "Always Allow" if the Keychain asks.
#
# Usage: scripts/run-app-macos.sh [--simulate] [--settings]
#   --simulate   pretend networks instead of real Wi-Fi
#   --settings   open the settings window straight away
set -euo pipefail
cd "$(dirname "$0")/../app"

[ -d node_modules ] || npm ci
# Local builds skip the signed update files: only the release workflow has the key.
npm run --silent tauri -- build --debug --bundles app \
  --config '{"bundle":{"createUpdaterArtifacts":false}}'

APP="$(cd .. && pwd)/target/debug/bundle/macos/Thunder WiFi Switcher.app"
# Quit a copy that's already running, so the new build (and flags) take effect.
# Wait until it has closed: `open` fails (error -600) while it's still closing.
if pkill -f "$APP/Contents/MacOS/" 2>/dev/null; then
  for _ in $(seq 50); do
    pgrep -f "$APP/Contents/MacOS/" >/dev/null || break
    sleep 0.1
  done
fi
open "$APP" --args "$@"

echo
echo "Opened Thunder WiFi Switcher. Look for its icon in the menu bar."
echo "Logs: ~/Library/Logs/ThunderWiFi/thunderwifi.log"
