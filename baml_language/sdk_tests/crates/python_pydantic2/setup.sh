#!/usr/bin/env bash
# Per-fixture uv setup for the sdk_test_python_pydantic2 crate.
#
# Invoked automatically by `cargo nextest run` via the setup-script
# binding in `baml_language/.config/nextest.toml` — whenever the run
# selects any sdk_test_python_pydantic2 test. For plain `cargo test`
# (no nextest) run this manually after `cargo test --no-run` populates
# each fixture's `generated/pyproject.toml`.
#
# This turns those generated/ dirs into a real `.venv/` with baml_core
# installed editable. The crucial bit is `--reinstall-package
# baml_core`: a plain `uv sync` is a NO-OP on incremental Rust edits —
# uv doesn't track the Rust sources behind the editable install — so
# the maturin-built `baml_core/baml_py.abi3.so` stays stale and pytest
# imports fail on freshly-added symbols (e.g. `register_host_callable`).
# `--reinstall-package` forces uv to rebuild that editable install,
# which kicks off the maturin build of bridge_python's native addon.
# The cargo compile underneath is incremental, so steady-state re-runs
# are cheap. Re-run after bridge_python Rust changes or after adding a
# new fixture.
#
# This mirrors the nodejs_typescript crate's setup.sh: install/native-
# build lives here, OUT of build.rs, so `cargo check`/`cargo doc`
# succeed without uv installed and the heavy work only fires when a
# python sdk-test is actually selected.

set -euo pipefail

cd "$(dirname "$0")"  # baml_language/sdk_tests/crates/python_pydantic2

WORKSPACE_ROOT="$(cd ../../.. && pwd)"

# Shared uv cache under target/, matching the UV_CACHE_DIR the emitted
# tests thread through (run_test_cmd / CACHE_ENV_VAR in harness_setup).
export UV_CACHE_DIR="$WORKSPACE_ROOT/target/uv-cache"
mkdir -p "$UV_CACHE_DIR"

# `uv` may not be on PATH directly; fall back to `mise which uv`, the
# same fallback run_test_cmd uses for the test-time `uv run` calls.
uv_bin="uv"
command -v uv >/dev/null 2>&1 || uv_bin="$(mise which uv)"

# baml_core is editable-installed into every fixture's venv but resolves
# to the same shared `sdks/python/.../baml_py.abi3.so`, so the first
# reinstall rebuilds it and the rest are quick no-ops. We loop all
# fixtures anyway to guarantee each venv exists and is synced.
for fixture_dir in */generated; do
    [[ -d "$fixture_dir" ]] || continue
    echo "==> uv sync --reinstall-package baml_core in $fixture_dir"
    (cd "$fixture_dir" && "$uv_bin" sync --reinstall-package baml_core)
done

# Per-run breadcrumb for the in-test guard. nextest reads $NEXTEST_ENV
# after this script and injects these vars into the matched tests'
# processes — so `setup_guard::ran` (see harness_runner) can prove
# this script ran *this* run. Plain `cargo test` has no $NEXTEST_ENV,
# so the var stays unset and the guard fails with a helpful message.
# Keep the var name in sync with SETUP_ENV_VAR in
# harness_setup/src/python_pydantic2.rs.
if [[ -n "${NEXTEST_ENV:-}" ]]; then
    echo "SDK_TEST_PYTHON_PYDANTIC2_SETUP=1" >> "$NEXTEST_ENV"
fi
