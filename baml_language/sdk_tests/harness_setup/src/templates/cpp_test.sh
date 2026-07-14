#!/usr/bin/env bash
# Compile-and-run driver for one C++ sdk-test fixture. Written into
# <fixture>/generated/ by sdk_test_harness_setup::cpp; test sources come from
# the customizable/ overlay (tests/*.cc), the typed SDK from
# baml_sdk/ (emitted by sdkgen_cpp), and the bridge from the repo's
# bridge_cpp headers + the dev-profile bridge_cffi cdylib built by
# crates/cpp/setup.sh.
set -euo pipefail
cd "$(dirname "$0")"

WORKSPACE_ROOT="$(cd ../../../../.. && pwd)" # baml_language/

INCLUDES=(
    -I baml_sdk/include
    -I "$WORKSPACE_ROOT/sdks/cpp/bridge_cpp/include"
    -I "$WORKSPACE_ROOT/crates/bridge_cffi/include"
    -I "$WORKSPACE_ROOT/sdk_tests/crates/cpp/common"
)
LIBDIR="$WORKSPACE_ROOT/target/debug"

if ! compgen -G "tests/*.cc" > /dev/null; then
    # No ported tests yet: still syntax-check the generated SDK itself.
    if compgen -G "baml_sdk/src/*.cc" > /dev/null; then
        c++ -std=c++17 -Wall -Wextra -fsyntax-only "${INCLUDES[@]}" baml_sdk/src/*.cc
        echo "no tests/*.cc in this fixture yet; generated SDK syntax-checked"
    else
        echo "no tests/*.cc and no generated SDK sources; nothing to compile"
    fi
    exit 0
fi
SOURCES=(tests/*.cc)
if compgen -G "baml_sdk/src/*.cc" > /dev/null; then
    SOURCES+=(baml_sdk/src/*.cc)
fi

# The compile and run checks execute concurrently under nextest; each mode
# writes its own binary so they cannot clobber each other mid-execution.
compile() {
    c++ -std=c++17 -Wall -Wextra "${INCLUDES[@]}" "${SOURCES[@]}" -o "$1" \
        -L"$LIBDIR" -lbridge_cffi -Wl,-rpath,"$LIBDIR"
}

case "${1:-}" in
    compile)
        compile fixture_tests_compile_check
        ;;
    run)
        compile fixture_tests
        ./fixture_tests
        ;;
    *)
        echo "usage: test.sh {compile|run}" >&2
        exit 2
        ;;
esac
