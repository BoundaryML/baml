#!/usr/bin/env bash
# CMake compile-and-run driver for one C++ sdk-test fixture. Written into
# <fixture>/generated/ by sdk_test_harness_setup::cpp; test sources come
# from the customizable/ overlay (tests/*.cc), the typed SDK from
# baml_sdk/ (emitted by sdkgen_cpp, consumed exactly like an end user:
# add_subdirectory + baml::sdk, which builds the pinned protobuf-lite
# runtime via FetchContent), and the dev-profile bridge_cffi cdylib built
# by crates/cpp/setup.sh.
set -euo pipefail
cd "$(dirname "$0")"

FIXTURE="$(basename "$(cd .. && pwd)")"
GENERATED="$PWD"
WORKSPACE_ROOT="$(cd ../../../../.. && pwd)" # baml_language/
COMMON_DIR="$WORKSPACE_ROOT/sdk_tests/crates/cpp/common"

MODE="${1:-}"
case "$MODE" in
    compile | run) ;;
    *)
        echo "usage: test.sh {compile|run}" >&2
        exit 2
        ;;
esac

# Build trees live under target/ (not generated/, which codegen wipes), so
# the FetchContent protobuf build survives regeneration. The compile and
# run checks execute concurrently under nextest; each mode gets its own
# build tree so they cannot clobber each other mid-execution. The pinned
# protobuf/abseil *clones* are shared read-only from setup.sh.
BUILD_DIR="$WORKSPACE_ROOT/target/cpp-fixture-builds/$FIXTURE-$MODE"

HAVE_TESTS=0
if compgen -G "tests/*.cc" > /dev/null; then
    HAVE_TESTS=1
fi
# tests/cxx20/*.cc need C++20 (co_await coverage); they build as a second
# executable only when the toolchain has C++20, so the main test binary
# keeps verifying that the generated SDK compiles as plain C++17.
HAVE_CXX20_TESTS=0
if compgen -G "tests/cxx20/*.cc" > /dev/null; then
    HAVE_CXX20_TESTS=1
fi

# Consumer-shaped shim project: add_subdirectory over the generated SDK,
# plus the fixture tests when present.
mkdir -p "$BUILD_DIR"
{
    echo 'cmake_minimum_required(VERSION 3.16)'
    echo 'project(fixture_tests LANGUAGES CXX)'
    echo "add_subdirectory(\"$GENERATED/baml_sdk\" baml_sdk)"
    if [ "$HAVE_TESTS" = 1 ]; then
        echo "file(GLOB TEST_SOURCES \"$GENERATED/tests/*.cc\")"
        echo 'add_executable(fixture_tests ${TEST_SOURCES})'
        echo "target_include_directories(fixture_tests PRIVATE \"$COMMON_DIR\")"
        echo 'target_link_libraries(fixture_tests PRIVATE baml::sdk)'
        echo 'if(NOT MSVC)'
        echo '  target_compile_options(fixture_tests PRIVATE -Wall -Wextra)'
        echo 'endif()'
    fi
    if [ "$HAVE_CXX20_TESTS" = 1 ]; then
        echo 'if("cxx_std_20" IN_LIST CMAKE_CXX_COMPILE_FEATURES)'
        echo "  file(GLOB CXX20_TEST_SOURCES \"$GENERATED/tests/cxx20/*.cc\")"
        echo '  add_executable(fixture_tests_cxx20 ${CXX20_TEST_SOURCES})'
        echo '  target_compile_features(fixture_tests_cxx20 PRIVATE cxx_std_20)'
        echo "  target_include_directories(fixture_tests_cxx20 PRIVATE \"$COMMON_DIR\")"
        echo '  target_link_libraries(fixture_tests_cxx20 PRIVATE baml::sdk)'
        echo '  if(NOT MSVC)'
        echo '    target_compile_options(fixture_tests_cxx20 PRIVATE -Wall -Wextra)'
        echo '  endif()'
        echo 'endif()'
    fi
} > "$BUILD_DIR/CMakeLists.txt"

# Pre-cloned pinned sources from setup.sh: skips FetchContent population,
# so concurrent configures cannot race and no network is needed here.
cmake -S "$BUILD_DIR" -B "$BUILD_DIR/build" \
    -DFETCHCONTENT_SOURCE_DIR_PROTOBUF="$WORKSPACE_ROOT/target/cpp-protobuf-src" \
    -DFETCHCONTENT_SOURCE_DIR_ABSL="$WORKSPACE_ROOT/target/cpp-absl-src" \
    > "$BUILD_DIR/configure.log" 2>&1 ||
    { cat "$BUILD_DIR/configure.log" >&2 && exit 1; }
# Bounded parallelism: up to eight fixture builds run concurrently under
# nextest, and a bare `-j` (unbounded with Makefiles) can starve the CI
# runner to death. Quarter of the cores per build tree, minimum 2.
NPROC="$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)"
JOBS=$((NPROC / 4)); [ "$JOBS" -lt 2 ] && JOBS=2
cmake --build "$BUILD_DIR/build" -j "$JOBS" > "$BUILD_DIR/build.log" 2>&1 ||
    { cat "$BUILD_DIR/build.log" >&2 && exit 1; }

if [ "$HAVE_TESTS" = 0 ]; then
    echo "no tests/*.cc in this fixture yet; generated SDK compiled"
    exit 0
fi

if [ "$MODE" = run ]; then
    case "$(uname -s)" in
        Darwin) RUNTIME_LIB="libbridge_cffi.dylib" ;;
        MSYS* | MINGW* | CYGWIN*) RUNTIME_LIB="bridge_cffi.dll" ;;
        *) RUNTIME_LIB="libbridge_cffi.so" ;;
    esac
    BAML_RUNTIME_PATH="$WORKSPACE_ROOT/target/debug/$RUNTIME_LIB" \
        "$BUILD_DIR/build/fixture_tests"
    if [ "$HAVE_CXX20_TESTS" = 1 ]; then
        if [ -x "$BUILD_DIR/build/fixture_tests_cxx20" ]; then
            BAML_RUNTIME_PATH="$WORKSPACE_ROOT/target/debug/$RUNTIME_LIB" \
                "$BUILD_DIR/build/fixture_tests_cxx20"
        else
            echo "note: toolchain lacks C++20; tests/cxx20 skipped"
        fi
    fi
fi
