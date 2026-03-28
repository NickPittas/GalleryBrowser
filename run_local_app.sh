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
export XDG_CACHE_HOME="${LOCAL_STATE_DIR}/cache"
export XDG_CONFIG_HOME="${LOCAL_STATE_DIR}/config"

exec "${VENV_DIR}/bin/gallerybrowser" "$@"
