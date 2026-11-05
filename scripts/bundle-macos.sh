#!/bin/sh
# Build dist/Retype.app from a release build.
#
#   scripts/bundle-macos.sh                 app only; uses ffmpeg from PATH / Homebrew
#   scripts/bundle-macos.sh --with-ffmpeg   also embed ffmpeg + its libraries (self-contained)
#
# Signing and notarization (optional, needs an Apple Developer account):
#   RETYPE_SIGN_ID="Developer ID Application: Name (TEAMID)"   sign with hardened runtime
#   RETYPE_NOTARY_PROFILE=retype   notarize and staple, using a profile saved with
#                                  `xcrun notarytool store-credentials retype`
# Without RETYPE_SIGN_ID the app is ad-hoc signed and runs only on this Mac
# (or after right-click > Open elsewhere).
set -eu
cd "$(dirname "$0")/.."

WITH_FFMPEG=0
for arg in "$@"; do
  case "$arg" in
    --with-ffmpeg) WITH_FFMPEG=1 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

cargo build --release
APP=dist/Retype.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/retype "$APP/Contents/MacOS/retype"

# Icon drawn per size by `cargo run --example make_icons` (Apple's icon grid).
cp assets/icons/Retype.icns "$APP/Contents/Resources/Retype.icns"

# Document types, so Finder offers "Open With > Retype" and the Dock icon takes drops.
TYPES=$( { printf 'public.movie\npublic.audio\npublic.image\npublic.folder\n'; "$APP/Contents/MacOS/retype" utis; } | sed 's|.*|<string>&</string>|' | tr -d '\n')

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Retype</string>
<key>CFBundleDisplayName</key><string>Retype</string>
<key>CFBundleIdentifier</key><string>co.4nchor.retype</string>
<key>CFBundleExecutable</key><string>retype</string>
<key>CFBundleIconFile</key><string>Retype</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$VERSION</string>
<key>CFBundleVersion</key><string>$VERSION</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
<key>NSAppleEventsUsageDescription</key><string>Retype reads the files you have selected in Finder when you press the convert shortcut.</string>
<key>CFBundleDocumentTypes</key><array><dict>
<key>CFBundleTypeName</key><string>Convertible file</string>
<key>CFBundleTypeRole</key><string>Viewer</string>
<key>LSHandlerRank</key><string>Alternate</string>
<key>LSItemContentTypes</key><array>$TYPES</array>
</dict></array>
</dict></plist>
PLIST

if [ "$WITH_FFMPEG" = 1 ]; then
  python3 scripts/embed_ffmpeg.py "$APP"
fi

SIGN_ID="${RETYPE_SIGN_ID:--}"
if [ "$SIGN_ID" = "-" ]; then
  codesign --force --deep --sign - "$APP" 2>/dev/null || true
else
  ENT=$(mktemp)
  cat > "$ENT" <<ENTITLEMENTS
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>com.apple.security.automation.apple-events</key><true/>
</dict></plist>
ENTITLEMENTS
  # Inside out: libraries and helpers first, then the app.
  if [ -d "$APP/Contents/Frameworks" ]; then
    find "$APP/Contents/Frameworks" -name '*.dylib' -exec codesign --force --timestamp --options runtime --sign "$SIGN_ID" {} \;
  fi
  for t in ffmpeg ffprobe; do
    [ -f "$APP/Contents/Resources/$t" ] && codesign --force --timestamp --options runtime --sign "$SIGN_ID" "$APP/Contents/Resources/$t"
  done
  codesign --force --timestamp --options runtime --entitlements "$ENT" --sign "$SIGN_ID" "$APP"
  codesign --verify --strict --verbose=2 "$APP"
  if [ -n "${RETYPE_NOTARY_PROFILE:-}" ]; then
    ZIP=dist/Retype.zip
    ditto -c -k --keepParent "$APP" "$ZIP"
    xcrun notarytool submit "$ZIP" --keychain-profile "$RETYPE_NOTARY_PROFILE" --wait
    xcrun stapler staple "$APP"
    rm -f "$ZIP"
  fi
fi
echo "Built $APP"
