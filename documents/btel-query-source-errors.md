# Call/spawn sites and error origins

Implementation and validation record for the
[audit](btel-query-source-errors-audit.md), on top of `2e8be7ca1`.
Recordings now say which expression called a function or spawned a thread,
and where each error started. `baml query` and the playground both read it.

Follow-up review found and fixed stale handler notes that could falsely
prove a conversion's origin or obscure a later rethrow. A later probe showed
that even a running catch cannot prove a conversion's source, so an
`UnknownError` conversion now never establishes a proven link. When its
throw carries an earlier throw's context, it is `unresolved` with reason
`source_not_recorded`. A conversion of a value never thrown before is a
fresh raise. See the
[review and regression tests](btel-query-error-origin-review.md) for the
current proof rules and their limits. The outdated sealing-test assertion
reported below is also fixed; its 18 targeted recording, cloud and
completion tests pass. The initial full-suite result below is historical.

The [follow-up performance investigation](btel-query-throw-performance.md)
includes the lifetime fix. It predates the conversion change, which does not
run on fresh throws. Its final paired throw-heavy batch measured
148.03 ms at HEAD and 186.74 ms with error evidence (+26.2%). Four allocation
and stack-reuse experiments failed to improve the existing implementation;
none were retained. This does not resolve the original roughly 30% overhead,
and the workload does not measure rethrow or conversion-origin lookup cost.

## What it answers now

This is a small program run twice with `baml run`: `main` catches one error,
awaits a failing child twice, and `angry` lets an error escape. The class
and blank lines are left out below; the comments give the real line numbers.

```baml
function Check(n: int) -> int throws Failure {
  if (n > 2) { throw Failure { code: n } }          // line 6
  n
}
function Step(n: int) -> int throws Failure {
  Check(n) + 1                                       // line 11
}
function main(n: int) -> int {
  let local = Check(1);                              // line 15
  let caught = Step(n) catch (e) { Failure => 0 };   // line 16
  let child = spawn { Step(n) };                     // line 17
  let first = (await child) catch (e) { Failure => 10 };
  let again = (await child) catch (e) { Failure => 20 };
  local + caught + first + again
}
function angry(n: int) -> int {
  Step(n)                                            // line 24
}
```

Every call and spawn expression has a line. The entry calls have none,
because nothing in BAML called them:

```text
$ baml query "SELECT caller_fqn, fqn, edge_kind, call_site_state, call_site_file, call_site_line FROM call_paths WHERE fqn LIKE 'user.%' OR fqn LIKE '%lambda%' ORDER BY recording_id, local_call_path_id"
caller_fqn         | fqn                | edge_kind | call_site_state | call_site_file     | call_site_line
-------------------+--------------------+-----------+-----------------+--------------------+---------------
NULL               | user.angry         | root      | no_caller       | NULL               | NULL
user.angry         | user.Step          | call      | resolved        | baml_src/main.baml | 24
user.Step          | user.Check         | call      | resolved        | baml_src/main.baml | 11
NULL               | user.main          | root      | no_caller       | NULL               | NULL
user.main          | user.Check         | call      | resolved        | baml_src/main.baml | 15
user.main          | user.Step          | call      | resolved        | baml_src/main.baml | 16
user.Step          | user.Check         | call      | resolved        | baml_src/main.baml | 11
user.main          | .<lambda(main, 0)> | spawn     | resolved        | baml_src/main.baml | 17
NULL               | .<lambda(main, 0)> | call      | no_caller       | NULL               | NULL
.<lambda(main, 0)> | user.Step          | call      | resolved        | baml_src/main.baml | 17
user.Step          | user.Check         | call      | resolved        | baml_src/main.baml | 11
(11 rows; complete; 2 recordings, 2 files indexed)

$ baml query "SELECT kind, spawn_fqn, spawn_site_state, spawn_site_line FROM threads ORDER BY recording_id, kind DESC"
kind  | spawn_fqn          | spawn_site_state | spawn_site_line
------+--------------------+------------------+----------------
root  | NULL               | not_spawned      | NULL
spawn | .<lambda(main, 0)> | resolved         | 17
root  | NULL               | not_spawned      | NULL
(3 rows; complete; 2 recordings, 2 files indexed)
```

Every time unwinding starts is a raise. The child's throw escapes the
child; each `await` passes it along, and both awaits name the child's raise
as their origin:

