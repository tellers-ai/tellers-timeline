#!/usr/bin/env bash
# Build the Python bindings (maturin develop) and start the visual test UI.
set -euo pipefail
cd "$(dirname "$0")/../.."

ROOT="$PWD"
if [[ -x bindings/python/.venv/bin/python ]]; then
  VENV="$ROOT/bindings/python/.venv"
elif [[ -x .venv/bin/python ]]; then
  VENV="$ROOT/.venv"
elif [[ -x venv/bin/python ]]; then
  VENV="$ROOT/venv"
else
  VENV=""
fi
PYTHON="${VENV:+$VENV/bin/python}"
PYTHON="${PYTHON:-python3}"

if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  MATURIN=""
  if [[ -n "$VENV" && -x "$VENV/bin/maturin" ]]; then
    MATURIN="$VENV/bin/maturin"
  elif command -v maturin >/dev/null 2>&1; then
    MATURIN="$(command -v maturin)"
  fi
  if [[ -n "$MATURIN" ]]; then
    echo "Building Python bindings into ${VENV:-current python} ($MATURIN develop)..."
    (cd bindings/python && VIRTUAL_ENV="$VENV" "$MATURIN" develop -q) || {
      echo "maturin develop failed; using the already-installed bindings" >&2
    }
  else
    echo "maturin not found; using the already-installed bindings (pip install maturin to rebuild)" >&2
  fi
fi

exec "$PYTHON" tools/timeline-ui/server.py "$@"
