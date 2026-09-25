# What a recorded raise costs

With a recording on, a program that throws a lot ran about 25% slower on
this branch than on the BTEL stack below it (#4958). It now runs as fast as
#4958. This report says where the time went and what changed. Programs
that do not throw are not affected: the successful call, return and await
paths run the same code as before.

Measured 2026-09-25 on a Ryzen 9 5950X with `perf` (this time with
`perf_event_paranoid` lowered, so the numbers come from a profiler rather
than from switch builds).

## The workload

`throw` runs 2,048 calls of `main`. Each call throws 100 times, and each
throw unwinds `t3 → t2 → t1` into a catch in `main`. That is 204,800 raises
in about 150 ms, one every 0.7 µs. Every raise is a fresh `throw` four frames
deep. Any cost per raise shows up here many times over.

## Results

Execution time with a recording on, median of 7 pinned runs (cores 8–15):

| Clock | #4958 | Branch before | Branch now |
| --- | ---: | ---: | ---: |
| `Auto` (the production default) | 135.2 ms | 166.3 ms (+23.7%) | 133.7 ms (−1.1%) |
| `Monotonic` (the harness default) | 151.8 ms | 191.5 ms (+25.5%) | 149.7 ms (−1.4%) |

The "before" column comes from an earlier batch of the same binaries. A
second batch measured `Auto` at 134.9 ms for #4958 and 135.5 ms now
(+0.4%). Differences this small are noise.

The harness reads time with `clock_gettime`, which costs more than the TSC
read that `Auto` uses in production. `BTEL_BENCH_CLOCK=auto` now selects
`Auto`; the default stays `Monotonic` so older reports stay comparable.

The recording went from 72.5 to 48.5 bytes per raise (15.8 MB → 10.9 MB per
run). Process CPU, including the recorder thread, went from 293 to 208 ms
(#4958: 182 ms). The rest of that CPU is the recorder encoding and writing
the raises, on its own thread.

Workloads that do not throw, `Auto`, median of 5:

| Workload | #4958 | Branch now | Change |
| --- | ---: | ---: | ---: |
| tiny | 896.1 ms | 877.3 ms | −2.1% |
| dense | 566.2 ms | 576.1 ms | +1.7% |
| spawn | 389.1 ms | 383.7 ms | −1.4% |

These are within the spread of repeated runs of one binary.

## Where the time went

Before this change, each raise cost about 710 extra cycles on the thread
running BAML:

- About 150 cycles went to two heap allocations: the boxed raise and its
  frame list. The recorder thread freed them, so glibc could not reuse the
  memory from the VM thread's own cache and took its slow path.
- About 130 went to copying the stack. For every frame, the VM loaded the
  function object from the heap and checked its registration flag.
- About 120 went to two extra span writes. Each write looks up the thread's
  producer in thread-local storage.
- About 80 went to reading the clock for the raise's timestamp.
- About 60 went to finding the raise kind. The VM loaded the frame's
  function again to decode the opcode, although the unwinder already had it.

The recorder thread spent another 1,300 cycles per raise. Most of it was
prost computing each raise's encoded length about four times per file.

## What changed

The raise names its call path instead of copying the stack. The producer
already keeps the active call path for every observed call, and each call
path records its caller and the PC of the call. So the raise frame's path,
followed back to the thread's first frame, is the stack. The reader lists
those callers once per path, not once per raise.

With the stack gone, a raise fits the normal 56-byte span slot, so a fresh
raise allocates nothing. A rethrow or an await sends its origin in a small
record just before the raise. Only two rare cases still allocate: a stack
the call path cannot describe, and an inherited trace.

One clock read now serves the whole unwind. The unwinder used to read the
clock again for every frame it popped, to stamp that frame's exit. No code
runs between those pops, so while raises are recorded, the popped frames
exit at the raise's time. In this workload that is one read per raise
instead of four. Runs without a recording keep reading per frame.

The raise and its end usually share one record. The recorder needs the
raise before the retained calls the unwind completes, so it can link them.
When the unwind completes no retained call, nothing has to go between them,
so the VM writes a single `ErrorRaisedAndEnded` record at the end. If a
retained call is about to complete, the VM writes the raise first, as
before. Timing-only frames report through the separate timing stream and
never need this. Each span record costs a cache line that the recorder
thread last touched, so this matters more than the instruction count
suggests. The file format does not change: the recorder still writes a
raise and an end.

Smaller changes:

- The raise-frame marker is only sent when the raise frame's function has
  a policy. Without a policy the frame can never be promoted, so the
  marker could never name anything. In this workload that saves one record
  per raise.
- The unwinder completes popped frames with the function it already
  loaded, instead of loading it a second time.
- The raise kind is decoded from the function the unwinder already holds.
- On a catch, the handler's function ID is looked up once instead of twice.
- The recorder encodes each error message once, as it arrives, and writes
  the bytes when it seals the file. It no longer re-publishes the raise
  function, which its call path already published.

## What the stack loses

A stack from a call path is not identical to the old frame list:

- Native frames are not on call paths. A failure inside native code is
  still placed at the BAML call that entered it.
- Direct recursion shares one call path, so `f → f → f` is listed once.
- There is no cap of 64 frames in the recording. The reader still lists at
  most 64.

`error_raises.stack_depth` still counts every frame. When fewer are listed,
`stack_state` says `partial`. When some bytecode frame has no telemetry,
for example with `BAML_TELEMETRY=low`, the VM lists the frames explicitly
as before, so no mode loses stacks.

This only changes a table that was added on this branch. Nothing released
reads it.

## What is left

The thread running BAML still does some work per raise: one record write,
the landing note for the catch, and the raise's bookkeeping. It is now
paid for by the clock reads and function loads the unwinder no longer
does, at least for a stack like this benchmark's.

The recorder thread still spends CPU on every raise, and the recording is
about 48 bytes per raise larger. Most of those bytes are IDs: the raise ID
appears in both the raise and its end, and the thread ID in every raise.

## Reproduce

Binaries are built from `git archive` exports in tmpfs, with symbols and
otherwise the release profile:

```sh
cd baml_language
BAML_GIT_SHA=<commit> RUSTC_WRAPPER= CARGO_INCREMENTAL=0 \
  CARGO_PROFILE_RELEASE_STRIP=none CARGO_PROFILE_RELEASE_DEBUG=limited \
  cargo +1.98.0 build --release -p bex_engine --example btel_record_bench
```

One run, pinned, in either clock mode:

```sh
BAML_TELEMETRY=medium BTEL_BENCH_CLOCK=auto taskset -c 8-15 \
  target/release/examples/btel_record_bench btel throw 2048 <project> 0
```

Profiles use a fixed period so sample counts compare across binaries:

```sh
perf record -c 200000 -e cycles -o out.data <the command above, 40000 roots>
perf report -i out.data --sort comm,sym -n
```