```text
$ baml query "SELECT x.entry_fqn, e.kind, e.fqn, e.site_line, e.origin_state, e.origin_via, e.unwind_result, e.handler_fqn, e.stack_depth FROM error_raises e JOIN executions x ON x.execution_id = e.execution_id ORDER BY e.raised_at"
entry_fqn  | kind  | fqn        | site_line | origin_state | origin_via | unwind_result | handler_fqn | stack_depth
-----------+-------+------------+-----------+--------------+------------+---------------+-------------+------------
user.main  | throw | user.Check | 6         | fresh        | NULL       | caught        | user.main   | 3
user.main  | throw | user.Check | 6         | fresh        | NULL       | unhandled     | NULL        | 3
user.main  | await | user.main  | 18        | proven       | await      | caught        | user.main   | 1
user.main  | await | user.main  | 19        | proven       | await      | caught        | user.main   | 1
user.angry | throw | user.Check | 6         | fresh        | NULL       | unhandled     | NULL        | 3
(5 rows; complete; 2 recordings, 2 files indexed)
```

`error_occurrences` has one row per error. The two throws at line 6 in
`main` are two errors, even though they have equal values. The child's
error was raised once and passed along twice, so it has three raises:

```text
$ baml query "SELECT kind, fqn, site_file, site_line, raises, unhandled_raises FROM error_occurrences ORDER BY raised_at"
kind  | fqn        | site_file          | site_line | raises | unhandled_raises
------+------------+--------------------+-----------+--------+-----------------
throw | user.Check | baml_src/main.baml | 6         | 1      | 0
throw | user.Check | baml_src/main.baml | 6         | 3      | 1
throw | user.Check | baml_src/main.baml | 6         | 1      | 1
(3 rows; complete; 2 recordings, 2 files indexed)

$ baml query "SELECT f.position, f.fqn, f.site_line FROM error_frames f JOIN error_raises e ON e.raise_id = f.raise_id JOIN executions x ON x.execution_id = e.execution_id WHERE x.entry_fqn = 'user.angry' ORDER BY f.position"
position | fqn        | site_line
---------+------------+----------
0        | user.Check | 6
1        | user.Step  | 11
2        | user.angry | 24
(3 rows; complete; 2 recordings, 2 files indexed)

$ baml query "SELECT * FROM errors"
error: `errors` from the old tracer is not available: query error_occurrences for distinct errors, error_raises for every throw/rethrow with its origin, error_frames for stacks, and error_call_links or error_calls for the retained calls that failed
```

Lines come from the recording, not from today's files. After two lines
were added to the top of `main.baml`, the answer did not move:

```text
$ baml query "SELECT kind, fqn, site_line FROM error_occurrences ORDER BY raised_at LIMIT 1"
kind  | fqn        | site_line
------+------------+----------
throw | user.Check | 6
```

The playground compares the recording's source identity with the program it
has built now. It marks every link `verified`, `stale` or `unverified`, and
a stale link says "(changed since run)".

This program has no retained calls: by default only LLM functions are.
With retained calls, `error_call_links` and the new `calls` columns say
which raise failed each one; the tests below cover that.

## Decisions

### Source maps

- The recording carries each referenced function's line table, copied from
  the compact code the VM executes. PCs are byte offsets in that code, the
  same unit `caller_pc` already used. The map says how long the code is,
  so the reader rejects a PC past the end.
- The copy happens once, when an engine with a recording starts, beside the
  existing function metadata. Calls never look up lines.
- The reader resolves `caller_pc` in the caller's map. The file comes from
  the caller's metadata. An entry in a different compiler file than the
  function's own is reported as `foreign_file`, not resolved.
- A direct recursive call reuses its caller's call path and records no PC
  of its own. The path keeps its entry site; the recursive invocation's own
  site is `recursive_reentry`, with no line. The normal call path is
  unchanged, so this stays a limit.

### Error identity

- Every time the VM starts unwinding, it records a raise with a fresh
  telemetry ID. Two throws get two raises, even with equal values.
- An occurrence is identified by its first raise. A `throw`, a runtime
  panic, a native failure and a host failure each start one.
- A raise that passes an error along (a rethrow, a failed await) names its
  origin only when the VM proved it. Otherwise it is `ambiguous`, with how
  many origins matched, or `unresolved`, with a reason. Values, CAS IDs and
  traces are never used to guess.
- An `UnknownError` conversion never establishes a proven link. See below.

### How origins are proven

