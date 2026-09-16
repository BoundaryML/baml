#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

verify_target() {
  local target="$1"
  local expected_arch="$2"
  local actual_arch=''
  local _
  for _ in {1..30}; do
    if actual_arch="$("$SCRIPT_DIR/ssh.sh" "$target" -- uname -m 2>/dev/null)"; then
      break
    fi
    sleep 5
  done
  [[ "$actual_arch" == "$expected_arch" ]] || { printf 'error: %s reported architecture %q, expected %q\n' "$target" "$actual_arch" "$expected_arch" >&2; return 1; }
  printf '%-9s SSH verified (%s)\n' "$target" "$actual_arch"
}

verify_target x64 x86_64
verify_target graviton aarch64
