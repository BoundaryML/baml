# BTEL query demo

2026-09-24, after integration with #4958. Every output block below is copied
from one run of the demo script. Its transcript is
`documents/btel-query-demo/output.txt`; trailing spaces are removed in both
copies. The transcript was regenerated after
[recording completion](btel-query-recording-completion.md): each `baml run`
now ends its recording, so answers no longer say "unsealed". It also
includes the [population-outcome](btel-query-outcomes.md) and
[value-comparison](btel-query-value-comparisons.md) checks. Producer audits
and freezes are recorded on those pages.

## Run it

```bash
# Build the CLI (worktree root).
cd baml_language && cargo build -p baml_cli --bin baml-cli && cd ..

# Record and query. Needs python3 for the mock LLM server, which listens on
# a free local port (or MOCK_PORT).
documents/btel-query-demo/run_demo.sh
```

The script creates a new directory, copies `documents/btel-query-demo/baml_src`
into it, starts `mock_llm.py` (a fake OpenAI-compatible server) and waits
until it accepts connections, then records three runs with `baml run`. Every
CLI call must exit with the code the script expects, or the script stops
with an error: queries exit 0, `angry` exits 1 because it throws, the
refused `errors` query exits 2, and the damaged-evidence queries in step 10
exit 1 because they are incomplete. It deletes files only inside the
directory it created. `main` scores three reviews with an LLM function across
two spawned threads, computes `Fib(15)`, and asks for a summary. `angry`
sends a review the mock answers with HTTP 500, so it throws. LLM functions
capture their arguments, outputs and errors; that is what `args`, `output`
and `error` query.

## What it shows

**1. Recent executions, with outcome and timing.** `completed_calls` counts
every recorded invocation; `calls_retained` counts only individually kept
calls.

```text
$ baml query "SELECT execution_id, entry_fqn, status, duration_ns, completed_calls, calls_retained, threads_total FROM executions WHERE entry_fqn LIKE 'user.%' ORDER BY started_unix_ns DESC"
execution_id                       | entry_fqn  | status  | duration_ns | completed_calls | calls_retained | threads_total
-----------------------------------+------------+---------+-------------+-----------------+----------------+--------------
32b8702a679d47539295894224a405a3:2 | user.angry | errored | 39705326    | 211             | 1              | 1
78fc61071af447a597e1b535dea10774:2 | user.main  | ok      | 150763074   | 3200            | 4              | 3
0c23f0eb4a384311bbd4bd5a3311ac7d:2 | user.main  | ok      | 150143968   | 3200            | 4              | 3
(3 rows; complete; 3 recordings, 3 files indexed)
```

**2. The spawn tree of one execution.**

```text
$ baml query "SELECT thread_id, kind, parent_node_kind, parent_thread_id, spawn_fqn, start_offset_ns, duration_ns, end_status FROM threads WHERE execution_id = '0c23f0eb4a384311bbd4bd5a3311ac7d:2' ORDER BY start_offset_ns"
thread_id                          | kind  | parent_node_kind | parent_thread_id                   | spawn_fqn          | start_offset_ns | duration_ns | end_status
-----------------------------------+-------+------------------+------------------------------------+--------------------+-----------------+-------------+-----------
0c23f0eb4a384311bbd4bd5a3311ac7d:2 | root  | NULL             | NULL                               | NULL               | 0               | 150143968   | ok
0c23f0eb4a384311bbd4bd5a3311ac7d:3 | spawn | thread           | 0c23f0eb4a384311bbd4bd5a3311ac7d:2 | .<lambda(main, 0)> | 375971          | 149689596   | ok
0c23f0eb4a384311bbd4bd5a3311ac7d:4 | spawn | thread           | 0c23f0eb4a384311bbd4bd5a3311ac7d:2 | .<lambda(main, 1)> | 468122          | 79835985    | ok
(3 rows; complete; 3 recordings, 3 files indexed)
```

