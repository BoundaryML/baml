#!/usr/bin/env bash
# Per-fixture uv setup for the sdk_test_python_pydantic2 crate.
#
# Invoked automatically by `cargo nextest run` via the setup-script
# binding in `baml_language/.config/nextest.toml` — whenever the run
# selects any sdk_test_python_pydantic2 test. For plain `cargo test`
# (no nextest) run this manually after `cargo test --no-run` populates
# each fixture's `generated/pyproject.toml`.
#
# Two responsibilities, split by tool:
#
#   1. `uv sync` per fixture turns each generated/ dir into a real
#      `.venv/` with pytest/pydantic/etc. installed and baml_core
#      editable-linked. Every fixture's editable link resolves to the
#      same shared `sdks/python/src/baml_core/baml_py.abi3.so`.
#
#   2. `maturin develop` (once) rebuilds that shared `.so` from the
#      current Rust sources. We target `sdks/python`'s own `.venv` —
#      populated from `sdks/python/pyproject.toml`'s dev group via
#      `uv sync --group dev --no-install-project`. That venv path is
#      stable across runs, which keeps pyo3's build-config fingerprint
#      valid, so cargo's incremental cache hits and steady-state
#      rebuilds are ~7s.
#
# Why not `uv sync --reinstall-package baml_core` (the old approach)?
# That was strictly slower, ~70s EVERY run even when nothing changed.
# uv builds the editable wheel in an isolated env with an *ephemeral*
# interpreter at a fresh `target/uv-cache/builds-v0/.tmpXXXX/bin/python`
# path each invocation. pyo3-build-config is keyed on the interpreter,
# so its fingerprint changed every run, invalidating pyo3 and cascading
# into a full recompile + relink of the bridge_python addon — i.e. the
# cargo cache was present but never hit. `maturin develop` against a
# pinned venv has none of that overhead and is purely incremental, so
# it dominates the old path on every axis. (It also builds the dev
# profile, matching what `cargo nextest` builds the rest of the
# workspace with, so the engine crates' debug artifacts are shared too;
# pass `--release` below if you need release semantics in the .so.)
#
# Note the split is deliberate: a plain `uv sync` (no --reinstall) does
# NOT rebuild the .so on incremental Rust edits — uv doesn't track the
# Rust sources behind the editable install — so step 2 owns rebuilds.
# Re-run after bridge_python Rust changes or after adding a new fixture.
#
# This mirrors the typescript_node crate's setup.sh: install/native-
# build lives here, OUT of build.rs, so `cargo check`/`cargo doc`
# succeed without uv installed and the heavy work only fires when a
# python sdk-test is actually selected.

set -euo pipefail

cd "$(dirname "$0")"  # baml_language/sdk_tests/crates/python_pydantic2

WORKSPACE_ROOT="$(cd ../../.. && pwd)"
SDK_PY="$WORKSPACE_ROOT/sdks/python"

# Shared uv cache under target/, matching the UV_CACHE_DIR the emitted
# tests thread through (run_test_cmd / CACHE_ENV_VAR in harness_setup).
export UV_CACHE_DIR="$WORKSPACE_ROOT/target/uv-cache"
mkdir -p "$UV_CACHE_DIR"

# `uv` may not be on PATH directly; fall back to `mise which uv`, the
# same fallback run_test_cmd uses for the test-time `uv run` calls.
uv_bin="uv"
command -v uv >/dev/null 2>&1 || uv_bin="$(mise which uv)"

# EXPERIMENT (build-caching/04 approach B): build baml_core ONCE as a
# wheel, then install that prebuilt wheel into every fixture venv. This
# removes the redundant per-fixture editable build — plain `uv sync` used
# to build baml_core's editable wheel under build isolation with an
# *ephemeral* interpreter (see the --reinstall note above), busting
# pyo3's fingerprint and forcing a full bridge_python rebuild every run
# (~113s even with a warm target/), and then `maturin develop` rebuilt it
# again. One `maturin build` (dev profile, reuses the warm cargo target/
# ~20s) replaces both.

# 1. Build the shared baml_core wheel once (sdks/python/.venv supplies maturin).
SDK_PY_VENV="$SDK_PY/.venv"
WHEEL_DIR="$WORKSPACE_ROOT/target/wheels"
# Must exist before the first uv command: CI exports UV_FIND_LINKS=$WHEEL_DIR
# and uv errors on a missing find-links dir even when nothing needs it yet.
mkdir -p "$WHEEL_DIR"
echo "==> uv sync (dev) in $SDK_PY"
(cd "$SDK_PY" && UV_PROJECT_ENVIRONMENT="$SDK_PY_VENV" "$uv_bin" sync --group dev --no-install-project)
echo "==> maturin build (shared baml_core wheel)"
(cd "$SDK_PY" && VIRTUAL_ENV="$SDK_PY_VENV" "$SDK_PY_VENV/bin/maturin" build --out "$WHEEL_DIR")

# 2. Per fixture: install deps + the prebuilt baml_core wheel. No
#    per-fixture cdylib build. Strip the editable `[tool.uv.sources]
#    baml_core` block so baml_core resolves as a normal dependency from
#    UV_FIND_LINKS (exported by CI to $WHEEL_DIR; the local 0.1.3 wheel
#    outranks any public release). Plain `uv sync`/`uv run` then installs
#    the prebuilt wheel + dev tools without ever building baml_core.
export UV_FIND_LINKS="${UV_FIND_LINKS:-$WHEEL_DIR}"
for fixture_dir in */generated; do
    [[ -d "$fixture_dir" ]] || continue
    # Delete the [tool.uv.sources] section (header → next blank line).
    sed -i.bak '/^\[tool\.uv\.sources\]$/,/^$/d' "$fixture_dir/pyproject.toml"
    rm -f "$fixture_dir/pyproject.toml.bak"
    echo "==> uv sync in $fixture_dir"
    (cd "$fixture_dir" && "$uv_bin" sync)
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
