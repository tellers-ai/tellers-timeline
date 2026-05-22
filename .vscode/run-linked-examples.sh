#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

EXAMPLE="${1:-all}"

if [[ -x bindings/python/.venv/bin/python ]]; then
  PYTHON=bindings/python/.venv/bin/python
elif [[ -x venv/bin/python ]]; then
  PYTHON=venv/bin/python
else
  PYTHON=python
fi

"$PYTHON" examples/linked_clip_examples.py "$EXAMPLE"
