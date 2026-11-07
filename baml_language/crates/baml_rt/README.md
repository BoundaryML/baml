# BAML runtime integration

`Instance` owns a non-moving heap and a stackful task loop. Generated native
functions run as root tasks with scoped TLS bindings, restored on return/error.
Allocation and safepoints obtain roots through loop-owned `IntegrationRoots`.
Managed pointers must stay on scanned stacks, in traced objects or explicit root
leases; host boxes and vectors are not implicit roots. Task-switch-aware bindings
are required before exposing generated child tasks through this adapter.

The typed backend supports the three workloads: classes, nullable and
union values, finite interface dispatch, growing lists, string-keyed maps,
strings, streaming typed JSON, checked i63 arithmetic, catches and rethrows.
Unsupported constructs fail at pack time. Errors preserve source-side frames.
Generated prologues enforce stack bounds with reserve for error construction.
Fallible allocation uses a separate traced reserve for allocation-failure errors;
host allocator failures and arbitrary retention of caught OOM values are outside
that guarantee. Heap capacity survives successive jobs.

Medium telemetry is the default. Off/low skip function instrumentation; medium
records exact path/reentry counts and durations. High and promoted policies emit
spans, captures and error evidence through the existing telemetry processor.
Task boundaries and yields flush local deltas.

Select with `baml pack --rust --rt ... main`. Plain execution prints an
integer. `--jobs N --warmup N` uses the native speedtest JSON protocol and retains
an instance across jobs. `BAML_GC_STRESS=1` forces collection at stress points;
`BAML_RT_HEAP_LIMIT=<bytes>` limits ordinary usable heap capacity, not RSS.

From `baml_language`, with `CARGO_TARGET_DIR` inside your worktree:

```sh
cargo test -p baml_rt
cargo test -p baml_rt --release
BAML_RUST_DIFF_PROBES='programs::richards 50,memory::binary trees 14,programs::agent loop 300' cargo test -p baml_tests --test rust_differential -- --ignored
```

Use packed workload binaries with `--jobs`/`--warmup` for benchmarks; pin a CPU
and interleave variants. Results and runtime contracts are in the baml_rt PR
description (#5566).
