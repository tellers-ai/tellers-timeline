#!/usr/bin/env bash
# Build the Python bindings (maturin develop) and start the visual test UI.
set -euo pipefail
cd "$(dirname "$0")/../.."

ROOT="$PWD"
VENV="$ROOT/bindings/python/.venv"

# Keep the UI's tooling self-contained. F5 should work from a fresh checkout,
# rather than depending on whichever virtual environment happens to exist.
if [[ ! -x "$VENV/bin/python" ]]; then
  python3 -m venv "$VENV"
fi
PYTHON="$VENV/bin/python"

if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  if ! "$PYTHON" -m maturin --version >/dev/null 2>&1; then
    echo "Installing the one-time UI build dependency (maturin)..."
    "$PYTHON" -m pip install -q maturin
  fi
  echo "Building Python bindings into $VENV..."
  (cd bindings/python && "$PYTHON" -m maturin develop -q)
fi

exec "$PYTHON" tools/timeline-ui/server.py "$@"
