#!/bin/sh
# Build the release bundle, sign it with a stable local identity, install it to /Applications
# and relaunch it. See docs/DEVELOPMENT.md.
#
# macOS keys the Accessibility and Input Monitoring grants to the code signature. An ad-hoc
# signature changes on every build, so every build would need the three grants again. A
# self-signed "Vox Dev" certificate gives a stable signature; grant once, keep it.
set -eu
cd "$(dirname "$0")/.."

IDENTITY="${VOX_SIGNING_IDENTITY:-Vox Dev}"
APP=src-tauri/target/release/bundle/macos/Vox.app

if ! security find-identity -v -p codesigning | grep -q "\"$IDENTITY\""; then
  echo "No code-signing identity named '$IDENTITY' in the keychain."
  echo "Create one with scripts/make-dev-identity.sh, or set VOX_SIGNING_IDENTITY."
  echo "Continuing with an ad-hoc signature (permissions must be re-granted after each build)."
  IDENTITY="-"
fi

pnpm tauri build --bundles app
codesign --force --deep --options runtime \
  --entitlements src-tauri/Entitlements.plist \
  --identifier com.pixelsmashing.dictation \
  -s "$IDENTITY" "$APP"
codesign -dv "$APP" 2>&1 | grep -E "^(Identifier|Authority|Signature)" || true

# Wait for the old process to be gone before replacing the bundle: LaunchServices refuses to
# open an app it still has registered as exiting (error -600).
RUNNING="Vox.app/Contents/MacOS/vox"
pkill -f "$RUNNING" 2>/dev/null || true
tries=0
while pgrep -f "$RUNNING" >/dev/null 2>&1; do
  tries=$((tries + 1))
  if [ "$tries" -gt 50 ]; then
    echo "Vox is still running after 5 seconds. Quit it from the tray and run this again."
    exit 1
  fi
  sleep 0.1
done

rm -rf /Applications/Vox.app
cp -R "$APP" /Applications/

# The process table and LaunchServices do not always agree on "gone"; one more go is enough.
tries=0
until open /Applications/Vox.app; do
  tries=$((tries + 1))
  if [ "$tries" -ge 3 ]; then
    echo "Installed /Applications/Vox.app, but it did not launch. Open it from Finder."
    exit 1
  fi
  sleep 1
done
echo "Installed and launched /Applications/Vox.app"