- **Rethrow.** When unwinding lands in a handler, the VM notes which raise
  put the error into which stack slot of which frame and function. A note
  counts only while its handler is active at that frame's current PC, has
  not been superseded, and its slot is safe to use as evidence. One origin
  among valid matching notes proves it. Two or more is `ambiguous`; missing
  or unsafe evidence is `unresolved`. The
  [review](btel-query-error-origin-review.md) explains the lifetime and
  slot-write checks, including forwarded errors that remain unresolved.
- The notes store stack indexes and IDs, never heap values, so GC ignores
  them. A new landing replaces an older note for the same handler. Notes of
  popped frames go at the next raise. At most 1024 notes are kept; once one
  is dropped, lookups say `landings_evicted` until the next top-level run.
  A dropped note could otherwise leave a stale note as the only match.
- **Await.** When a spawned thread fails, the engine asks its VM for the
  raise that escaped the thread. It stores that raise and its origin under
  the future's ID before the future wakes its awaiters. A failed `await`
  reads the entry and never removes it, so every await of that future
  finds the same origin. The engine keeps 4096 entries and evicts the
  oldest first. An await that finds nothing says `future_link_missing`, or
  `future_link_evicted` when an entry that old was dropped.
- **`UnknownError.from`: not proven.** The VM carries the source error's
  trace to the next throw of the converted value. It finds that trace by
  value, and the conversion only receives a value. An unrelated equal value
  matches too, such as the same interned string obtained from another call,
  even inside the catch that is still running. So when the converted
  value's throw carries such a context, its raise is `unresolved` via
  `normalization`, with reason `source_not_recorded`. The carried trace is
  kept as an inherited trace, which is weaker evidence. This holds for
  `from(e)` too. A conversion of a value that was never thrown carries no
  context and is a fresh raise.

### Transport

- The VM emits one boxed `ErrorRaised` span record when unwinding starts and
  one scalar `ErrorUnwindEnded` when it stops. The failed-call completions in
  between are unchanged. A throw caught in the same frame has a raise and an
  end, and no failed call.
- The raise frame may be timing-only and get promoted when it completes; its
  call ID only exists then. The VM emits a scalar marker right after
  completing that frame, and the recorder attaches the promoted ID.
- The recorder keeps one active unwind per telemetry thread and links every
  function completion on that thread to it until its end. A thread
  completion or a new raise on the thread clears an unwind that never
  ended, so it claims nothing later. Other threads' records can sit between
  a raise and its end, in the same or other chunks; they are never linked.
- Raises, links and ends go to a separate error section of the file,
  counted like other metadata. They never use the fixed 128-byte span-event
  reservation. A stack keeps its innermost 64 frames and says how many there
  were. A file without errors has no error section.
- Only publishers that write recordings ask for error evidence. With no
  recording, or after recording is disabled, the VM builds none.

## Query and playground surface

- `error_calls` keeps its meaning: retained calls that failed.
- `error_raises`: every raise, with kind, site, origin state, previous
  raise, handler, unwind result, failed-call count, stack depth and an
  inherited trace when the origin is not proven.
- `error_occurrences`: one row per fresh raise, with how many raises passed
  it along, the calls they failed, how many escaped, and the captured error
  value of a failed call.
- `error_frames`: each raise's stack, innermost first, with sites.
- `error_call_links`: the raise frame's call and every call a raise unwound.
- `call_paths`, `threads` and `calls` gain site columns with a state.
  `calls` also gains `error_raise_id`, `error_occurrence_id` and
  `error_link_state`.
- Older recordings keep failed calls, with `error_link_state` =
  `not_recorded` and site states `no_source_map`.
- The playground shows one error per occurrence. Rethrows and awaits appear
  as steps under it, with their own sites. Ambiguous and unresolved raises
  are listed but not counted as errors. A native or host failure reads
  "failed in the native call at …". Call, spawn, raise and step sites open
  through the existing source navigation. Older recordings still show
  errored calls.

## Compiler metadata fixes

Checking real recordings against the source text found five line-table
spans that labelled the wrong expression. Only metadata changed; the
bytecode is the same.

| Instruction | Before | After |
| --- | --- | --- |
| `spawn` | `{ Middle(n) }` | `spawn { Middle(n) }` |
| `throw` | `Failure { code: n }` | `throw Failure { code: n }` |
| `await` | `child` | `await child` |
| An instruction using an inlined operand, e.g. `/` | `n - n` | `10 / (n - n)` |
| Any lambda's definition span | file 0, `[0, 0)` | the lambda expression |

