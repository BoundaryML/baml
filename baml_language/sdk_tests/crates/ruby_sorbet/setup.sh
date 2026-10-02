#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
TEST_ROOT="$PWD"
WORKSPACE_ROOT="$(cd ../../.. && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$WORKSPACE_ROOT/target}"
if [[ "$TARGET_DIR" != /* ]]; then
  TARGET_DIR="$WORKSPACE_ROOT/$TARGET_DIR"
fi
FIXTURE_DIR="$TARGET_DIR/ruby-bridge-fixtures"
INCLUDE_DIR="$WORKSPACE_ROOT/crates/bridge_cffi/include"
mkdir -p "$FIXTURE_DIR"

(
  cd "$WORKSPACE_ROOT"
  cargo build -p bridge_cffi
  cargo run --quiet -p sdk_test_codegen -- ruby_sorbet
  cargo run --quiet -p sdk_test_codegen -- \
    emit-bytecode --fixture function_calls \
    --out "$FIXTURE_DIR/function-calls.bytecode"
)

if [[ "$(uname -s)" == "Darwin" ]]; then
  LIB_EXT="dylib"
  C_SHARED=( -dynamiclib )
  CXX_SHARED=( -dynamiclib )
  REAL_RUNTIME="$TARGET_DIR/debug/libbridge_cffi.dylib"
else
  LIB_EXT="so"
  C_SHARED=( -shared -fPIC )
  CXX_SHARED=( -shared -fPIC -pthread )
  REAL_RUNTIME="$TARGET_DIR/debug/libbridge_cffi.so"
fi

cc -std=c11 -Wall -Wextra -Werror -D_POSIX_C_SOURCE=200809L \
  "${C_SHARED[@]}" -pthread -I"$INCLUDE_DIR" \
  "$TEST_ROOT/test/native/bridge_fixture.c" \
  -o "$FIXTURE_DIR/libbaml_ruby_test_fixture.$LIB_EXT"
cc -std=c11 -Wall -Wextra -Werror \
  "${C_SHARED[@]}" \
  "$TEST_ROOT/test/native/missing_getter.c" \
  -o "$FIXTURE_DIR/libbaml_ruby_missing_getter.$LIB_EXT"
c++ -std=c++17 -Wall -Wextra -Werror \
  "${CXX_SHARED[@]}" \
  "$TEST_ROOT/test/native/thread_callback.cpp" \
  -o "$FIXTURE_DIR/libbaml_ruby_thread_callback.$LIB_EXT"
printf 'not a dynamic library\n' > "$FIXTURE_DIR/not-a-library"

# The pinned checker has no Windows, 32-bit Linux or musl binary. Keep its
# bundle separate so those hosts can still install and run the runtime suite.
case "$(uname -s):$(uname -m)" in
  Darwin:x86_64|Darwin:arm64) STATIC_SUPPORTED=1 ;;
  Linux:x86_64|Linux:aarch64)
    if getconf GNU_LIBC_VERSION >/dev/null 2>&1; then STATIC_SUPPORTED=1; else STATIC_SUPPORTED=0; fi
    ;;
  *) STATIC_SUPPORTED=0 ;;
esac
if [[ "$STATIC_SUPPORTED" == 1 ]]; then
  (
    export BUNDLE_GEMFILE="$TEST_ROOT/test/static/Gemfile"
    export BUNDLE_PATH="$TARGET_DIR/ruby-sorbet-static-bundle"
    export BUNDLE_FROZEN=true
    ruby -S bundle install --jobs 4 --retry 3
  )
else
  echo "Sorbet static gate unavailable: checker 0.6.13506 requires x86_64/arm64 macOS or x86_64/aarch64 glibc Linux; runtime tests still run."
fi

export BUNDLE_GEMFILE="$TEST_ROOT/Gemfile"
export BUNDLE_PATH="$TARGET_DIR/ruby-bundle"
ruby -S bundle install --jobs 4 --retry 3

if [[ -n "${NEXTEST_ENV:-}" ]]; then
  {
    echo "SDK_TEST_RUBY_SORBET_SETUP=1"
    echo "BUNDLE_GEMFILE=$BUNDLE_GEMFILE"
    echo "BUNDLE_PATH=$BUNDLE_PATH"
    echo "BAML_RUBY_TEST_FIXTURE=$FIXTURE_DIR/libbaml_ruby_test_fixture.$LIB_EXT"
    echo "BAML_RUBY_TEST_MISSING_GETTER=$FIXTURE_DIR/libbaml_ruby_missing_getter.$LIB_EXT"
    echo "BAML_RUBY_TEST_INVALID_LIBRARY=$FIXTURE_DIR/not-a-library"
    echo "BAML_RUBY_TEST_THREAD_FIXTURE=$FIXTURE_DIR/libbaml_ruby_thread_callback.$LIB_EXT"
    echo "BAML_RUBY_TEST_REAL_RUNTIME=$REAL_RUNTIME"
    echo "BAML_RUBY_TEST_REAL_BYTECODE=$FIXTURE_DIR/function-calls.bytecode"
  } >> "$NEXTEST_ENV"
fi
