set shell := ["/bin/bash", "-cu"]

default:
  @just --list

regen-schema:
  cargo run -p tellers-timeline-schema --quiet

build-all:
  cargo build --workspace --all-targets --exclude tellers-timeline-wasm --exclude tellers-timeline-python

# Runs Rust tests, Python tests (via maturin develop), and wasm tests
# Requires Python and Node toolchains installed
test-all:
  cargo test -p tellers-timeline-core -p tellers-timeline-schema -q
  # Python binding tests
  if command -v maturin >/dev/null 2>&1; then \
    cd bindings/python && \
    maturin develop -q && \
    cd ../.. && \
    if [ -d bindings/python/.venv ]; then \
      bindings/python/.venv/bin/python -m pip install -q pytest && \
      bindings/python/.venv/bin/python -m pytest -q bindings/python/tests; \
    else \
      python -m pip install -q pytest && \
      python -m pytest -q bindings/python/tests; \
    fi; \
  else \
    echo "maturin not found, skipping python tests"; \
  fi
  # JS wasm build (no runtime tests for now)
  rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
  cargo build -p tellers-timeline-wasm --target wasm32-unknown-unknown -q
