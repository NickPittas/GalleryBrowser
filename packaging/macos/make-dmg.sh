#!/usr/bin/env bash
# Assemble GalleryBrowser.app (universal) and package it as a .dmg. Run on macOS from the repo root.
#
#   packaging/macos/make-dmg.sh <version> <arm64-binary> <x86_64-binary> <output-dir>
#
# Needs: lipo, iconutil, hdiutil, codesign (Xcode CLT) and rsvg-convert (brew install librsvg).
set -euo pipefail

if [ "$#" -ne 4 ]; then
  echo "usage: $0 <version> <arm64-binary> <x86_64-binary> <output-dir>" >&2
  exit 2
fi

VERSION="$1"
ARM_BIN="$2"
X86_BIN="$3"
OUT_DIR="$4"

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SVG="$ROOT/flatpak/io.github.nickpittas.GalleryBrowser.svg"
PLIST_TEMPLATE="$ROOT/packaging/macos/Info.plist"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

APP="$WORK/GalleryBrowser.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$OUT_DIR"

# Universal binary.
lipo -create "$ARM_BIN" "$X86_BIN" -output "$APP/Contents/MacOS/gallerybrowser"
chmod 755 "$APP/Contents/MacOS/gallerybrowser"
lipo -info "$APP/Contents/MacOS/gallerybrowser"

# Info.plist with the release version.
sed "s/@VERSION@/$VERSION/g" "$PLIST_TEMPLATE" > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"

# Icon: SVG -> iconset -> .icns.
ICONSET="$WORK/GalleryBrowser.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  double=$((size * 2))
  rsvg-convert -w "$size" -h "$size" "$SVG" -o "$ICONSET/icon_${size}x${size}.png"
  rsvg-convert -w "$double" -h "$double" "$SVG" -o "$ICONSET/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/GalleryBrowser.icns"

cp "$ROOT/NOTICE" "$APP/Contents/Resources/NOTICE"
if [ -f "$ROOT/LICENSE" ]; then
  cp "$ROOT/LICENSE" "$APP/Contents/Resources/LICENSE"
fi

# Ad-hoc signature (not a Developer ID signature, no notarization).
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"

# DMG with an /Applications drop target.
STAGE="$WORK/dmg"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
DMG="$OUT_DIR/GalleryBrowser-${VERSION}-macos-universal.dmg"
rm -f "$DMG"
hdiutil create -volname "GalleryBrowser $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
echo "wrote $DMG"
