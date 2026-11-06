# Allocation profile, Spark/aarch64, CPU 12

Baseline: `adc6de86ff`, with only the example's shape/trial command-line selector
added for repeated sampling. Both profile binaries use release opt-level 3, fat
LTO and one codegen unit, with debug lines retained and stripping disabled.
The final timing table uses the normal release build (symbols stripped), the
original one warmup / one tree job or two low-retention jobs, median of five.
The workload, checksums, root protocol and object sizes are unchanged.

## Findings

`perf record` confirms allocation was already inlined. Before optimization,
low retention spent roughly 64% of sampled cycles in `bench::low` and trees
66% in `bench::tree`. The low loop's annotated instructions include repeated
descriptor/size validation, vector-index multiplication and bounds checks,
per-object zero stores, start-bit read/modify/write, a saturating spending
update, and another descriptor lookup for finalization. The `RootSource` argument
is materialized on the cold call branch; it adds no root visit or virtual dispatch
to the fast path. RootSource removal would not address these measured costs.

Collection count alone understated allocation cost: the timed low-retention
jobs grow beyond the warmup's backing capacity. A diagnostic 20-job trial of the
partially optimized allocator was about 2.0 ms/job, versus 5.5 for the original
two-job timing window. This was **diagnostic only**, not substituted for the
requested benchmark. First-touch faults and growing/copying metadata were the
remaining bottleneck. Whole-process `perf stat` measurements make that visible:

| Counter, five low-retention trials including warmup/setup/teardown | Before | After |
|---|---:|---:|
| Minor faults | 62,920 | 5,910 |
| User time | 33.953 ms | 26.313 ms |
| System time | 79.889 ms | 13.156 ms |

These counters cover more than the timed jobs, so they must not be subtracted
from ms/job. Linux's policy was `madvise`; a separate `/proc/<pid>/smaps` probe
observed 34 MiB of `AnonHugePages` in the optimized process. The hint actually
obtained huge pages on this run, not just a successful build. The process-wide
probe and fault counters are retained alongside the profile reports.

Approximate shares of sampled cycles (not percentages of wall-clock ms/job):

| Shape / symbol | Before | After |
|---|---:|---:|
| Low: allocation + workload loop (`bench::low`) | 63.92% | 72.23% |
| Low: free-run search (`Block::run`) | 5.81% | 3.15% |
| Low: object lookup (`Heap::locate`) | 3.40% | 2.39% |
| Low: bulk libc memset | below 1% | 13.41% |
| Trees: allocation + recursion (`bench::tree`) | 66.24% | 63.15% |
| Trees: checksum traversal (`bench::check`) | 10.32% | 12.87% |
| Trees: object lookup (`Heap::locate`) | 12.60% | 5.29% |
| Trees: free-run search (`Block::run`) | 3.55% | 2.44% |
| Trees: bulk libc memset | below 1% | 7.37% |

The post-change loop/zeroing percentages are larger shares of a smaller total;
per-object zeroing was previously inline, so libc memset alone is not a before/
after measure of total zeroing. Recorded cycle estimates fall from 1.981B to
1.288B for low retention and 9.709B to 7.577B for trees over equal trial counts.
The kernel-address samples reported by this PMU remain unresolved (14.41% of the
baseline low profile, 2.44% after); they are not assigned invented function names.
There were no lost samples. Low profiles had 1,723 / 671 samples; tree profiles
3,979 / 2,784. Treat small percentage differences as sampling noise.

## Changes and measurements

- Bulk zeroing alone did not close the gap (about 5.87 ms low). It replaces
  per-object zero stores with contiguous run initialization and preserves exact
  zeroing, including alignment padding.
- Cache validated allocation layout and active metadata; retain an immediate
  start-bit store for every object. No batched/deferred object-start publication.
- Derive exact spending from cursor deltas, settle on slow paths, and cache the
  budget boundary. No altered budget, early collection or additional headroom.
- Obtain 32 KiB logical blocks from 2 MiB aligned backing batches and advise huge
  pages before first touch. Normal-page fallback is correct; single-block
  allocation is attempted if reserving a batch fails. This can increase memory
  residency for tiny heaps and leaves up to almost 2 MiB unused batch capacity.
- Keep metadata in stable boxes. Growing the previous `Vec<Block>` copied large
  metadata records and incurred additional faults. With huge-page backing,
  changing metadata storage reduced low retention from about 3.27 to 2.35 ms.
- Resume line search after its last consumed run. Use an alignment-aware hasher
  for allocator-owned block-address keys and skip null edges before lookup.
  These bring the final run to **2.015 ms low / 26.979 ms trees**.

These are implementation changes within the existing contracts: objects never
move; conservative candidate validation, start/mark bitmaps, lazy sweep, line
reuse and live-byte budget policy remain. Regression tests cover exact spending
at/beyond the threshold, changing descriptors within a run, large allocations
interleaved with small ones, and zeroing reclaimed runs next to live partial
lines. Existing finalizer, weak-table and optimized fiber tests also pass.

## Reproduction

From `baml_language`, with `perf` on PATH (this run used the installed
`/nix/store/i0cd89mpcvd3jb6slpzry4vr23gz4j9k-perf-linux-7.1/bin/perf`):

```sh
export CARGO_TARGET_DIR="$PWD/crates/bex_v2_heap/target/build"
CARGO_PROFILE_RELEASE_DEBUG=1 CARGO_PROFILE_RELEASE_STRIP=none \
  cargo build -p bex_v2_heap --example bench --release
cp "$CARGO_TARGET_DIR/release/examples/bench" crates/bex_v2_heap/target/bench-after

taskset -c 12 perf record -e cycles:u -F 999 \
  -o crates/bex_v2_heap/target/after-low.data -- \
  crates/bex_v2_heap/target/bench-after low 100
taskset -c 12 perf record -e cycles:u -F 999 \
  -o crates/bex_v2_heap/target/after-trees.data -- \
  crates/bex_v2_heap/target/bench-after trees 50
perf report -i crates/bex_v2_heap/target/after-low.data \
  --stdio --no-children --percent-limit 1
perf annotate -i crates/bex_v2_heap/target/after-low.data \
  --stdio --symbol bench::low
perf stat -e page-faults,minor-faults \
  taskset -c 12 crates/bex_v2_heap/target/bench-after low 5
```

Baseline used those commands with `bench-before`, built before allocator edits.
Both binaries, raw perf data and annotated disassembly remain in the ignored
crate `target/` directory; the small text reports are committed. The CLI is
`bench [trees|low] [trials=5] [jobs=shape-default]`; no arguments preserves the
original benchmark. The optional jobs override exists for diagnosis, not the
reported acceptance run.

The first final timing run contained large outliers, so a serial confirmation
was run and retained in `../bench-results.txt`. Both its predecessor and the
baseline rerun are retained here. No best-of-five selection or replacement of
individual trials was performed. The final median meets low <=2.3 and trees
<=28, but low <=1.5 remains unmet. Timing and huge-page availability on other
hosts are not established by these measurements.
