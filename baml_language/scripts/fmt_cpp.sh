#!/usr/bin/env bash
# Formats the hand-written C++ SDK sources in place with clang-format
# (config: sdks/cpp/.clang-format, sdk_tests/crates/cpp/.clang-format).
# Generated output (baml_sdk trees under generated/, cbindgen's baml_cffi.h,
# protoc's checked-in pb/ sources) is intentionally not covered. The prek hook runs this and fails the commit
# when files change, mirroring the cargo-fmt hook.
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v clang-format > /dev/null; then
    echo "error: clang-format not found on PATH (brew install clang-format)" >&2
    exit 1
fi

find sdks/cpp/bridge_cpp sdk_tests/crates/cpp \
    \( -name generated -o -name pb \) -prune -o \
    \( -name '*.h' -o -name '*.cc' \) -print0 |
    xargs -0 clang-format -i
