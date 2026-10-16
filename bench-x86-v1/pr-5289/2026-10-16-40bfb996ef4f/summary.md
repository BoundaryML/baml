## Native bench `40bfb996ef4f` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 52.9 µs | 61.4 µs | 86.7 µs | 1.6× | 4.237 µs | 0.08× | 0.98× |
| array-empty | 23.3 µs | 30.0 µs | 56.7 µs | 2.4× | 0.007 µs | 0.00× | 0.04× |
| array-small | 25.3 µs | 93.1 µs | 131.6 µs | 5.2× | 0.101 µs | 0.00× | 0.09× |
| array-iterator | 124.2 µs | 3.76 ms | 4.47 ms | 36× | 3.127 µs | 0.03× | 0.88× |
| array-indexed | 119.3 µs | 469.6 µs | 497.9 µs | 4.2× | 6.459 µs | 0.05× | 0.10× |
| array-build | 575.0 µs | 6.09 ms | 6.99 ms | 12× | 53.3 µs | 0.09× | 0.28× |
| merge-random | 708.8 µs | 2.62 ms | 2.85 ms | 4.0× | 111.9 µs | 0.16× | 0.50× |
| merge-sorted | 573.5 µs | 2.42 ms | 2.47 ms | 4.3× | 91.9 µs | 0.16× | 0.58× |
| merge-reverse | 613.0 µs | 2.40 ms | 2.65 ms | 4.3× | 101.9 µs | 0.17× | 0.54× |
| merge-duplicates | 661.5 µs | 2.56 ms | 2.74 ms | 4.1× | 86.8 µs | 0.13× | 0.51× |
| quick-random | 840.8 µs | 2.76 ms | 2.89 ms | 3.4× | 78.0 µs | 0.09× | 0.77× |
| quick-sorted | 698.8 µs | 2.44 ms | 2.52 ms | 3.6× | 81.5 µs | 0.12× | 0.73× |
| quick-reverse | 705.7 µs | 2.43 ms | 2.49 ms | 3.5× | 80.4 µs | 0.11× | 0.68× |
| quick-duplicates | 395.4 µs | 1.32 ms | 1.44 ms | 3.6× | 46.5 µs | 0.12× | 1.0× |
| json-aggregate | 8.24 ms | 8.43 ms | 8.61 ms | 1.0× | 1.87 ms | 0.23× | 0.05× |
| allocation-low-retention | 82.24 ms | 327 ms | 351 ms | 4.3× | 18.07 ms | 0.22× | 0.19× |
| allocation-high-retention | 151 ms | 456 ms | 497 ms | 3.3× | 26.58 ms | 0.18× | 0.16× |
| calls-empty | 22.9 µs | 24.2 µs | 48.5 µs | 2.1× | 0.031 µs | 0.00× | 0.03× |
| calls | 25.4 µs | 35.2 µs | 64.9 µs | 2.6× | 0.143 µs | 0.01× | 0.04× |
| json-hello | 31.4 µs | 30.6 µs | 57.8 µs | 1.8× | 0.252 µs | 0.01× | 0.03× |
| generate-sort | 4.28 ms | 4.28 ms | 4.33 ms | 1.0× | 854.9 µs | 0.20× | 0.55× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.

Against `22ef413fd0a0`: **0 faster, 1 slower, 20 unchanged** (net of control drift, threshold 5%).
- allocation-high-retention: 84.70 ms → 151 ms (+78.5%, control +29.7%) **slower**
