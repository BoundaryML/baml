## Native bench `95198878542b` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 56.5 µs | 62.9 µs | 91.3 µs | 1.6× | 4.324 µs | 0.08× | 1.1× |
| array-empty | 22.4 µs | 30.9 µs | 57.0 µs | 2.5× | 0.007 µs | 0.00× | 0.04× |
| array-small | 24.5 µs | 89.1 µs | 126.3 µs | 5.2× | 0.111 µs | 0.00× | 0.08× |
| array-iterator | 118.4 µs | 3.75 ms | 4.44 ms | 37× | 2.932 µs | 0.02× | 0.91× |
| array-indexed | 112.4 µs | 432.6 µs | 462.6 µs | 4.1× | 5.404 µs | 0.05× | 0.11× |
| array-build | 557.3 µs | 5.92 ms | 6.67 ms | 12× | 45.7 µs | 0.08× | 0.28× |
| merge-random | 726.9 µs | 2.58 ms | 2.72 ms | 3.7× | 110.8 µs | 0.15× | 0.45× |
| merge-sorted | 599.2 µs | 2.35 ms | 2.48 ms | 4.1× | 80.2 µs | 0.13× | 0.52× |
| merge-reverse | 634.8 µs | 2.39 ms | 2.52 ms | 4.0× | 98.2 µs | 0.15× | 0.49× |
| merge-duplicates | 714.2 µs | 2.53 ms | 2.68 ms | 3.8× | 85.6 µs | 0.12× | 0.44× |
| quick-random | 859.1 µs | 2.82 ms | 2.92 ms | 3.4× | 86.0 µs | 0.10× | 0.71× |
| quick-sorted | 724.3 µs | 2.44 ms | 2.56 ms | 3.5× | 81.4 µs | 0.11× | 0.67× |
| quick-reverse | 736.3 µs | 2.43 ms | 2.55 ms | 3.5× | 71.7 µs | 0.10× | 0.64× |
| quick-duplicates | 417.0 µs | 1.30 ms | 1.43 ms | 3.4× | 46.6 µs | 0.11× | 0.89× |
| json-aggregate | 7.88 ms | 7.73 ms | 7.95 ms | 1.0× | 1.83 ms | 0.23× | 0.05× |
| allocation-low-retention | 82.71 ms | 327 ms | 355 ms | 4.3× | 18.30 ms | 0.22× | 0.18× |
| allocation-high-retention | 150 ms | 453 ms | 492 ms | 3.3× | 26.75 ms | 0.18× | 0.17× |
| calls-empty | 22.2 µs | 23.3 µs | 48.6 µs | 2.2× | 0.030 µs | 0.00× | 0.04× |
| calls | 25.1 µs | 33.6 µs | 65.7 µs | 2.6× | 0.138 µs | 0.01× | 0.04× |
| json-hello | 30.8 µs | 30.5 µs | 56.4 µs | 1.8× | 0.292 µs | 0.01× | 0.03× |
| generate-sort | 4.25 ms | 4.24 ms | 4.34 ms | 1.0× | 898.1 µs | 0.21× | 0.54× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.

Against `22ef413fd0a0`: **0 faster, 1 slower, 20 unchanged** (net of control drift, threshold 5%).
- allocation-high-retention: 84.70 ms → 150 ms (+77.0%, control +29.0%) **slower**
