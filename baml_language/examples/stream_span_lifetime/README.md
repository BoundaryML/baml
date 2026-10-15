# Streaming span lifetime

This fixture uses a scripted client, with no provider requests. The exact same
source runs on nightly and the modified local CLI:

```sh
cargo build -p baml_cli
python3 examples/stream_span_lifetime/compare.py
```

The script uses installed nightly `0.22.3-nightly.20261013.b`, local-only
recording, and the same telemetry setting for both (`medium` by default;
use `--telemetry high` to include additional execution detail). It creates separate
temporary projects and saves both BTEL recordings and JSON query results.
[observed.json](observed.json) contains the October 15 comparison at `high`.
Override the installed version with `--nightly VERSION`.

The three scenarios never call `final()`. Nightly records no retained `Extract`
operation spans; at `high`, its spec/stream companion rows remain visible. The
local build replaces those wrappers with:

| Input | Status | Captured result |
| --- | --- | --- |
| `success` | `return` | `Answer { value: 42 }` |
| `parse failure` | `user_error` | `baml.errors.Context` containing `ai.errors.ParseFailed` |
| `abandoned` | `closed` | No output or error |

These are logical operations associated with the existing `Extract` definition.
Their lifetime starts before opening the provider stream and ends after final
parsing, a failure, or closure. They use ordinary function rows and contribute
successful/failed executions to the function's profiler population. Neutral
closure is excluded from completed-request latency and population aggregates.
`run()` temporarily switches tracing ancestry and context on
the current execution. It keeps that execution's thread ID and clock and emits
no synthetic future records. Actual spawned tasks retain their own futures.
Generated companions and designated stdlib runners are pass-through functions:
they produce no wrapper spans, including at high telemetry. They forward the
effective `$trace` settings and any reservation already claimed at entry to
the operation creator, which uses that identity without claiming it again.

## Manual span API

```baml
let span = trace.start_span(
    Extract,
    inputs = { "input": "some text" },
    metadata = { "component": "streaming" },
);
let result = span.run(() -> { 42 });
span.finish(result);
```

`target` accepts a nonempty name or an existing BAML function value. Association
records its identity and source metadata; it neither invokes the function nor
validates the supplied values against its signature. Inputs are an optional
`map<string, unknown>`, snapshotted at construction. There is no input setter.
For a separately declared map, use that explicit value type.

- `finish(value)` records an output and ends successfully.
- `fail(error)` records an error and ends unsuccessfully; it does not throw.
- `fail_with_context(error, context)` preserves an existing `baml.errors.Context`
  stack/cause while recording the supplied error. It does not invent a traceback.
- `close()` ends without assigning an outcome.
- The first terminal operation wins. Later terminal operations do nothing.
- `id()` returns the span identity, or null with tracing disabled.
- `is_closed()` works with tracing enabled or disabled.
- `run(body)` activates ancestry and context for its body, including suspended
  work, and restores the caller's context on return or throw. It does not finish
  the span. Using a closed span throws `trace.SpanError`.
- `parent = another_span` selects an explicit open parent in the same runtime.
  Otherwise creation uses the current tracing ancestry. Creating a span does
  not change the caller's active context.

Last owning Rust drop closes an unfinished span. `Span` has no separate BAML
finalizer that could run before its owning stream releases resources. It queues the completion for a
safe producer scope, since GC can drop Rust data while heap locks are held.
Any remaining closures drain on the existing consumer before shutdown seals;
shutdown does not reserve another producer slot.
Runtime shutdown also closes surviving operations before sealing the recording;
its registry contains weak references and does not keep spans alive. This
fallback does not infer cancellation.

## Stream behavior

The generated spec carries the original function reference and bound arguments.
The streaming companion forwards the held configuration through `$trace`.
The returned `Stream<T>` owns the span. Construction, pulls, and final parsing run
under its context. EOF performs final parsing once: `next()` still returns
`Done` on a final parse failure, while `final()` rethrows the cached error.
Repeated `final()` returns the cached result without reactivating a closed span.
Cleanup releases the underlying stream and then closes any unfinished span.

This change is in the BAML stdlib/runtime. The Python adapter's preexisting
retained host-invocation layer remains an additional SDK boundary; this example
compares direct BAML execution, not SDK adapter tracing.

## BTEL and backend rollout

BTEL minor 12 uses ordinary function announcements/completions for owned spans,
with named input maps, type arguments, optional bound receiver captures, and
general neutral closure. Local `baml query` exposes these as ordinary function
rows with status `closed` when appropriate. The input map remains exactly as
supplied; the bound receiver has its own `receiver` column. The local index
schema is version 19, so older indexes rebuild. Legacy manual records remain
readable.

The BCS publisher rejects owned-span records before encoding or retaining their
captures. Backend readers must support the new semantics before remote emission
is enabled. Neutral closure must never be coerced to success or cancellation.

## Known limitations

Profiler self time still subtracts synchronous child totals. An owned operation
can outlive its creator or overlap other work, so that calculation can undercount
the creator's self time (clamping it to zero), double-subtract overlapping children,
or attribute unused stream lifetime to operation self time. Ownership ancestry and
recorded lifetimes are retained; separating execution accounting from ownership
is deferred.

Operation creation follows effective client/callback resolution. A failure before
creation has no operation row; it propagates to the caller. Declaration hooks run
on direct AI calls (including function pointers), not spec/Agent/stream execution.
Use the target's function policies for recording rules that must apply across paths.
