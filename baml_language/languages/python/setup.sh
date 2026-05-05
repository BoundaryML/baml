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

echo "==> uv sync"
uv sync

kill $HINT_PID 2>/dev/null || true
wait $HINT_PID 2>/dev/null || true

echo "==> Setup complete"
