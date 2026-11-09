#!/usr/bin/env bash
# Assemble GalleryBrowser.app (universal or single-architecture) and package it as a .dmg.
# Run on macOS from the repo root.
#
# Usage:
#   packaging/macos/make-dmg.sh <version> <arm64-binary> <x86_64-binary> <output-dir>   # Universal
#   packaging/macos/make-dmg.sh <version> <binary> <output-dir>                         # Single architecture
#
# Needs: iconutil, hdiutil, codesign (Xcode CLT).
# Optional: lipo (for universal builds), rsvg-convert (brew install librsvg; falls back to native sips).
set -euo pipefail

if [ "$#" -eq 4 ]; then
  VERSION="$1"
  BIN1="$2"
  BIN2="$3"
  OUT_DIR="$4"
elif [ "$#" -eq 3 ]; then
  VERSION="$1"
  BIN1="$2"
  BIN2=""
  OUT_DIR="$3"
else
  echo "usage: $0 <version> <binary-or-arm64> [x86_64-binary] <output-dir>" >&2
  exit 2
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SVG="$ROOT/flatpak/io.github.nickpittas.GalleryBrowser.svg"
PNG="$ROOT/flatpak/io.github.nickpittas.GalleryBrowser.png"
PLIST_TEMPLATE="$ROOT/packaging/macos/Info.plist"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

APP="$WORK/GalleryBrowser.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$OUT_DIR"

# Binary: universal (lipo) or single architecture.
if [ -n "$BIN2" ]; then
  lipo -create "$BIN1" "$BIN2" -output "$APP/Contents/MacOS/gallerybrowser"
  DMG_ARCH="universal"
else
  cp "$BIN1" "$APP/Contents/MacOS/gallerybrowser"
  ARCHS="$(lipo -archs "$APP/Contents/MacOS/gallerybrowser" 2>/dev/null || uname -m)"
  if [ "$(echo "$ARCHS" | wc -w)" -gt 1 ]; then
    DMG_ARCH="universal"
  else
    DMG_ARCH="$ARCHS"
  fi
fi
chmod 755 "$APP/Contents/MacOS/gallerybrowser"
lipo -info "$APP/Contents/MacOS/gallerybrowser"

# Info.plist with the release version.
sed "s/@VERSION@/$VERSION/g" "$PLIST_TEMPLATE" > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"

# Icon: SVG/PNG -> iconset -> .icns.
ICONSET="$WORK/GalleryBrowser.iconset"
mkdir -p "$ICONSET"
if command -v rsvg-convert >/dev/null 2>&1 && [ -f "$SVG" ]; then
  for size in 16 32 128 256 512; do
    double=$((size * 2))
    rsvg-convert -w "$size" -h "$size" "$SVG" -o "$ICONSET/icon_${size}x${size}.png"
    rsvg-convert -w "$double" -h "$double" "$SVG" -o "$ICONSET/icon_${size}x${size}@2x.png"
  done
elif [ -f "$PNG" ]; then
  for size in 16 32 128 256 512; do
    double=$((size * 2))
    sips -z "$size" "$size" "$PNG" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
    sips -z "$double" "$double" "$PNG" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
  done
else
  echo "error: neither rsvg-convert (with $SVG) nor $PNG found for icon creation" >&2
  exit 1
fi
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
DMG="$OUT_DIR/GalleryBrowser-${VERSION}-macos-${DMG_ARCH}.dmg"
rm -f "$DMG"
hdiutil create -volname "GalleryBrowser $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
echo "wrote $DMG"