**3. The most-called functions, and where self time goes.** The largest
self times are in `baml.sap.parse`, which parses the LLM responses.

```text
$ baml query "SELECT fqn, completed_calls, invocation_duration_sum_ns / completed_calls AS avg_ns FROM function_stats WHERE execution_id = '0c23f0eb4a384311bbd4bd5a3311ac7d:2' AND fqn LIKE 'user.%' ORDER BY completed_calls DESC LIMIT 5"
fqn                | completed_calls | avg_ns
-------------------+-----------------+---------
user.Fib           | 1973            | 11016
user.Weight        | 4               | 2045
user.Classify      | 3               | 76037111
user.Classify@spec | 3               | 140753
user.Score         | 3               | 76151432
(5 rows; complete; 3 recordings, 3 files indexed)

$ baml query "SELECT fqn, depth, completed_calls, self_ns, inclusive_ns FROM hot_call_paths WHERE execution_id = '0c23f0eb4a384311bbd4bd5a3311ac7d:2' ORDER BY self_ns DESC LIMIT 5"
fqn            | depth | completed_calls | self_ns  | inclusive_ns
---------------+-------+-----------------+----------+-------------
baml.sap.parse | 9     | 2               | 32540684 | 32540684
baml.sap.parse | 7     | 2               | 31149728 | 31149728
baml.sap.parse | 9     | 1               | 18993964 | 18993964
baml.sap.parse | 5     | 1               | 18269321 | 18269321
baml.sap.parse | 7     | 1               | 16526193 | 16526193
(5 rows; complete; 3 recordings, 3 files indexed)
```

**4. Recursion, and retained calls next to the population.** `Fib(15)` is
1,973 calls: 1 outermost call and 1,972 recursive re-entries on the same
calling context. `inclusive_ns` counts the recursion once (2.5 ms). Summing
every invocation's duration counts nested time again and again (24.0 ms).
`Fib` is never retained, so `calls` has no rows for it while the population
has 1,973.

```text
$ baml query "SELECT fqn, normal_completed_calls AS outer_calls, reentry_completed_calls AS recursive_calls, inclusive_ns, invocation_duration_sum_ns, self_ns, self_time_state FROM call_path_stats WHERE execution_id = '0c23f0eb4a384311bbd4bd5a3311ac7d:2' AND fqn = 'user.Fib'"
fqn      | outer_calls | recursive_calls | inclusive_ns | invocation_duration_sum_ns | self_ns | self_time_state
---------+-------------+-----------------+--------------+----------------------------+---------+----------------
user.Fib | 1           | 1972            | 2226499      | 21734986                   | 2226499 | valid
(1 row; complete; 3 recordings, 3 files indexed)

$ baml query "SELECT f.fqn, f.completed_calls AS population, (SELECT COUNT(*) FROM calls c WHERE c.execution_id = f.execution_id AND c.function_id = f.function_id) AS retained FROM function_stats f WHERE f.execution_id = '0c23f0eb4a384311bbd4bd5a3311ac7d:2' AND f.fqn IN ('user.Fib', 'user.Classify', 'user.Summarize')"
fqn            | population | retained
---------------+------------+---------
user.Classify  | 3          | 3
user.Summarize | 1          | 1
user.Fib       | 1973       | 0
(3 rows; complete; 3 recordings, 3 files indexed)
```

**5. Retained calls in context: parent, thread and call site.** The
`Classify` calls ran on the two spawned threads, called from `Score` at
bytecode pc 32. No source line is shown because none is recorded.

