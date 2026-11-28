#!/usr/bin/env bash
# Builds everything the Python tests and the bench of the native bridge use,
# under target/native_python/:
#
#   libtype_shapes.dylib  the type-shapes fixture plus `bridge_test`
#                         (this directory's bridge_test.baml), every function
#                         that packs
#   vm/baml_sdk           today's generated package for the same sources
#   native/baml_sdk       the same package over the native library: the lines
#                         listed below are the only ones that change
#   tests/                today's SDK tests for the fixture, unchanged, plus
#                         this directory's own tests and conftest (skips)
#   venv/                 one Python with today's runtime (`baml_bridge`,
#                         release) and the native module (`baml_native`,
#                         release): PYTHONPATH picks vm/ or native/
#
# Run from anywhere; heavy steps should go through perfq.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../../.." && pwd)"   # baml_language
WORK="$ROOT/target/native_python"
FIXTURE="$ROOT/sdk_tests/fixtures/type_shapes/baml_src"
# The host crate's `bridge_test` plus the functions the Python tests add
# (the first part is a copy of baml_native_host/tests/type_shapes/bridge_test.baml).
BRIDGE_TEST="$HERE/bridge_test.baml"
GENERATED="$ROOT/sdk_tests/crates/python_pydantic2/type_shapes/generated"
CUSTOMIZABLE="$ROOT/sdk_tests/crates/python_pydantic2/type_shapes/customizable"
mkdir -p "$WORK"

step() { echo "==> $*"; }

# 1. The library.
step "native_pack the fixture + bridge_test"
(cd "$ROOT" && cargo build --release -p baml_compiler2_native --example native_pack)
rm -rf "$WORK/src" && cp -R "$FIXTURE" "$WORK/src"
mkdir -p "$WORK/src/ns_bridge_test" && cp "$BRIDGE_TEST" "$WORK/src/ns_bridge_test/types.baml"
"$ROOT/target/release/examples/native_pack" --lib "$WORK/src" "$WORK/libtype_shapes.dylib" \
  > "$WORK/pack.txt"
"$ROOT/target/release/examples/native_pack" --lib "$WORK/src" "$WORK/describe.dylib" --describe \
  > "$WORK/interface.json" 2>/dev/null
HASH="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["interface_hash"])' "$WORK/interface.json")"

# 2. Today's generated package for the same sources. The generator reads the
# fixture in place, so `bridge_test` is staged into it for this run only.
step "today's generated package, with bridge_test"
STAGED="$FIXTURE/ns_bridge_test"
trap 'rm -rf "$STAGED"' EXIT
mkdir -p "$STAGED" && cp "$BRIDGE_TEST" "$STAGED/types.baml"
(cd "$ROOT" && cargo run --quiet -p sdk_test_codegen -- python_pydantic2)
rm -rf "$STAGED"
trap - EXIT
rm -rf "$WORK/vm" && mkdir -p "$WORK/vm" && cp -R "$GENERATED/baml_sdk" "$WORK/vm/"

# 3. The native bindings: today's package with the runtime swapped. These
# are the only generated lines that change:
#   baml_sdk/__init__.py
#     - from baml_bridge import BamlRuntime, set_type_map
#     + from baml_native import load as _load_native, set_type_map
#     - BamlRuntime.initialize_runtime_from_blob(_inlinedbaml.BYTECODE, _inlinedbaml.EMBEDDED_BAML_TOML)
#     + _load_native(_native_library.PATH, _native_library.INTERFACE_HASH)
#     - from . import _inlinedbaml
#     + from . import _native_library
#   every module that defines functions:
#     - from baml_bridge import define_function as _define_function
#     + from baml_native import define_function as _define_function
#   new: baml_sdk/_native_library.py (the library's path and interface hash),
#        and the library itself beside it.
step "native bindings"
rm -rf "$WORK/native" && mkdir -p "$WORK/native" && cp -R "$GENERATED/baml_sdk" "$WORK/native/"
python3 - "$WORK/native/baml_sdk" "$HASH" <<'EOF'
import pathlib, sys
root = pathlib.Path(sys.argv[1]); hash_hex = sys.argv[2]
def swap(path, old, new):
    text = path.read_text()
    assert text.count(old) == 1, (path, old)
    path.write_text(text.replace(old, new))
init = root / "__init__.py"
swap(init, "from baml_bridge import BamlRuntime, set_type_map\n",
     "from baml_native import load as _load_native, set_type_map\n")
swap(init, "BamlRuntime.initialize_runtime_from_blob(_inlinedbaml.BYTECODE, _inlinedbaml.EMBEDDED_BAML_TOML)\n",
     "_load_native(_native_library.PATH, _native_library.INTERFACE_HASH)\n")
swap(init, "from . import _inlinedbaml\n", "from . import _native_library\n")
changed = 3
for path in root.rglob("*.py"):
    text = path.read_text()
    old = "from baml_bridge import define_function as _define_function\n"
    if old in text:
        path.write_text(text.replace(old, "from baml_native import define_function as _define_function\n"))
        changed += 1
(root / "_native_library.py").write_text(
    "# The native library these bindings are for, and its interface hash.\n"
    "import os\n\n"
    'PATH = os.path.join(os.path.dirname(__file__), "_native_library.dylib")\n'
    f"INTERFACE_HASH = 0x{hash_hex}\n"
)
print(f"changed {changed} generated lines")
EOF
cp "$WORK/libtype_shapes.dylib" "$WORK/native/baml_sdk/_native_library.dylib"

# 4. Tests: today's, unchanged, and ours.
step "tests"
rm -rf "$WORK/tests" && mkdir -p "$WORK/tests"
cp -R "$CUSTOMIZABLE/roundtrip_tests" "$WORK/tests/"
cp "$CUSTOMIZABLE"/test_*.py "$WORK/tests/"
cp "$HERE"/*.py "$WORK/tests/"
cat > "$WORK/tests/pytest.ini" <<'EOF'
[pytest]
python_files = test_*.py
asyncio_mode = auto
addopts = -q -p no:cacheprovider
EOF

# 5. One venv, both runtimes, release builds.
step "venv"
if [[ ! -x "$WORK/venv/bin/python" ]]; then
  uv venv --python 3.13 "$WORK/venv"
  VIRTUAL_ENV="$WORK/venv" uv pip install "pydantic>=2" "protobuf>=6.31.1" \
    "typing-extensions>=4.14.0" pytest pytest-asyncio pytest-timeout
fi
(cd "$ROOT/sdks/python" && VIRTUAL_ENV="$WORK/venv" maturin develop --release --uv)
(cd "$ROOT/crates/baml_native_python" && VIRTUAL_ENV="$WORK/venv" maturin develop --release --uv)
echo "done: $WORK"
