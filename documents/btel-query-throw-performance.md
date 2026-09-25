# What a recorded raise costs

With a recording on, a program that throws a lot ran about 25% slower on
this branch than on the BTEL stack below it (#4958). This report says where
that time went and what changed. Programs that do not throw are not
affected: the successful call, return and await paths run the same code as
before.

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
| `Auto` (the production default) | 134.4 ms | 166.3 ms (+23.7%) | 146.0 ms (+8.6%) |
| `Monotonic` (the harness default) | 152.6 ms | 191.5 ms (+25.5%) | 171.6 ms (+12.5%) |

The harness reads time with `clock_gettime`, which costs more than the TSC
read that `Auto` uses in production. `BTEL_BENCH_CLOCK=auto` now selects
`Auto`; the default stays `Monotonic` so older reports stay comparable.

The recording went from 72.5 to 48.5 bytes per raise (15.8 MB → 10.9 MB per
run). Process CPU, including the recorder thread, went from 293 to 223 ms
(#4958: 181 ms). With telemetry off, all three builds run the same.

Workloads that do not throw, `Monotonic`, median of 5:

| Workload | #4958 | Branch now | Change |
| --- | ---: | ---: | ---: |
| tiny | 884.9 ms | 881.2 ms | −0.4% |
| spawn | 393.0 ms | 391.0 ms | −0.5% |
| dense | 673.3 ms | 602.8 ms | −10.5% |

`dense` moves a lot between runs of the same binary. Its difference is not a
speedup.

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

Smaller changes:

- The raise-frame marker is only sent when the raise frame's function has
  a policy. Without a policy the frame can never be promoted, so the
  marker could never name anything. In this workload that saves one record
  per raise.
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

The thread running BAML still spends about 50–75 ns more per raise than on
#4958. That is one clock read, two record writes and the landing note for
the catch. Removing any of them would mean recording less: the raise's
time, its end, or the evidence that proves a rethrow's origin.

The recording is still about 48 bytes per raise larger. Most of it is IDs:
the raise ID appears in both the raise and its end, and the thread ID in
every raise.

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
