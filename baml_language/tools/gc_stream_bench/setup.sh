#!/usr/bin/env bash
# Build the benchmark environment: generate baml_sdk/ and install an optimized
# baml_bridge wheel with GC pause timings (`gc_profiling`) into .venv.
#
# The wheel goes into this venv only, so the shared editable
# sdks/python/src/baml_bridge/baml_py.abi3.so (used by sdk_tests) is untouched.
# Rerun after changing Rust or baml_src/.
set -euo pipefail
cd "$(dirname "$0")"
BENCH="$PWD"
WORKSPACE="$(cd ../.. && pwd)"
PROFILE="${GC_BENCH_PROFILE:-fasttest}"

uv_bin="uv"
command -v uv >/dev/null 2>&1 || uv_bin="$(mise which uv)"

echo "==> baml-cli generate"
(cd "$WORKSPACE" && cargo build --quiet -p baml_cli --bin baml-cli)
BAML_AGENT_SKILL_CHECK=off "$WORKSPACE/target/debug/baml-cli" generate --quiet

echo "==> venv"
"$uv_bin" venv --allow-existing --quiet .venv
"$uv_bin" pip install --quiet --python .venv/bin/python \
    'maturin>=1.10,<2.0' 'pydantic>=2' 'typing-extensions>=4.14.0' 'protobuf>=6.31.1' psutil

echo "==> baml_bridge wheel (profile $PROFILE, gc_profiling)"
rm -rf .wheels
(cd "$WORKSPACE/sdks/python" && "$BENCH/.venv/bin/maturin" build --quiet \
    --profile "$PROFILE" --features gc_profiling \
    --interpreter "$BENCH/.venv/bin/python" --out "$BENCH/.wheels")
"$uv_bin" pip install --quiet --python .venv/bin/python --reinstall .wheels/*.whl

echo "==> ready: .venv/bin/python bench.py"
