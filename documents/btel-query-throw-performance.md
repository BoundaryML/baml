# Cost of recording raises

Follow-up to the performance section of
[btel-query-source-errors.md](btel-query-source-errors.md). With a
recording on, the `throw` benchmark ran about 30% slower than on HEAD. The
question was whether that can be cut without losing any of the error
evidence.

This report measures the snapshot saved as `final-source.patch` and
`final-untracked.tar`: the feature plus the first handler-lifetime fix.
A later [origin-review correction](btel-query-error-origin-review.md)
removed unsupported conversion-origin proofs. That last correction was
tested for correctness but was not rebuilt for these measurements.
References to the "final tree" or `final-recorder` below mean the saved
benchmark snapshot, not the later working tree.

**Outcome: no verified speedup from four narrow changes.** None ran faster
than a build of the same source without it, so none was ported. The measured
tree (the error feature plus the origin lifetime fix) still costs about
0.2 µs of execution per raise:

- With the standard harness (not pinned), `throw` goes from 144.0 to 185.8
  ms: +29.0%.
- In a pinned batch, it goes from 148.0 to 186.7 ms: +26.2%.
- A normal workload (`dense`) and the telemetry-off controls had much
  smaller differences, around 1–2%; these runs do not establish their cause.

## The workload

`throw` runs 2,048 calls of `main`. Each call makes 100 throws, and each
throw unwinds `t3 → t2 → t1` into a catch in `main`. That is 204,800 raises
per run. Every raise has the same four-frame stack, and every one is a
fresh `throw`.

It does not run a rethrow, a failed await or an `UnknownError` conversion.
So it never times the landing-note lookups that the origin lifetime fix
changed. The fix's cost on those paths is **not measured here**.

## Final numbers

The standard harness is `tools/btel_record_smoke.py`: unpinned, one warmup
and three runs per binary and mode, in shuffled order. It compared HEAD's
saved binary with the final tree:

| Workload | Mode | HEAD | Final | Change |
| --- | --- | ---: | ---: | ---: |
| throw | recording | 144.0 [142.8, 149.6] | 185.8 [185.5, 195.1] | +29.0% |
| throw | off | 99.5 [96.1, 102.5] | 101.7 [100.6, 102.1] | +2.2% |
| dense | recording | 581.3 [580.0, 584.2] | 586.3 [580.5, 591.9] | +0.9% |
| dense | off | 386.9 [386.5, 387.8] | 394.1 [389.0, 394.3] | +1.9% |

Execution in ms: median [minimum, maximum]. Raw data:
`final-vs-head.json`.

What each raise costs in `throw` with a recording:

- **Execution:** +41.8 ms, 0.20 µs per raise, on the thread running BAML.
  The per-call p95 went from 62 to 132 µs (100 raises per call).
- **CPU:** the process used 191.6 → 311.1 ms, about 0.58 µs per raise. This
  includes the recorder thread's work; CPU was not measured separately by
  thread.
- **Drain at shutdown:** 1.84 → 3.86 ms.
- **Bytes:** the recording grew from 0.76 to 15.67 MB, 73 bytes per raise.
- **Telemetry off:** no raise evidence is built. The +2.2% sits inside what
  the off controls do between builds. In the pinned batches the off controls
  measured 101.2/101.6 ms for HEAD and the candidate, and 103.4 ms for the
  measured build. Code layout and run variability are possible explanations;
  these measurements do not isolate a cause.

The pinned batch (`v3-pinned.json`, CPU cores 8–15, five runs) also shows
what the origin lifetime fix did to this workload:

| Binary | throw, recording | throw, off |
| --- | ---: | ---: |
| HEAD | 148.0 | |
| Error feature (saved candidate) | 189.1 | |
| Final: the feature plus the origin fix | 186.7 | 103.4 |

The fix did not slow fresh throws. As noted above, fresh throws never reach
the lookups it changed.

## Where the time goes

`perf` cannot run on this machine (`perf_event_paranoid` is 4 and there is
no sudo). So I built the candidate once more with switches read from
`BTEL_ABLATE`. Each switch skips one part of the raise path, and all runs
use the same binary. These runs were pinned to cores 8–15 (one CCD),
which cut the spread from about ±20 ms to about ±3 ms.

`ablate2-pinned.json`, seven runs each:

