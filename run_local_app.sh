#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENV_DIR="${ROOT_DIR}/.venv"
LOCAL_STATE_DIR="${ROOT_DIR}/.local_state"

if [[ ! -x "${VENV_DIR}/bin/python" ]]; then
    echo "Local virtual environment not found at ${VENV_DIR}."
    echo "Run ${ROOT_DIR}/setup_local_env.sh first."
    exit 1
fi

mkdir -p "${LOCAL_STATE_DIR}/cache" "${LOCAL_STATE_DIR}/config"

# ponytail: gst-inspect exits 0 even for missing elements, so grep; can't pip-install codecs
if ! gst-inspect-1.0 qtdemux 2>/dev/null | grep -q Factory || ! gst-inspect-1.0 avdec_h264 2>/dev/null | grep -q Factory; then
    echo "Missing GStreamer plugins — video preview (MP4/MOV) will not work." >&2
    echo "Install, then re-run:" >&2
    echo "  Arch:    sudo pacman -S --needed gst-plugins-base gst-plugins-good gst-libav" >&2
    echo "  Debian:  sudo apt install gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-libav" >&2
    exit 1
fi
export XDG_CACHE_HOME="${LOCAL_STATE_DIR}/cache"
export XDG_CONFIG_HOME="${LOCAL_STATE_DIR}/config"

exec "${VENV_DIR}/bin/gallerybrowser" "$@"
