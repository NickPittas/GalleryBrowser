#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENV_DIR="${ROOT_DIR}/.venv"
LOCAL_STATE_DIR="${ROOT_DIR}/.local_state"
PYTHON_BIN="${PYTHON:-python3}"

echo "Creating local virtual environment at ${VENV_DIR}"
"${PYTHON_BIN}" -m venv "${VENV_DIR}" --system-site-packages

mkdir -p "${LOCAL_STATE_DIR}/cache" "${LOCAL_STATE_DIR}/config"

echo "Upgrading packaging tools"
"${VENV_DIR}/bin/python" -m pip install --upgrade pip setuptools wheel

echo "Installing project dependencies into the local virtual environment"
"${VENV_DIR}/bin/python" -m pip install -r "${ROOT_DIR}/requirements.txt"
"${VENV_DIR}/bin/python" -m pip install -e "${ROOT_DIR}[dev]"

echo
echo "Local environment is ready."
echo "Run the app with: ${ROOT_DIR}/run_local_app.sh"