```text
$ baml query "SELECT c.fqn, c.parent_node_kind, t.kind AS thread_kind, t.spawn_fqn, p.depth, p.caller_fqn, p.caller_pc, c.duration_ns FROM calls c JOIN threads t ON t.thread_id = c.thread_id JOIN call_paths p ON p.call_path_id = c.call_path_id WHERE c.execution_id = '0c23f0eb4a384311bbd4bd5a3311ac7d:2' ORDER BY c.start_offset_ns"
fqn            | parent_node_kind | thread_kind | spawn_fqn          | depth | caller_fqn | caller_pc | duration_ns
---------------+------------------+-------------+--------------------+-------+------------+-----------+------------
user.Classify  | thread           | spawn       | .<lambda(main, 1)> | 5     | user.Score | 32        | 79257922
user.Classify  | thread           | spawn       | .<lambda(main, 0)> | 5     | user.Score | 32        | 77977156
user.Summarize | thread           | root        | NULL               | 1     | user.main  | 219       | 76626250
user.Classify  | thread           | spawn       | .<lambda(main, 0)> | 5     | user.Score | 32        | 70876255
(4 rows; complete; 3 recordings, 3 files indexed)
```

**6. Filters on nested captured inputs, and captured outputs.** Both `main`
runs match, so each review appears twice. The last two queries compare whole
captured values: a list literal, and equal class outputs across calls.

```text
$ baml query "SELECT args['review']['customer']['name'] AS customer, args['review']['stars'] AS stars, args['strict'] AS strict, output['sentiment'] AS sentiment, output['tags'][0] AS first_tag FROM calls WHERE fqn = 'user.Classify' AND args['review']['stars'] >= 4 AND args['review']['customer']['tier'] = 'gold'"
customer | stars | strict | sentiment | first_tag
---------+-------+--------+-----------+----------
Ann      | 5     | false  | positive  | support
Ann      | 4     | false  | positive  | support
Ann      | 5     | false  | positive  | support
Ann      | 4     | false  | positive  | support
(4 rows; complete; 3 recordings, 3 files indexed)

$ baml query "SELECT fqn, output FROM calls WHERE execution_id = '0c23f0eb4a384311bbd4bd5a3311ac7d:2' AND fqn = 'user.Summarize'"
fqn            | output
---------------+----------------------------------------------
user.Summarize | Customers like the support; delivery is slow.
(1 row; complete; 3 recordings, 3 files indexed)

$ baml query "SELECT COUNT(*) AS matching_lists FROM calls WHERE fqn = 'user.Classify' AND output['tags'] = baml_value_json('["support","speed"]')"
matching_lists
--------------
4
(1 row; complete; 3 recordings, 3 files indexed)

$ baml query "WITH samples AS MATERIALIZED (SELECT call_id, output FROM calls WHERE fqn = 'user.Classify' AND status = 'ok') SELECT COUNT(*) AS equal_output_pairs FROM samples a JOIN samples b ON a.output = b.output WHERE a.call_id < b.call_id"
equal_output_pairs
------------------
7
(1 row; complete; 3 recordings, 3 files indexed)
```

**7. Failures, without inventing where they were thrown.** `error_calls` has
one row per retained call that ended in an error. The old throw-oriented
`errors` relation is refused with an explanation, not answered with an
empty table.

```text
$ baml query "SELECT execution_id, entry_fqn, status, retained_errored_calls FROM executions WHERE status = 'errored'"
execution_id                       | entry_fqn  | status  | retained_errored_calls
-----------------------------------+------------+---------+-----------------------
32b8702a679d47539295894224a405a3:2 | user.angry | errored | 1
(1 row; complete; 3 recordings, 3 files indexed)

$ baml query "SELECT fqn, status, error_state, error['detail'] AS detail, error['status_code'] AS http_status, parent_node_kind FROM error_calls"
fqn           | status  | error_state | detail   | http_status | parent_node_kind
--------------+---------+-------------+----------+-------------+-----------------
user.Classify | errored | reference   | http 500 | 500         | thread
(1 row; complete; 3 recordings, 3 files indexed)

$ baml query "SELECT * FROM errors"
error: `errors` from the old tracer is not available: throw occurrences are not recorded (no throw identity, site or propagation links); query error_calls for retained calls that ended in an error
```