The last one made every call inside a lambda `foreign_file`. The first four
follow what call emission already did: restore the enclosing span after
pulling operands.

## Changes by layer

### Hot paths and layouts: unchanged

- Executable `Function`, `BytecodeFrame`/`Frame` (still asserted at 88 bytes
  plus a pointer), `FrameTelemetry` (32), `Future`, value and GC layouts.
- `TimingRecord` stays 32 bytes. The span slot stays 56 bytes, 8-aligned;
  the boxed raise is one pointer. A test asserts both.
- No instruction, call, return, await or spawn path gained work. The vm.rs
  diff touches only the unwinder, host injection, and one `Option` check
  when a top-level run starts on an empty stack. `UnknownError` context
  preservation is unchanged from HEAD.

### Cold data structures that changed

- `btel_types::FunctionMetadata` gains `source_map`. The static table
  copied at recording startup now includes every compile-time function's
  line table.
- `btel_records::SpanRecord` gains `ErrorRaised(Box<ErrorRaise>)`,
  `ErrorRaiseFrameCompleted` and `ErrorUnwindEnded`, plus the owned
  `ErrorRaise`, frame, origin and link types.
- `btel_processor::Publisher` gains `ERROR_EVIDENCE` (default false).
  `TelemetryRuntime` gains that flag and the bounded future-link store.
- `bex_vm::telemetry::TelemetryState` gains one pointer: the landing notes
  and escaped raise, allocated on the first recorded raise.
- The recorder's conversion buffer gains the active-unwind list and the
  error section.
- Wire format minor 2: `FunctionMetadata.source_map` (field 15) and
  `RecordingFile.errors` (field 8). `UnresolvedReason` includes
  `SOURCE_NOT_RECORDED` (6) for conversions. Readers accept any minor.
- Index schema 6: source maps on `function_def`, site columns on
  `call_path`, and tables `error_raise`, `error_frame`, `error_link`.

### Other changed files

- Engine: `settle_child_errored` stores the escaping raise before settling;
  `BexEngine::source_snapshot_id()` is new.
- Playground server: passes the installed program's source identity to the
  telemetry adapter.
- Cloud contract fixtures: the header's minor byte `10 01` became `10 02`
  in each fixture recording; digests updated, lengths unchanged (see the
  fixture README).
- Benchmark harness: two new workloads; the old five compile the same
  programs as before.

## Limits

- A direct recursive re-entry has no call site of its own.
- An `UnknownError` conversion never establishes a proven link, even
  `from(e)`. When it carries an earlier throw's context, the recording keeps
  that trace as an inherited trace. Linking it would need a record of which value the conversion
  received.
- The same error object caught by nested handlers and rethrown is
  `ambiguous`: the bytecode does not say which binding the rethrow read.
- Native and host failures are placed at the BAML call that entered them.
  The recording does not name the native function that failed. A host
  failure that passes along an earlier error is `unresolved`.
