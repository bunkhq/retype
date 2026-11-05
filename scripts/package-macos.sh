#!/bin/sh
# Build a self-contained Retype.app and wrap it in a DMG:
#   dist/Retype-<version>-mac-<arm64|intel>.dmg
# The DMG opens to Retype.app beside an Applications shortcut; drag one onto the other.
# Signing/notarization: same RETYPE_SIGN_ID / RETYPE_NOTARY_PROFILE as bundle-macos.sh.
set -eu
cd "$(dirname "$0")/.."
scripts/bundle-macos.sh --with-ffmpeg

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
case "$(uname -m)" in
  arm64) ARCH=arm64 ;;
  *) ARCH=intel ;;
esac
DMG="dist/Retype-$VERSION-mac-$ARCH.dmg"
STAGE=$(mktemp -d)/Retype
mkdir -p "$STAGE"
ditto dist/Retype.app "$STAGE/Retype.app"
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
hdiutil create -quiet -volname "Retype" -srcfolder "$STAGE" -fs HFS+ -format UDZO -imagekey zlib-level=9 "$DMG"

if [ -n "${RETYPE_SIGN_ID:-}" ]; then
  codesign --force --timestamp --sign "$RETYPE_SIGN_ID" "$DMG"
  if [ -n "${RETYPE_NOTARY_PROFILE:-}" ]; then
    xcrun notarytool submit "$DMG" --keychain-profile "$RETYPE_NOTARY_PROFILE" --wait
    xcrun stapler staple "$DMG"
  fi
fi
echo "Built $DMG ($(du -h "$DMG" | cut -f1))"