**8. Recorded parameter names, and what is supported.** The names come from
the recording, not from today's source files. `client` and `on_event` are
parameters the compiler adds to LLM functions; both LLM functions here have
them.

```text
$ baml query "SELECT fqn, position, name FROM function_parameters WHERE fqn = 'user.Classify' AND recording_id = (SELECT recording_id FROM executions WHERE execution_id = '0c23f0eb4a384311bbd4bd5a3311ac7d:2') ORDER BY position"
fqn           | position | name
--------------+----------+---------
user.Classify | 0        | review
user.Classify | 1        | strict
user.Classify | 2        | client
user.Classify | 3        | on_event
(4 rows; complete; 3 recordings, 3 files indexed)

$ baml query "SELECT capability, support, relation FROM capabilities WHERE support != 'supported'"
capability                                            | support     | relation
------------------------------------------------------+-------------+---------------------
structured value equality                             | partial     | calls
entry function                                        | partial     | executions
source locations                                      | partial     | function_definitions
failure ancestry                                      | partial     | error_calls, calls
live recordings                                       | partial     | recordings
throw occurrences (old errors relation)               | unsupported |
invocation starts and active calls                    | unsupported |
latency distributions                                 | unsupported |
await counts                                          | unsupported |
thread names                                          | unsupported |
producer process and version (old processes relation) | unsupported |
capture loss reasons (old health relation)            | unsupported |
old store internals (store_files, value_index)        | unsupported | recording_files
(13 rows; complete; 3 recordings, 3 files indexed)

$ baml query --schema --table call_path_stats
call_path_stats: Population timing per calling context, recursion-aware. Counts are completed invocations (not starts) over every recorded invocation, not only retained calls.
  recording_id               text
  execution_id               text
  call_path_id               text
  parent_call_path_id        text
  depth                      integer
  function_id                text
  fqn                        text
  definition_key             text
  kind                       text
  origin                     text
  edge_kind                  text         root, call, spawn
```

**9. Querying while a program is still recording.** `slow` runs about six
seconds, and the recorder publishes a file about once a second. The script
queries twice while it runs, waits for it to exit, and queries again. Each
query applies only the files that appeared since the previous one. The
execution is `incomplete` until its completion is indexed, then `ok`. The
recording is `unsealed` while `slow` runs. Its last file carries the end
marker, so the final query shows it `sealed`.

```text
(slow is still running)
rows: [[2, 'unsealed', 'incomplete', 1864]] | files decoded: 2 unchanged: 3
rows: [[4, 'unsealed', 'incomplete', 3728]] | files decoded: 2 unchanged: 5
(slow finished)
rows: [[7, 'sealed', 'ok', 5593]] | files decoded: 3 unchanged: 7
```

**10. Missing evidence is reported, not answered.** The script deletes one
captured blob and one recording file. Both `main` runs captured the same
review for Bob, so they share that blob. The rows keep their calls and say
why the value is missing. The count is marked incomplete instead of looking
like a clean answer, and the recording's index stops at the gap. The deleted
file belonged to a sealed recording, but its end marker now lies beyond the
gap, so the answers say "unsealed" again until the file returns.

