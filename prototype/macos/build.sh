#!/bin/bash
# Builds DormWiFi.app and installs it into /Applications.
set -euo pipefail
cd "$(dirname "$0")"

APP="build/DormWiFi.app"

if ! command -v swiftc >/dev/null; then
  echo "Swift compiler not found. Run:  xcode-select --install   then try again."
  exit 1
fi

echo "Building…"
rm -rf build
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
swiftc -O main.swift -o "$APP/Contents/MacOS/DormWiFi" \
  -framework AppKit -framework CoreWLAN -framework CoreLocation \
  -framework Security -framework ServiceManagement
cp Info.plist "$APP/Contents/Info.plist"
codesign --force --sign - "$APP"

echo "Installing to /Applications…"
pkill -x DormWiFi 2>/dev/null || true
rm -rf /Applications/DormWiFi.app
cp -R "$APP" /Applications/
open /Applications/DormWiFi.app

echo "Done! Look for the Wi-Fi icon with a number in your menu bar."
