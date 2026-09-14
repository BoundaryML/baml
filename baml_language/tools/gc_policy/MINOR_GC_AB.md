# Automatic minor GC A/B

This experiment compares canary commit `58db9d04208ebab6b28e4fb32ef04216aa129d82` (automatic major GC only) with this change (adaptive automatic minor GC). Both binaries include the same feature-gated collection instrumentation and concurrent benchmark harness. Each pair ran in fresh adjacent processes in randomized order on an Apple M3 Max running macOS 26.6.2 with Rust 1.98.0. Results are medians of paired baseline/candidate ratios; ranges are the minimum and maximum paired ratios.

The candidate requests one minor collection halfway through each full-GC allocation interval. Minor collection never erases cumulative full-GC debt. If at least half of the reserved young slots survive a minor collection, the policy skips minor collection for four full-GC intervals before probing again. It also suppresses minor collection for an interval whenever multiple BEX units of work are live because the concurrent experiment showed that additional safepoint coordination outweighed young-generation savings. The allocation fast path remains one contended counter update plus one adjacent threshold load; cold policy state is boxed so `BexHeap` retains canary's hot footprint.

## Final single-work result

There were 70 successful runs: 5 repetitions of 7 cases for each binary. The baseline binary SHA-256 was `225ca498daa48a02ef9b1ff2696ce310615ef938b04c6167b91f271167bf54a0`; the candidate binary SHA-256 was `0c43a0802d91ba1f8bd654b4669b2db6fcdcc86fb577023e9836863b14271d8a`.

| Case | Wall speedup | CPU speedup | Peak RSS MiB (canary → candidate) | Collections (minor/major) | Longest pause ms |
|---|---:|---:|---:|---:|---:|
| cache | 0.99× (0.97–1.01) | 0.99× (0.97–1.01) | 420.7 → 372.4 | 0/3 → 4/3 | 54.1 → 49.0 |
| churn | 1.00× (0.96–1.04) | 1.00× (0.97–1.03) | 317.5 → 309.1 | 0/8 → 8/8 | 6.7 → 3.7 |
| continuous 100k | 1.02× (0.92–1.03) | 1.01× (0.93–1.02) | 325.5 → 308.3 | 0/6 → 6/6 | 2.4 → 1.5 |
| large live | 0.99× (0.97–1.04) | 0.99× (0.97–1.02) | 621.8 → 619.9 | 0/2 → 1/2 | 358.3 → 346.8 |
| long call | 0.99× (0.96–1.00) | 0.99× (0.96–1.00) | 312.4 → 308.2 | 0/8 → 8/8 | 6.6 → 3.4 |
| payload | 0.94× (0.77–0.99) | 0.94× (0.78–0.99) | 398.3 → 403.9 | 0/0 → 0/0 | unavailable |
| retained | 0.98× (0.96–0.98) | 0.97× (0.96–0.98) | 463.7 → 467.0 | 0/2 → 1/2 | 116.5 → 116.9 |

The useful result is not a general CPU win. Short-lived slot-heavy cases are throughput-neutral within run-to-run variation while sampled peak RSS falls by 4–48 MiB and the longest pause falls by 9–49%. The retained case is about 2–3% slower and 3 MiB larger even though the high-survival probe is immediately backed off. The payload case performs no collection and lasts only about 20–30 ms, so its timing ratio is noise rather than a GC result.

## Final concurrent result

There were 60 successful runs: 10 repetitions of 3 eight-caller cases for each binary. The concurrency gate suppressed all candidate minors, providing a control for policy overhead while retaining canary's collection behavior.

| Case | Wall speedup | CPU speedup | Peak RSS MiB (canary → candidate) | Collections (minor/major) | Longest pause ms |
|---|---:|---:|---:|---:|---:|
| concurrent cache | 1.03× (0.94–1.09) | 0.98× (0.88–1.16) | 416.7 → 424.9 | 0/2 → 0/2 | 53.9 → 55.7 |
| concurrent churn | 1.08× (1.00–1.36) | 0.98× (0.88–1.25) | 317.3 → 317.0 | 0/5 → 0/5 | 6.6 → 6.7 |
| concurrent retained | 1.02× (0.87–1.36) | 1.01× (0.83–1.38) | 465.9 → 469.2 | 0/2 → 0/2 | 120.2 → 118.8 |

The wide ranges show scheduler and machine noise, but the medians and collection phase totals show no systematic policy overhead after the hot/cold split. The equivalent A/A control also had wide ranges: 0.90–1.15 wall for cache, 0.88–1.05 for churn, and 0.83–1.05 for retained.

## Rejected policy variants

An unconditional 8 MiB minor budget was unacceptable: large-live CPU throughput fell to 0.31× and retained throughput to 0.45× because the collector repeatedly copied high-survival young generations. Adding a four-full-cycle high-survival backoff removed most of that failure. An unconditional half-interval policy remained poor with eight callers: concurrent churn measured 0.88× wall and 0.74× CPU with 6 minors and 5 majors, while retained measured 0.76× wall and 0.72× CPU with 1 minor and 2 majors. Those results motivated the concurrency gate.

An early version also used two shared allocation counters and made even a zero-minor concurrent cache control slower. Replacing that with cumulative full debt plus a minor checkpoint was necessary but insufficient; keeping all new policy fields inline still perturbed the hot heap layout. The final boxed cold state restored no-minor concurrent behavior to the A/A noise envelope.

## What this does and does not measure

The trigger is reserved BEX object-slot debt, including unused TLAB capacity. It does not count string/container/media backing allocations or SDK/native allocations. The reported pause is the exclusive heap-permit interval; waiting for mutators to park is measured separately and post-GC finalizers/callbacks occur afterward. RSS includes compiler state, native allocations, allocator retention, and all other process memory, and is sampled rather than continuously observed. These are synthetic engine workloads, not end-to-end production SDK workloads, so production rollout should retain GC telemetry and compare representative application traces before changing the policy further.

The experiment can be reproduced with `build.py`, paired `run.py --policy current`, and `summarize.py` in this directory. Build manifests hash the source tree and immutable test binary; the runner records every result and profiled collection in JSON.