```text
$ baml query "SELECT args['review']['customer']['name'] AS customer, args_state, baml_value_state(args['review']) AS value_state FROM calls WHERE fqn = 'user.Classify' ORDER BY started_unix_ns"
customer | args_state | value_state
---------+------------+------------
Ann      | reference  | present
Ann      | reference  | present
NULL     | reference  | cas_missing
Ann      | reference  | present
Ann      | reference  | present
NULL     | reference  | cas_missing
Carl     | reference  | present
(7 rows; incomplete: some evidence was unavailable; 4 recordings, 5 files indexed; unsealed: the prefix may grow)
warning[cas_missing] x1: captured value blob not found in the CAS directory
warning[recording_gap] x1: recording a4d3520325e64ebb9a0b32fcd400cceb is indexed through file 2; file 3 stops indexing (see the issues relation)

$ baml query "SELECT COUNT(*) AS basic_reviews FROM calls WHERE fqn = 'user.Classify' AND args['review']['customer']['tier'] = 'basic'"
basic_reviews
-------------
1
(1 row; incomplete: some evidence was unavailable; 4 recordings, 5 files indexed; unsealed: the prefix may grow)
warning[cas_missing] x1: captured value blob not found in the CAS directory
warning[recording_gap] x1: recording a4d3520325e64ebb9a0b32fcd400cceb is indexed through file 2; file 3 stops indexing (see the issues relation)

$ baml query "SELECT code, severity, sequence, message FROM issues"
code         | severity | sequence | message
-------------+----------+----------+------------------------------------------------
sequence_gap | error    | 3        | file 3 is missing; files 3 to 7 are not indexed
(1 row; incomplete: some evidence was unavailable; 4 recordings, 5 files indexed; unsealed: the prefix may grow)
warning[recording_gap] x1: recording a4d3520325e64ebb9a0b32fcd400cceb is indexed through file 2; file 3 stops indexing (see the issues relation)
```

## Questions by support level

**Supported:** executions with outcome and timing; threads and the spawn tree;
calling contexts with caller and bytecode pc; completed-call counts per
context and function; recursion-aware inclusive, child, await and self time;
retained calls with captured args, output and error, filtered with brackets;
errored retained calls; function metadata and parameter names; clock
definitions and validity; evidence issues and the indexed extent; sealed
recordings and final clocks after a normal shutdown.

**Partial:** the entry function (only when one top-level call is recorded);
source locations (definition spans only, no call-site lines); failure ancestry
(the recorded chain of retained calls, not a throw stack); live recordings
(an unsealed prefix, with no liveness recorded); structured value equality
(`=` and `!=` only, no ordering or value grouping).

**Only new recordings supply:** success, error and cancellation counts for
aggregated calls through `call_path_stats` and `function_stats`; the end
marker and final clock states. Older recordings answer with NULL counts, stay
`unsealed` and report `is_final = 0`.

**Unsupported:** throw occurrences and their
propagation; invocation starts and active calls; latency percentiles, minimum
and maximum for aggregated calls; await counts; thread names; producer
process identity; why a value was not captured. `SELECT * FROM capabilities`
lists these with the reason.

## The playground

The playground reads the same index. Opening one of these runs shows its
threads, calling contexts and retained calls. New recordings show succeeded,
errored and cancelled completion counts in the calling-context inspector,
including calls without retained spans. The opened-run overview shows a
population error count only when its contexts cover the full known population.
Where the recording has no number (invocation starts, or outcome counts in
older recordings), the views say "not recorded" or show "—" instead of 0. A run
with no recorded end shows as `incomplete`, not `running`. `angry`'s failure is
listed as a retained call that ended in an error, with its value and a note
that where it was thrown is not recorded. It is not presented as a distinct
throw with a stack.

That UI behavior is checked by TypeScript tests: the adapter's unit tests
and static renders of the overview and the context inspector. Nobody has
clicked through it in VS Code or a browser yet.

## Next

The reader phase is done: tests for the new relations and SQL behavior, the
playground's thread and calling-context views and their correctness
follow-up, and the implementation record in
[btel-query-prototype.md](btel-query-prototype.md#reader-phase-2026-09-24).
The [short reader checks](btel-query-performance.md) and first reader
optimization are implemented. See the [outcome phase](btel-query-outcomes.md)
for the subsequent measured processor/reader extension.
Further statistics optimization is deferred. The
[remaining parity review](btel-query-parity-review.md) recommends structured
value comparisons, now [implemented](btel-query-value-comparisons.md) entirely
in the reader; step 6 above shows them. Recorded source-site metadata is the
next candidate for a producer extension.