- A thrown error while another is being handled is its own occurrence. The
  "during handling of" relation (the language's `ErrorContext` cause) is
  not recorded.
- The raise frame's promoted call is attached through the marker. No
  public policy promotes errored calls today, so this is covered by VM and
  recorder unit tests, not by a real engine recording.
- A raise's own value is not captured. `error_occurrences.error` reads a
  failed retained call's captured error. A throw caught in place has none.
- Timing-only calls fail without IDs. They are counted in
  `call_path_stats`, never linked.
- An engine fault that is not an unwind (`TracedInternalError`) has no
  raise. If the unwinder itself fails, the raise ends `aborted`.
- A cancelled child that never ran an await records no raise of its own.
  The parent's await raises `await_cancelled`.
- Stacks keep 64 frames; inherited traces keep 16, with names and files
  clipped to 256 bytes.
- The defer pad's rethrow is placed at the block that ran the defers.
- A native function that fails after one of its callbacks returned is
  located with the PC the unwinder uses for that frame. That path was not
  exercised by these tests.
- The playground verifies sources against the program it last built, not
  unsaved edits. `baml query` shows recorded lines without checking today's
  files.
- WASM builds record no error evidence.

## Tests

Validation on Rust 1.98.0 with `CARGO_INCREMENTAL=0`.

Real recordings (`baml_query_btel`):

- `sites`: two sites of one function, nested operands, indirect `f(4)`,
  method `box.get()`, a native `map` callback, two spawns, child wrappers,
  direct recursion. Each resolved site is compared with the source text.
- `error_origins`:
  - `raises_are_distinct_and_only_proven_origins_are_linked`: a throw caught
    in place, propagation through three retained frames, equal-valued
    throws, rethrow, the same object caught by nested handlers (ambiguous,
    2 candidates), a throw while handling, defer, `UnknownError`
    conversion (`unresolved`, `source_not_recorded`, with a complete
    inherited trace), JSON parse, division by zero, a missing file (native,
    runtime, host), cancellation, and an escaping error.
  - `awaits_keep_the_childs_origin_and_equal_children_stay_distinct`:
    repeated awaits, parent catch, two children throwing equal values.
  - `interleaved_deep_unwinds_link_each_call_to_its_own_raise`: eight
    children unwind 121 retained frames each at once, across 256-record
    chunks. No call is linked to another thread's raise; stacks truncate
    to 64.
  - `a_collection_between_catch_and_rethrow_keeps_the_origin`: a major GC
    runs between the catch and the rethrow.
- `site_error_evidence`: an older recording, a caller definition arriving in
  a later file, out-of-range, sentinel, unmapped and foreign-file PCs, a
  malformed map, a live prefix with a raise but no end, an end without its
  raise, a proven origin that is not indexed, a conflicting duplicate raise,
  a second raise claiming a call, and a malformed raise.

Unit tests:

- `btel_reader::source_map`: resolution and rejection.
- `btel_recorder::errors`: interleaved threads, an unwind with no end
  cleared by a new raise and by thread completion, the promotion marker,
  an error-free file without an error section.
- `bex_vm` `error_evidence`: a promoted raise frame completes before its
  marker; no records are built without a recording.
- `bex_vm` `telemetry::errors`: landing-note matching, replacement and
  eviction.
- `btel_records`: span and timing slot sizes.
- `baml_lsp_server` `playground_btel`: verified, stale and unverified
  sources; call-site line and span; a recorded throw with site, stack,
  failed calls and value.
- Playground TypeScript: mapping of origin, boundary, propagation and
  verification; spawn sites; no invented site; an overview render.

Results:

- `nextest` over `btel_types`, `btel_records`, `btel_settings`,
  `btel_processor`, `btel_recorder`, `btel_file`, `btel_bcs`, `btel_reader`,
  `baml_query_btel`, `bex_vm_types`, `bex_vm`, `bex_engine`,
  `baml_compiler2_emit` and `baml_compiler2_mir`: 909 passed, 1 failed,
  7 skipped. The failure is `bex_engine::telemetry_recording`
  `execution_reaches_encoded_files_and_shutdown_drains_once`. It asserts
  that no file of a normally shut-down recording has an end marker. HEAD's
  sealing commit writes one (its own `telemetry_files` test asserts that)
  and did not update this older test. This change does not touch sealing.
  I did not rebuild HEAD's tests to confirm it fails there too.
- `baml_lsp_server` library: 76 passed. `baml_cli` `query_e2e`: 2 passed.
- `baml_tests` subsets that read traces, line tables or spawn and error
  semantics: `errors`, `exceptions`, `errorcontext`, `future_all_settled`,
  `emit_determinism`, `spawn_semantics`, `cancel_cascade`, `cleanup`,
  `runtime_function_identity`, `optimizer_stack`, `optimization`,
  `short_circuit_locals`, `runtime_diagnostic_consistency` (65 passed) and
  `mounted_package_parity` with `emit_determinism` (15 passed). The whole
  97-binary `baml_tests` suite was not run.
- Clippy with warnings denied, all targets, on the 15 changed crates, plus
  the `bex_engine` examples: clean. `rustfmt --check` with the repository's
  import options on every changed Rust file: clean.
- Playground: `vitest` 248 passed (29 files), `tsc --noEmit` clean, Biome
  clean on the changed files.
- The CLI demo above ran on a debug `baml-cli` built from this worktree.

## Performance

Paired release binaries on this workstation (AMD Ryzen 9 5950X), Rust
1.98.0, `CARGO_INCREMENTAL=0`, with the existing short harness:
monotonic clock, four Tokio workers, a major GC every eight roots, 1 MiB
files, 4 KiB capture payload. Each workload ran one warmup and three
measured repetitions per binary and mode, in randomized interleaved order,
with telemetry-off controls. Measurement took 103.4 s, plus a 20.4 s
follow-up on one workload: 123.8 s in total. Builds were separate (241 s
candidate, 297 s HEAD).

- **Before:** HEAD `2e8be7ca1`, built from `git archive` with the new
  harness file. The five original workloads compile exactly the programs
  the earlier harness did.
- **After:** this change. Binaries, the exact source patch, raw samples and
  provenance are in `baml_language/target/source-errors-bench/`, not in git.
- Two workloads are new. `throw` makes 100 throws per root, each unwinding
  three frames to a catch: 204,800 raises per run. `dictionary` adds 2,000
  small functions to the program, so startup copies 2,000 more functions'
  metadata and line tables.

Execution time, median [minimum, maximum] in milliseconds. Positive is
slower.

| Workload | Mode | HEAD | This change | Change |
| --- | --- | ---: | ---: | ---: |
| Tiny calls | recording | 870.4 [868.6, 876.5] | 863.6 [843.4, 895.7] | −0.8% |
| Tiny calls | off | 752.4 [751.1, 752.4] | 740.9 [731.4, 745.8] | −1.5% |
| Dense calls | recording | 595.2 [579.9, 597.2] | 589.8 [577.9, 590.9] | −0.9% |
| Dense calls | off | 389.6 [381.6, 391.8] | 388.5 [388.2, 390.3] | −0.3% |
| Spawn | recording | 457.6 [426.4, 494.3] | 391.4 [373.6, 444.4] | −14.5% |
| Spawn | off | 494.7 [473.3, 656.8] | 477.2 [465.5, 484.9] | −3.5% |
| Repeated capture | recording | 492.2 [491.2, 507.7] | 514.8 [514.6, 546.3] | +4.6% |
| Repeated capture | off | 409.3 [402.2, 428.7] | 401.8 [395.5, 406.0] | −1.8% |
| Unique capture | recording | 294.4 [278.3, 305.7] | 281.4 [274.5, 287.8] | −4.4% |
| Unique capture | off | 31.9 [31.4, 36.4] | 31.9 [31.2, 32.0] | +0.1% |
| **Throw** | recording | 140.9 [140.6, 143.4] | 183.3 [181.6, 186.5] | **+30.1%** |
| Throw | off | 100.4 [100.0, 100.8] | 101.9 [97.4, 102.7] | +1.4% |
| Dictionary | recording | 9.0 [8.8, 9.3] | 8.5 [8.3, 8.8] | −5.5% |
| Dictionary | off | 7.7 [7.7, 7.8] | 7.5 [7.5, 7.7] | −2.5% |

Repeated capture was the only successful workload that looked slower, so it
got a follow-up with five repetitions and another order seed. That run went
the other way: 469.9 → 453.0 ms (−3.6%). Pooled over eight samples per
binary, execution is 476.9 → 465.9 ms (−2.3%) and process CPU 775.1 →
757.8 ms (−2.2%). Spawn varied a lot in both binaries; its −14.5% is not
evidence of a speedup. None of this proves zero overhead on the successful
path; it shows no consistent slowdown there.

Exceptions do cost something when a recording is on:

- **Time:** 42.4 ms more for 204,800 raises, about 0.21 µs per raise on the
  executing thread. Process CPU, including the recorder thread, went from
  186 to 307 ms: about 0.59 µs per raise. The per-root p95 went from 60.9 to
  102.0 µs (100 raises per root).
- **Bytes:** the recording grew from 0.76 MB to 15.67 MB, about 73 bytes per
  raise: the raise with a three-frame stack and its end.
- **Drain:** 1.8 → 3.8 ms at shutdown.
- With telemetry off, throws cost the same as before (+1.4%, within the
  noise of the other off controls). A run without a recording builds no
  raises at all.

Startup, memory and bytes:

- Engine startup with a recording moved within noise: dictionary 12.82 →
  12.79 ms, other workloads between −8.5% and +13.8%, and telemetry-off
  controls moved as much (−14.5% to +20.1%) with no telemetry work at all.
  Copying line tables for 2,000 extra functions was not visible in this
  harness; it is not zero, and this is a single-machine check.
- Peak RSS medians changed by −1.9% to −0.9% everywhere except `throw`
  with a recording, +1.1%.
- Recording bytes: `dictionary` +0.3% (three referenced functions now carry
  maps); tiny, dense, spawn and both capture workloads changed by under
  0.1%. CAS bytes were identical.
