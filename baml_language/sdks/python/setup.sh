#!/usr/bin/env bash
# Setup: install Python deps + build the Rust extension into the venv.
# Subsequent runs are fast if no Rust code changed.
set -euo pipefail
cd "$(dirname "$0")"

# Print a hint if the build takes a while (first run compiles Rust).
_slow_build_hint() {
    sleep 15
    echo "  (still building — first run compiles the Rust extension, subsequent runs are fast)"
}
_slow_build_hint &
HINT_PID=$!

# NOTE: a plain `uv sync` does NOT rebuild baml_bridge after incremental
# Rust edits. baml_bridge is an editable maturin install, but uv keys the
# rebuild on the Python project metadata, not the Rust sources behind it
# — so after you change Rust under rust/bridge_python (or any engine
# crate), `uv sync` is a no-op and leaves a STALE `baml_py.abi3.so` in
# the venv. pytest then imports the old extension and fails on freshly-
# added symbols. To pick up Rust changes, force a rebuild with one of:
#   uv sync --reinstall-package baml_bridge   # forces maturin to rebuild
#   maturin develop                         # faster: reuses cargo's
#                                           # incremental cache (~7s vs
#                                           # ~70s; uv's build isolation
#                                           # uses an ephemeral interpreter
#                                           # that busts the pyo3 fingerprint
#                                           # and rebuilds bridge_python every run)
echo "==> uv sync"
uv sync

kill $HINT_PID 2>/dev/null || true
wait $HINT_PID 2>/dev/null || true

echo "==> Setup complete"