| Variant | ms | What it shows |
| --- | ---: | --- |
| HEAD | 149.4 | |
| All evidence (switches off) | 193.9 | Same as the saved candidate (191.2). |
| Stop right after "is a recording on?" | 150.7 | No large cost attributable to the check in this batch. |
| Build everything, emit nothing, free it on the VM thread | 174.2 | Building costs 23.5 ms (0.115 µs per raise). |
| Emit everything except the frame list | 189.2 | Removing the frame list saves about 5 ms in this experiment. |
| Emit everything, fixed timestamp | 189.0 | Removing clock reads saves about 5 ms in this experiment. |
| The recorder ignores error records | 205.0 | Skipping recorder work did not improve execution. |

No producer ever waited for a free chunk in these runs: the counters in
`lab1-ablate.patch` report zero exhausted waits. So the recorder keeps up.
Its extra CPU does not reach the BAML thread through backpressure. I don't
know why skipping its work made execution slower.

The remaining 19.7 ms difference involves emitting records and handing the
evidence to the recorder thread. These components interact, so subtraction
does not give independent costs. A second switch build
(`ablate3-cycles.json`) investigates that difference within one binary:

| Variant | ms |
| --- | ---: |
| All evidence | 201.6 |
| Evidence built and freed on the VM thread; a one-field record emitted in its place | 190.3 |
| Nothing | 148.9 |

That suggests cross-thread ownership of the boxed raise and its frame list
contributes to the 11.3 ms difference (55 ns per raise). The experiment also
changes the emitted record and recorder work, so it does not isolate the
allocator alone. The rdtsc counters show, per raise:

| Section | All evidence | Freed on the VM thread |
| --- | ---: | ---: |
| Capture the frame list | 207 cycles | 82 cycles |
| Box the raise and emit it | 162 cycles | 103 cycles |
| Whole raise start | 580 cycles | 382 cycles |

Each probe adds about 33 cycles. The differences suggest that cross-thread
freeing affects allocation and cache behavior. A plausible explanation is
glibc's per-thread allocation caches, but these probes do not establish the
exact allocator path or cache misses.

The rest is spread thin: a clock read, a walk and registration of four
frames, two small copies, and the landing note on catch. No single piece is
large.

## What I tried

Every candidate intends to preserve the same evidence and wire format,
with roughly the same file size (15.67 MB), span slot and hot-path layouts.
Recordings themselves differ in IDs and timestamps. Each candidate
was built exactly like the matched control. For the first three the control
is `lab-baseline`, the unmodified candidate built in the same directory
(189.3 ms). The saved candidate measured 187.2 ms in the same batch, so lab
builds are comparable with it.

| Candidate | Change | ms | vs. control | Kept |
| --- | --- | ---: | ---: | --- |
| `v1-inline8` | Up to 8 frames stored inside the boxed raise (`SmallVec`); the raise's function read back from its captured frame. Box: 128 → 312 bytes. | 197.7 | +8.4 | no |
| `v1b-inline4-inplace` | Same with 4 frames (the harness stack depth), filled straight into the box. | 194.8 | +5.5 | no |
| `v2-compact` | 16-byte frames (a native frame is one with no PC), the rarely present inherited trace boxed separately, and the same read-back. Both allocations become glibc fastbin sizes. | 199.8 | +10.5 | no |
| `v3-arc-last-frames` | One cached copy of the last stack on the error book; an equal stack reuses it through `Arc`. Built on the final tree. Control: `final-recorder`, 186.7 ms. | 190.1 | +3.4 | no |

Sources: `lab-baseline-pinned.json` and `v3-pinned.json`. The first three
were also slower than the saved candidate in `v1-pinned.json`,
`v2-pinned.json` and the root agent's `root-v1b-pinned.json`. With
telemetry off, `v2-compact` and `v1b` stayed within 2.4 ms of the candidate,
far less than their gaps with a recording on. So the slowdown is in the raise
path, not a shift of the whole VM. `v1-inline8` has no off control.

Why none was kept:

- **Inline frames (v1, v1b)** remove one allocation, but the box gets bigger
  and every raise copies it. The larger box also crosses to the recorder
  thread. Both sizes were slower.
- **Compact frames (v2)** halve the frame list and shrink the box. They were
  the slowest of all. The cause is unproven; allocator size classes are one
  possible explanation.
- **v1, v1b and v2 also share one small change:** the raise's function is
  read back from its captured frame instead of being registered twice. I did
  not build that change alone, so it is untested by itself. It saves one
  atomic flag check and a pointer lookup.
