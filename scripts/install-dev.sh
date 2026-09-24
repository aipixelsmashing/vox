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

pkill -f "Vox.app/Contents/MacOS/vox" 2>/dev/null || true
rm -rf /Applications/Vox.app
cp -R "$APP" /Applications/
open /Applications/Vox.app
echo "Installed and launched /Applications/Vox.app"
