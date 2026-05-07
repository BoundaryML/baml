#!/usr/bin/env bash
#
# Suite A2 orchestrator. Builds the cross_lang_baml + cross_lang_go
# binaries, then runs hyperfine over (workload × language), parses the
# resulting JSON, and either prints the result table or POSTs it to a
# benchmark-results service.
#
# Env (optional):
#   BENCH_SERVICE_URL    — if set, POST results to this URL.
#   BENCH_SERVICE_TOKEN  — bearer token (required when BENCH_SERVICE_URL set).
#   BENCH_RUN_ID         — run id to attach results to.
#
# Args:
#   --warmup N           hyperfine --warmup (default 3)
#   --runs N             hyperfine --runs (default 10)
#   --workloads w1,w2,…  comma-separated workload allowlist (default: all)
#   --langs l1,l2,…      comma-separated lang allowlist (default: baml,python,go)

set -euo pipefail

HARNESS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CROSS_LANG_DIR="$(dirname "$HARNESS_DIR")"
# cross_lang/harness/run.sh → bench → baml_tests → crates → baml_language → repo root
REPO_ROOT="$(cd "$CROSS_LANG_DIR/../../../../.." && pwd)"

DEFAULT_WORKLOADS=(
    fib_20 loop_500k string_concat_5k array_push_50k array_iter_10k
    class_create_50k field_access_50k call_chain_100_x_5k
    nested_loop mixed_ops closure_call_50k
    hello_world arithmetic fib_20_e2e class_and_loop one_hundred_functions
)
WORKLOADS=("${DEFAULT_WORKLOADS[@]}")
LANGS=(baml python go)
WARMUP=3
RUNS=10

while [[ $# -gt 0 ]]; do
    case "$1" in
        --warmup) WARMUP="$2"; shift 2 ;;
        --runs)   RUNS="$2";   shift 2 ;;
        --workloads)
            IFS=',' read -r -a WORKLOADS <<<"$2"; shift 2 ;;
        --langs)
            IFS=',' read -r -a LANGS <<<"$2"; shift 2 ;;
        *) echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done

if ! command -v hyperfine >/dev/null 2>&1; then
    echo "hyperfine not found on PATH" >&2
    exit 2
fi
if ! command -v jq >/dev/null 2>&1; then
    echo "jq not found on PATH" >&2
    exit 2
fi

echo "==> building cross_lang_baml"
( cd "$REPO_ROOT/baml_language" && cargo build --release --bin cross_lang_baml -p baml_tests )
BAML_BIN="$REPO_ROOT/baml_language/target/release/cross_lang_baml"

GO_BIN="$(mktemp -t cross_lang_go.XXXXXX)"
echo "==> building cross_lang_go → $GO_BIN"
( cd "$CROSS_LANG_DIR" && go build -o "$GO_BIN" ./cli/go )

results_json="["
sep=""
for workload in "${WORKLOADS[@]}"; do
    for lang in "${LANGS[@]}"; do
        case "$lang" in
            baml)
                src="$CROSS_LANG_DIR/workloads/baml/${workload}.baml"
                [[ -f "$src" ]] || { echo "skip: $src missing" >&2; continue; }
                cmd=("$BAML_BIN" "$src")
                ;;
            python)
                src="$CROSS_LANG_DIR/workloads/python/${workload}.py"
                [[ -f "$src" ]] || { echo "skip: $src missing" >&2; continue; }
                cmd=(python3 "$CROSS_LANG_DIR/cli/python/run.py" "$src")
                ;;
            go)
                cmd=("$GO_BIN" "$workload")
                ;;
            *) echo "unknown lang: $lang" >&2; exit 2 ;;
        esac
        tmp="$(mktemp -t hyperfine.XXXXXX.json)"
        echo "==> $workload [$lang]: ${cmd[*]}"
        hyperfine --warmup "$WARMUP" --runs "$RUNS" --export-json "$tmp" -- "${cmd[*]}"
        cell="$(jq --arg w "$workload" --arg l "$lang" '
            .results[0] as $hf | {
              workload: $w, language: $l,
              mean_ns:   ($hf.mean   * 1e9 | floor),
              stddev_ns: ($hf.stddev * 1e9 | floor),
              min_ns:    ($hf.min    * 1e9 | floor),
              max_ns:    ($hf.max    * 1e9 | floor),
              runs: ($hf.times | length),
              raw_hyperfine_json: $hf
            }' "$tmp")"
        results_json+="$sep$cell"
        sep=","
        rm -f "$tmp"
    done
done
results_json+="]"

payload="$(jq -n --argjson results "$results_json" \
    '{suite:"suite_a2", results:$results}')"

if [[ -n "${BENCH_SERVICE_URL:-}" ]]; then
    : "${BENCH_RUN_ID:?BENCH_RUN_ID required when posting}"
    : "${BENCH_SERVICE_TOKEN:?BENCH_SERVICE_TOKEN required when posting}"
    echo "==> POST ${BENCH_SERVICE_URL%/}/benchmark-runs/${BENCH_RUN_ID}/results"
    curl -fsS \
        -H "Authorization: Bearer $BENCH_SERVICE_TOKEN" \
        -H "Content-Type: application/json" \
        -X POST \
        --data-binary "$payload" \
        "${BENCH_SERVICE_URL%/}/benchmark-runs/${BENCH_RUN_ID}/results"
    echo
else
    echo "$payload" | jq .
fi

rm -f "$GO_BIN"
