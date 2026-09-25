#!/usr/bin/env bash
# Record the demo program with `baml run`, then answer each demo question with
# `baml query`. Usage: run_demo.sh [BAML_CLI] [PARENT_DIR]
#
# BAML_CLI defaults to this worktree's debug build; build it with
#   cd baml_language && cargo build -p baml_cli --bin baml-cli
# Every run works in a new directory created under PARENT_DIR (default: the
# system temp directory); the destructive steps only touch that directory.
# Any CLI call that exits other than expected stops the script with an error.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
cli=${1:-$here/../../baml_language/target/debug/baml-cli}
parent=${2:-${TMPDIR:-/tmp}}
[ -x "$cli" ] || { echo "demo: no CLI at $cli" >&2; exit 1; }
mkdir -p "$parent"
work=$(mktemp -d "$parent/btel-demo.XXXXXX")
port=${MOCK_PORT:-$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')}

mkdir "$work/project"
cp -r "$here/baml_src" "$work/project/"
cd "$work/project"
export BAML_AGENT_SKILL_CHECK=off MOCK_LLM_URL="http://127.0.0.1:$port"

mock=
slow=
cleanup() {
  [ -n "$slow" ] && kill "$slow" 2>/dev/null || true
  [ -n "$mock" ] && kill "$mock" 2>/dev/null || true
}
trap cleanup EXIT
python3 "$here/mock_llm.py" "$port" &
mock=$!
for _ in $(seq 50); do
  if (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; then break; fi
  kill -0 "$mock" 2>/dev/null || { echo "demo: mock server exited" >&2; exit 1; }
  sleep 0.2
done
(exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null ||
  { echo "demo: mock server not listening on $port" >&2; exit 1; }
echo "== working in $work (mock LLM on port $port)"

# Run the CLI and print its output without the toolchain warning. Returns the
# CLI's exit status.
baml() {
  local out status=0
  out=$("$cli" "$@" 2>&1) || status=$?
  printf '%s\n' "$out" | grep -v 'internal BAML toolchain' || true
  return "$status"
}
# expect CODE COMMAND...: stop the demo unless COMMAND exits with CODE.
expect() {
  local want=$1 status=0
  shift
  "$@" || status=$?
  if [ "$status" -ne "$want" ]; then
    echo "demo: expected exit $want, got $status: $*" >&2
    exit 1
  fi
}
# q SQL [CODE]: print and run a query. `baml query` exits 0 when complete and
# 1 when some evidence was unavailable.
q() {
  echo
  echo "\$ baml query \"$1\""
  expect "${2:-0}" baml query "$1"
}
# One JSON field of the first row of a query.
first() {
  local out
  out=$(expect 0 "$cli" query --format jsonl "$1" 2>/dev/null)
  printf '%s\n' "$out" | head -1 | python3 -c "import json, sys; print(json.loads(sys.stdin.readline())['$2'])"
}

echo "== recording: main twice, angry once (it throws, so baml run exits 1)"
expect 0 baml run main -- --label monday
expect 0 baml run main -- --label tuesday
expect 1 baml run angry | tail -1

main_id=$(first "SELECT execution_id FROM executions WHERE entry_fqn = 'user.main' ORDER BY started_unix_ns LIMIT 1" execution_id)
[ -n "$main_id" ] || { echo "demo: no main execution" >&2; exit 1; }

echo
echo "== 1. recent executions with outcomes and timing"
q "SELECT execution_id, entry_fqn, status, duration_ns, completed_calls, calls_retained, threads_total FROM executions WHERE entry_fqn LIKE 'user.%' ORDER BY started_unix_ns DESC"

echo
echo "== 2. threads and spawn tree of one execution"
q "SELECT thread_id, kind, parent_node_kind, parent_thread_id, spawn_fqn, start_offset_ns, duration_ns, end_status FROM threads WHERE execution_id = '$main_id' ORDER BY start_offset_ns"

echo
echo "== 3. most completed functions, and expensive calling contexts"
q "SELECT fqn, completed_calls, invocation_duration_sum_ns / completed_calls AS avg_ns FROM function_stats WHERE execution_id = '$main_id' AND fqn LIKE 'user.%' ORDER BY completed_calls DESC LIMIT 5"
q "SELECT fqn, depth, completed_calls, self_ns, inclusive_ns FROM hot_call_paths WHERE execution_id = '$main_id' ORDER BY self_ns DESC LIMIT 5"

echo
echo "== 4. recursion-aware timing, and retained calls vs the aggregate population"
q "SELECT fqn, normal_completed_calls AS outer_calls, reentry_completed_calls AS recursive_calls, inclusive_ns, invocation_duration_sum_ns, self_ns, self_time_state FROM call_path_stats WHERE execution_id = '$main_id' AND fqn = 'user.Fib'"
q "SELECT f.fqn, f.completed_calls AS population, (SELECT COUNT(*) FROM calls c WHERE c.execution_id = f.execution_id AND c.function_id = f.function_id) AS retained FROM function_stats f WHERE f.execution_id = '$main_id' AND f.fqn IN ('user.Fib', 'user.Classify', 'user.Summarize')"

echo
echo "== 5. one retained call: its parent, thread and calling context"
q "SELECT c.fqn, c.parent_node_kind, t.kind AS thread_kind, t.spawn_fqn, p.depth, p.caller_fqn, p.caller_pc, c.duration_ns FROM calls c JOIN threads t ON t.thread_id = c.thread_id JOIN call_paths p ON p.call_path_id = c.call_path_id WHERE c.execution_id = '$main_id' ORDER BY c.start_offset_ns"

echo
echo "== 6. filters on scalar and nested inputs; captured output"
q "SELECT args['review']['customer']['name'] AS customer, args['review']['stars'] AS stars, args['strict'] AS strict, output['sentiment'] AS sentiment, output['tags'][0] AS first_tag FROM calls WHERE fqn = 'user.Classify' AND args['review']['stars'] >= 4 AND args['review']['customer']['tier'] = 'gold'"
q "SELECT fqn, output FROM calls WHERE execution_id = '$main_id' AND fqn = 'user.Summarize'"

# Exact list content and equal class values across distinct invocations.
# MATERIALIZED filters the input before SQLite evaluates value comparisons.
q "SELECT COUNT(*) AS matching_lists FROM calls WHERE fqn = 'user.Classify' AND output['tags'] = baml_value_json('[\"support\",\"speed\"]')"
q "WITH samples AS MATERIALIZED (SELECT call_id, output FROM calls WHERE fqn = 'user.Classify' AND status = 'ok') SELECT COUNT(*) AS equal_output_pairs FROM samples a JOIN samples b ON a.output = b.output WHERE a.call_id < b.call_id"
test "$(first "SELECT COUNT(*) AS n FROM calls WHERE fqn = 'user.Classify' AND output['tags'] = baml_value_json('[\"support\",\"speed\"]')" n)" = 4
test "$(first "WITH samples AS MATERIALIZED (SELECT call_id, output FROM calls WHERE fqn = 'user.Classify' AND status = 'ok') SELECT COUNT(*) AS n FROM samples a JOIN samples b ON a.output = b.output WHERE a.call_id < b.call_id" n)" = 7

echo
echo "== 7. errored executions and errored retained calls (no throw provenance)"
q "SELECT execution_id, entry_fqn, status, retained_errored_calls FROM executions WHERE status = 'errored'"
q "SELECT fqn, status, error_state, error['detail'] AS detail, error['status_code'] AS http_status, parent_node_kind FROM error_calls"
# Refused by design: exit 2, like any SQL the reader cannot answer.
q "SELECT * FROM errors" 2

echo
echo "== 8. function and parameter metadata; schema discovery"
q "SELECT fqn, position, name FROM function_parameters WHERE fqn = 'user.Classify' AND recording_id = (SELECT recording_id FROM executions WHERE execution_id = '$main_id') ORDER BY position"
q "SELECT capability, support, relation FROM capabilities WHERE support != 'supported'"
echo
echo "\$ baml query --schema --table call_path_stats"
schema=$(expect 0 baml query --schema --table call_path_stats)
printf '%s\n' "$schema" | head -12

echo
echo "== 9. query while recording; each refresh reads only new files"
"$cli" run slow -- --rounds 12 >/dev/null 2>&1 &
slow=$!
live="SELECT r.indexed_sequence, r.seal_state, e.status, e.completed_calls FROM recordings r JOIN executions e ON e.recording_id = r.recording_id WHERE e.entry_fqn = 'user.slow'"
show_live() {
  local out
  out=$(expect 0 "$cli" query --format json "$live" 2>/dev/null)
  printf '%s' "$out" | python3 -c '
import json, sys
d = json.load(sys.stdin)
f = d["outcome"]["refresh"]
print("rows:", d["rows"], "| files decoded:", f["files_decoded"], "unchanged:", f["files_unchanged"])'
}
sleep 3
kill -0 "$slow" 2>/dev/null && echo "(slow is still running)"
show_live
sleep 2
show_live
status=0
wait "$slow" || status=$?
slow=
[ "$status" -eq 0 ] || { echo "demo: slow exited $status" >&2; exit 1; }
echo "(slow finished)"
show_live

echo
echo "== 10. missing and damaged evidence is reported, not answered"
cid=$(first "SELECT args_cid FROM calls WHERE fqn = 'user.Classify' AND args['review']['stars'] = 3 LIMIT 1" args_cid)
blob=$(find .baml/btel/cas -type f -name "$cid")
[ -n "$cid" ] && [ -n "$blob" ] || { echo "demo: no blob for $cid" >&2; exit 1; }
rm -- "$blob"
slow_recording=$(first "SELECT recording_id FROM executions WHERE entry_fqn = 'user.slow'" recording_id)
segment=".baml/btel/recordings/$slow_recording/00000000000000000003.btel"
[ -f "$segment" ] || { echo "demo: no $segment" >&2; exit 1; }
rm -- "$segment"
echo "(deleted blob $cid and file 3 of recording $slow_recording)"
q "SELECT args['review']['customer']['name'] AS customer, args_state, baml_value_state(args['review']) AS value_state FROM calls WHERE fqn = 'user.Classify' ORDER BY started_unix_ns" 1
q "SELECT COUNT(*) AS basic_reviews FROM calls WHERE fqn = 'user.Classify' AND args['review']['customer']['tier'] = 'basic'" 1
q "SELECT code, severity, sequence, message FROM issues" 1

echo
echo "== done: every command exited as expected"
