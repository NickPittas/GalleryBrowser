#!/usr/bin/env bash
# Regenerate flatpak/cargo-sources.json from Cargo.lock, build the Flatpak and write
# GalleryBrowser-<version>.flatpak into the repo root.
#
# Usage: flatpak/build.sh <version>        e.g. flatpak/build.sh 0.2.0
#
# Prerequisites (user-level): flatpak, python3, git and
#   flatpak install --user flathub org.flatpak.Builder org.freedesktop.Sdk//25.08 \
#       org.freedesktop.Sdk.Extension.rust-stable//25.08
set -euo pipefail

if [ $# -ne 1 ] || [ -z "$1" ]; then
    echo "usage: $0 <version>" >&2
    exit 2
fi
VERSION="$1"

APP_ID=io.github.nickpittas.GalleryBrowser
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOOLS_REF="${FLATPAK_BUILDER_TOOLS_REF:-master}"
GEN_URL="https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/${TOOLS_REF}/cargo/flatpak-cargo-generator.py"
WORK="${FLATPAK_GEN_DIR:-${TMPDIR:-/tmp}/gallerybrowser-flatpak-gen}"

cd "$ROOT"

# 1. cargo-sources.json (throwaway venv + generator, reused between runs)
mkdir -p "$WORK"
if [ ! -x "$WORK/venv/bin/python" ]; then
    python3 -m venv "$WORK/venv"
    "$WORK/venv/bin/pip" install --quiet aiohttp tomlkit
fi
curl -fsSL "$GEN_URL" -o "$WORK/flatpak-cargo-generator.py"
"$WORK/venv/bin/python" "$WORK/flatpak-cargo-generator.py" Cargo.lock -o flatpak/cargo-sources.json

# 2. Build into a local repo
flatpak run org.flatpak.Builder --user --force-clean --repo=flatpak/repo \
    flatpak/build-dir flatpak/${APP_ID}.yml

# 3. Single-file bundle
OUT="GalleryBrowser-${VERSION}.flatpak"
rm -f "$OUT"
flatpak build-bundle flatpak/repo "$OUT" "$APP_ID" \
    --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo
ls -l "$OUT"
echo "Install with: flatpak install --user -y --reinstall ./$OUT"
