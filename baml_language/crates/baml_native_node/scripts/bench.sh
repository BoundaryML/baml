#!/usr/bin/env bash
# The Node bench: today's SDK against the native bindings, same fixture, same
# script (js/bench/bench.ts), same machine. `scripts/stage.sh` makes both
# packages first.
#
#   scripts/bench.sh prepare        compile both packages, run every config once
#   scripts/bench.sh time [rounds]  interleaved rounds of median ns per call
#   scripts/bench.sh instructions   instructions per call: two call counts, the difference
#
# Results go to target/native-node/bench-*.jsonl; scripts/bench_summary.mjs
# makes the tables.
set -euo pipefail
cd "$(dirname "$0")/../../../target/native-node"

CASES="int inner profile ints_out ints_in points_out points_in"
# Each line: label, package, environment.
CONFIGS=(
  "today|today|"
  "native-eager|native|BAML_NATIVE_TURN=eager BAML_NATIVE_BUILD=positional"
  "native-microtask|native|BAML_NATIVE_TURN=microtask BAML_NATIVE_BUILD=positional"
  "native-props|native|BAML_NATIVE_TURN=eager BAML_NATIVE_BUILD=props"
  "native-init|native|BAML_NATIVE_TURN=eager BAML_NATIVE_BUILD=init"
)

# Which cases and modes each config runs.
runs() {
  for config in "${CONFIGS[@]}"; do
    IFS='|' read -r label package env <<<"$config"
    for case in $CASES; do
      for mode in async sync; do
        case "$label" in
          native-microtask) [[ $mode == async ]] || continue ;;
          native-props|native-init)
            [[ $case == inner || $case == profile || $case == points_out ]] || continue ;;
        esac
        echo "$label|$package|$env|$case|$mode"
      done
    done
  done
  for case in $CASES; do
    echo "plain-js|native||js_$case|sync"
  done
}

calls_for() {
  case "$1" in
    int|inner|js_int|js_inner) echo 20000 ;;
    profile|js_profile) echo 2000 ;;
    *) echo 30 ;;
  esac
}

case "${1:-}" in
  prepare)
    for side in today native; do
      (cd "$side" && pnpm exec tsc -p tsconfig.dist.json)
    done
    cp native/node/baml_sdk/libtype_shapes.dylib native/dist/baml_sdk/
    # One untimed run of everything: macOS scans new binaries on first run.
    runs | while IFS='|' read -r label package env case mode; do
      (cd "$package" && env $env node dist/bench.js "$case" "$mode" calls 4 >/dev/null)
    done
    echo prepared
    ;;
  time)
    rounds=${2:-3}
    out=bench-time.jsonl
    : > "$out"
    for round in $(seq "$rounds"); do
      runs | while IFS='|' read -r label package env case mode; do
        line=$(cd "$package" && env $env node dist/bench.js "$case" "$mode" time)
        echo "{\"round\":$round,\"config\":\"$label\",${line#\{}" >> "$out"
      done
    done
    echo "wrote $out"
    ;;
  instructions)
    out=bench-instructions.jsonl
    : > "$out"
    runs | while IFS='|' read -r label package env case mode; do
      k=$(calls_for "$case")
      counts=()
      for calls in "$k" $((k * 3)); do
        n=$(cd "$package" && env $env /usr/bin/time -l node dist/bench.js "$case" "$mode" calls "$calls" 2>&1 >/dev/null |
          awk '/instructions retired/ {print $1}')
        counts+=("$n")
      done
      echo "{\"config\":\"$label\",\"case\":\"$case\",\"mode\":\"$mode\",\"k\":$k,\"low\":${counts[0]},\"high\":${counts[1]}}" >> "$out"
    done
    echo "wrote $out"
    ;;
  *)
    echo "usage: bench.sh prepare | time [rounds] | instructions" >&2
    exit 2
    ;;
esac