- **Arc reuse (v3)** removes the frame-list allocation, but now both threads
  touch the same reference count for every raise. It was 3.4 ms slower than
  the measured build. One same-source control comparison differed by about
  2 ms between build directories; that is not an established correction
  factor. There is no demonstrated win.

## What is left

One direction worth investigating is avoiding transfer of heap allocations
to the recorder thread. The same-binary stand-in saved 11.3 ms, while also
changing the records consumed. One implementation would send the raise and its
frames would travel as fixed-size span records (a header, then frames packed
several per record), and the recorder would put the raise back together per
thread. That changes the in-memory record protocol and needs new recorder
state for a raise split across records. It would still send frame records,
so the stand-in's result does not predict its actual speedup. This was not
implemented or measured.

The 73 bytes per raise could also shrink without losing anything. Packed
frame arrays, or a per-file stack table for repeated stacks, would do it.
Both change the wire format and the reader, so I left them alone.

## Limits

- No profiler. The attribution comes from switch builds and rdtsc probes.
  Differences of 3 ms or less between two builds are within build and run
  noise.
- One machine (Ryzen 9 5950X) and one allocator. The bench binary uses glibc
  malloc. `baml-cli` uses mimalloc, and that was not measured. Under
  jemalloc (`LD_PRELOAD`, unpinned, `ablate1.json`) the gap was the same:
  144.2 → 185.2 ms.
- Diagnosis batches were pinned to cores 8–15. The final HEAD comparison
  uses the unpinned standard harness, like the earlier report.
- Only fresh throws with a four-frame stack. Deep stacks, native frames,
  rethrows, awaits and conversions were not timed.
- Total timed measurement: 208.7 s across nine short batches, including the
  root agent's 11.0 s batch. Builds were separate: 165–295 s each.

## Reproduce

Everything is under `baml_language/target/throw-perf/` (not in git).
`provenance.json` lists each binary's source, sha256, build time and batch.

```sh
cd baml_language
B=target/source-errors-bench; T=target/throw-perf
# Final comparison (standard harness):
flock /tmp/btel-source-errors-validation.lock python3 tools/btel_record_smoke.py \
  --before $B/head-recorder --after $T/final-recorder \
  --output $T/<new>.json --workloads throw dense --repeats 3 --seed 7
# Pinned comparison of any binaries (name=path=BTEL_ABLATE; an "off" prefix
# runs with telemetry off):
flock /tmp/btel-source-errors-validation.lock env PIN=8-15 python3 $T/ablate.py \
  $T/<new>.json throw 2048 5 head=$B/head-recorder=0 final=$T/final-recorder=0 \
  v3=$T/v3-arc-last-frames=0 off-final=$T/final-recorder=0
```

To rebuild a lab variant, start from a fresh export of `baml_language` at
HEAD `2e8be7ca16760b821836a249a72826520a6156f6`. Apply
`../source-errors-bench/candidate.patch` (from the repository root) and
extract `candidate-untracked.tar` inside `baml_language`. For `v3`, use
`final-source.patch` and `final-untracked.tar` instead. Then apply the
variant's patch inside `baml_language` and build:

```sh
RUSTC_WRAPPER='' RUSTC_WORKSPACE_WRAPPER='' CARGO_INCREMENTAL=0 \
  cargo +1.98.0 build --release -j 4 -p bex_engine --example btel_record_bench
```

`lab1-ablate.patch` and `lab2-cycles.patch` were regenerated by re-running
the same edit scripts on the candidate source. The binaries that produced
the data are saved next to them.

Artifacts in `target/throw-perf/`:

- Binaries: `final-recorder`, `lab-baseline`, `lab1-ablate`, `lab2-cycles`,
  `v1-inline8`, `v1b-inline4-inplace`, `v2-compact`, `v3-arc-last-frames`.
- Patches: the six above, plus `final-source.patch` and
  `final-untracked.tar` for the final tree.
- Raw samples: `ablate1.json`, `ablate2-pinned.json`, `ablate3-cycles.json`,
  `v1-pinned.json`, `root-v1b-pinned.json`, `v2-pinned.json`,
  `lab-baseline-pinned.json`, `v3-pinned.json`, `final-vs-head.json`.
- Runner: `ablate.py`, plus `busy.sh` (the idle check).
- Baselines used and left untouched: `target/source-errors-bench/`.
