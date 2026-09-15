# Automatic minor GC A/B

This experiment compares canary commit `58db9d04208ebab6b28e4fb32ef04216aa129d82` (automatic major GC only) with this change (adaptive automatic minor GC). Both binaries include the same feature-gated collection instrumentation and concurrent benchmark harness. Each pair ran in fresh adjacent processes in randomized order on an Apple M3 Max running macOS 26.6.2 with Rust 1.98.0. Results are medians of paired baseline/candidate ratios; ranges are the minimum and maximum paired ratios.

The candidate requests one minor collection halfway through each full-GC allocation interval. Minor collection never erases cumulative full-GC debt. If at least half of the reserved young slots survive a minor collection, the policy skips minor collection for four full-GC intervals before probing again. It also suppresses minor collection for an interval whenever multiple BEX units of work are live because the concurrent experiment showed that additional safepoint coordination outweighed young-generation savings. The allocation fast path remains one contended counter update plus one adjacent threshold load; cold policy state is boxed so `BexHeap` retains canary's hot footprint.

## Final single-work result

There were 70 successful runs: 5 repetitions of 7 cases for each binary. The baseline binary SHA-256 was `c8379ce625e9b13d1161f05609db236dd25b6efa24cce84835effa93155042c4`; the candidate binary SHA-256 was `86b1324cd5ea73caa20f7e992cd28fd4add5c004d60e64867ba234fbadbfb754`.

| Case | Wall speedup | CPU speedup | Peak RSS MiB (canary → candidate) | Collections (minor/major) | Longest pause ms |
|---|---:|---:|---:|---:|---:|
| cache | 0.98× (0.95–0.99) | 0.98× (0.95–0.99) | 417.1 → 374.1 | 0/3 → 4/3 | 54.8 → 50.5 |
| churn | 1.01× (0.98–1.02) | 1.01× (0.98–1.02) | 319.0 → 306.6 | 0/8 → 8/8 | 6.3 → 3.5 |
| continuous 100k | 0.99× (0.97–1.04) | 0.99× (0.97–1.02) | 318.2 → 307.8 | 0/6 → 6/6 | 2.5 → 1.5 |
| large live | 0.98× (0.94–0.99) | 0.98× (0.95–0.98) | 617.6 → 618.2 | 0/2 → 1/2 | 394.2 → 396.6 |
| long call | 0.98× (0.97–1.01) | 0.98× (0.96–1.01) | 310.3 → 307.0 | 0/8 → 8/8 | 6.5 → 3.6 |
| payload | 0.94× (0.92–0.99) | 0.93× (0.92–0.99) | 403.4 → 402.1 | 0/0 → 0/0 | unavailable |
| retained | 0.96× (0.94–0.97) | 0.96× (0.94–0.98) | 464.6 → 461.3 | 0/2 → 1/2 | 120.1 → 116.8 |

The useful result is not a general CPU win. Churn is throughput-neutral while sampled peak RSS falls by 12 MiB and the longest pause falls by 44%. Continuous and long-call cases trade roughly 1–2% throughput for smaller sampled RSS and pauses. Cache is about 2% slower while sampled peak RSS falls by 43 MiB. Retained work is about 4% slower and large-live about 2% slower even though the high-survival probe is immediately backed off. The payload case performs no collection and lasts only about 20–30 ms, so its timing ratio is noise rather than a GC result.

## Final concurrent result

There were 60 successful runs: 10 repetitions of 3 eight-caller cases for each binary. The concurrency gate suppressed all candidate minors, providing a control for policy overhead while retaining canary's collection behavior.

| Case | Wall speedup | CPU speedup | Peak RSS MiB (canary → candidate) | Collections (minor/major) | Longest pause ms |
|---|---:|---:|---:|---:|---:|
| concurrent cache | 1.06× (0.98–1.14) | 0.98× (0.86–1.18) | 406.3 → 410.5 | 0/2 → 0/2 | 53.0 → 55.6 |
| concurrent churn | 1.05× (0.98–1.12) | 0.99× (0.92–1.21) | 311.2 → 313.0 | 0/5 → 0/5 | 7.1 → 7.2 |
| concurrent retained | 1.01× (0.96–1.04) | 1.00× (0.94–1.11) | 457.6 → 459.7 | 0/2 → 0/2 | 117.5 → 116.4 |

The wide ranges show scheduler and machine noise, but the medians and collection phase totals show no systematic policy overhead after the hot/cold split. The equivalent A/A control also had wide ranges: 0.90–1.15 wall for cache, 0.88–1.05 for churn, and 0.83–1.05 for retained.

## Rejected policy variants

An unconditional 8 MiB minor budget was unacceptable: large-live CPU throughput fell to 0.31× and retained throughput to 0.45× because the collector repeatedly copied high-survival young generations. Adding a four-full-cycle high-survival backoff removed most of that failure. An unconditional half-interval policy remained poor with eight callers: concurrent churn measured 0.88× wall and 0.74× CPU with 6 minors and 5 majors, while retained measured 0.76× wall and 0.72× CPU with 1 minor and 2 majors. Those results motivated the concurrency gate.

An early version also used two shared allocation counters and made even a zero-minor concurrent cache control slower. Replacing that with cumulative full debt plus a minor checkpoint was necessary but insufficient; keeping all new policy fields inline still perturbed the hot heap layout. The final boxed cold state restored no-minor concurrent behavior to the A/A noise envelope.

## What this does and does not measure

The trigger is reserved BEX object-slot debt, including unused TLAB capacity. It does not count string/container/media backing allocations or SDK/native allocations. The reported pause is the exclusive heap-permit interval; waiting for mutators to park is measured separately and post-GC finalizers/callbacks occur afterward. RSS includes compiler state, native allocations, allocator retention, and all other process memory. Single-work RSS is sampled every 32 calls and concurrent RSS every 5 ms, so brief peaks can still be missed. These are synthetic engine workloads, not end-to-end production SDK workloads, so production rollout should retain GC telemetry and compare representative application traces before changing the policy further.

The experiment can be reproduced with `build.py`, paired `run.py --policy current`, and `summarize.py` in this directory. Build manifests hash the source tree and immutable test binary; the runner records every result and profiled collection in JSON.
