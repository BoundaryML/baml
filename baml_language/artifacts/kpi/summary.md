BAML speedtest on Spark (aarch64, 20 cores).

Run from `baml_language` against backend build commit `b2696e289b`:

```sh
scripts/speedtest run --kpi --rows vm-off,vm-medium,native-off,native-medium,rt-off,rt-medium,node,bun,python --trials 3 --cpu 19 --rust-build-dir "$PWD/target/speedtest-h31qtmf1/rust-build" --results-dir "$PWD/artifacts/runs" --out "$PWD/artifacts/kpi"
```

3 trials, pinned to CPU 19, interleaved with seed 1. Sampled one-minute load: 10.97–88.14.
Started 2026-11-07T04:57:35Z; finished 2026-11-07T07:31:29Z.
732 timed trials; 0 failed trials; 0 failed cells.

Times below are median ms/job. Speedup is comparison time / rt-off time; above 1 means rt is faster.

| Workload | vm-off | native-off | rt-off | node | bun | python | RT vs VM | RT vs Node |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| classes::method call 100k | 25.9 | 6.805 | 1.387 | 0.05215 | 0.2952 | 14.28 | 18.67× | 0.03759× |
| compute::array build sum 100k | 26.4 | 3.132 | unsupported | 2.096 | 1.444 | 8.367 | — | — |
| compute::array traversal 8k | 2.819 | 0.6063 | unsupported | 0.1074 | 0.3275 | 0.8943 | — | — |
| compute::fib32 recursive | 345.2 | 67.32 | 24.32 | 20.32 | 12.06 | 264.8 | 14.19× | 0.8357× |
| compute::generate sort 8k | 4.966 | 0.9047 | unsupported | 3.009 | 4.475 | 4.758 | — | — |
| compute::json aggregate 2k | 40.67 | 23.13 | unsupported | 9.846 | 11.95 | 38.64 | — | — |
| compute::json hello | 0.001423 | 0.0006012 | unsupported | 0.0001143 | 0.0005809 | 0.0009005 | — | — |
| compute::json ndjson events | 5.476 | 4.242 | unsupported | 1.142 | 1.29 | 3.901 | — | — |
| compute::json request body | 0.12 | 0.05707 | unsupported | 0.03625 | 0.06093 | 0.1154 | — | — |
| compute::json tree path | 5.238 | 3.14 | unsupported | 0.9503 | 1.317 | 2.464 | — | — |
| compute::k-nucleotide 25k | 225.5 | 76.52 | unsupported | 14.52 | 18.12 | 41.98 | — | — |
| compute::merge sort 20k | 179.7 | 21.59 | unsupported | 12.27 | 11.42 | 171.5 | — | — |
| compute::nbody 20k | 226.5 | 57.19 | unsupported | 1.206 | 5.429 | 173.8 | — | — |
| compute::nested loops 500x500 | 10.35 | 0.1758 | 0.2079 | 0.1383 | 0.4128 | 7.789 | 49.8× | 0.6654× |
| compute::pure call 1m | 81.32 | 7.878 | 2.096 | 0.5323 | 1.746 | 42.64 | 38.8× | 0.254× |
| compute::quick sort 3-way 1k | 7.647 | 0.5222 | unsupported | 0.6972 | 0.8226 | 3.784 | — | — |
| interfaces::polymorphic dispatch 100k | 52.44 | 22.09 | 1.575 | 1.022 | 0.948 | 17.48 | 33.29× | 0.6487× |
| memory::allocation high retention | 415.5 | 91.17 | unsupported | 27.46 | 31.13 | 141.2 | — | — |
| memory::allocation low retention | 295.2 | 50.59 | unsupported | 13.8 | 16.19 | 88.22 | — | — |
| memory::binary trees 14 | 1502 | 386.1 | 36.86 | 49.36 | 27.3 | 683.7 | 40.74× | 1.339× |
| memory::cycles into live | 201.7 | 302.2 | unsupported | 69.9 | 19.69 | 53.93 | — | — |
| memory::cyclic churn | 32.34 | 8.947 | unsupported | 1.815 | 2.829 | 10.04 | — | — |
| memory::heap calls | 3.036 | 0.9726 | unsupported | 0.08163 | 0.09639 | 2.513 | — | — |
| memory::live heap | 332.4 | 348.4 | unsupported | 73.09 | 48.93 | 107.8 | — | — |
| memory::live walk | 62.85 | 26.1 | unsupported | 6.916 | 3.015 | 13.88 | — | — |
| memory::splay 2k | 302.8 | 76.36 | unsupported | 29.65 | 13.03 | 70.1 | — | — |
| process::startup | 0.01715 | 0.01491 | 0.000656 | 0.000688 | 0.000816 | 0.000544 | 26.15× | 1.049× |
| programs::agent loop 300 | 36.01 | 46.81 | 2.45 | 5.708 | 4.397 | 15.06 | 14.7× | 2.33× |
| programs::json parser 15 | 375.9 | 166.3 | unsupported | 12.08 | 19.55 | 103.8 | — | — |
| programs::richards 50 | 506.1 | 53.87 | 2.599 | 2.74 | 15.59 | 94.46 | 194.7× | 1.054× |
| string::concat loop 10k | 6.356 | 2.159 | 19.61 | 0.03922 | 0.0773 | 0.4412 | 0.3241× | 0.002× |
| string::split medium literal 10k | 9.574 | 7.338 | unsupported | 0.2488 | 0.1811 | 2.896 | — | — |

Admitted rt workloads: medium telemetry in ms/job and shared off/medium build KPIs.

| Workload | vm-medium | rt-medium | RT size (KiB) | RT pack (s) |
|---|---:|---:|---:|---:|
| classes::method call 100k | 47.05 | 3.725 | 11101.3 | 24.50 |
| compute::fib32 recursive | 1045 | 263.5 | 11017.5 | 8.32 |
| compute::nested loops 500x500 | 22.13 | 0.08671 | 10988.2 | 9.71 |
| compute::pure call 1m | 112.8 | 50.24 | 11026.3 | 8.56 |
| interfaces::polymorphic dispatch 100k | 48.37 | 63.48 | 11416.1 | 8.51 |
| memory::binary trees 14 | 1640 | 904 | 11265.2 | 15.00 |
| process::startup | 0.01838 | 0.2117 | 10961.7 | 21.40 |
| programs::agent loop 300 | 62.67 | 6.739 | 13187.0 | 48.48 |
| programs::richards 50 | 432.7 | 84.39 | 20917.8 | 227.16 |
| string::concat loop 10k | 5.888 | 19.47 | 11049.5 | 37.24 |

Release optimization is used for both compiled backends. The native dependency cache was partially warmed by an interrupted initial attempt; rt began with a cold cache. Subsequent packs share the respective backend cache. Pack times include compilation under concurrent machine load. Trials were reduced from five to three because of the projected full-run duration; no workloads or per-trial jobs were reduced.

VM/native use the BAML timing driver; rt uses its native monotonic timing driver and also checks every timed return. Unsupported compiler admissions are excluded from comparisons. Each telemetry level shares its backend executable.

Additional progress monitoring observed a one-minute load peak of 119.32 during packing. The JSON load range uses the runner’s build/verify/trial event samples.
