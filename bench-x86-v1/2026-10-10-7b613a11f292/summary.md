## Native bench `7b613a11f292` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 4.238 µs | 46.9 µs | 74.3 µs | 18× | 6.912 µs | 1.6× | 12× |
| array-empty | 0.076 µs | 29.0 µs | 55.9 µs | 736× | 0.006 µs | 0.08× | 11× |
| array-small | 0.177 µs | 90.6 µs | 126.4 µs | 713× | 0.093 µs | 0.52× | 10× |
| array-iterator | 5.399 µs | 3.78 ms | 4.41 ms | 817× | 2.706 µs | 0.50× | 20× |
| array-indexed | 5.492 µs | 445.9 µs | 472.3 µs | 86× | 5.231 µs | 0.95× | 2.1× |
| array-build | 25.4 µs | 6.17 ms | 6.79 ms | 268× | 41.1 µs | 1.6× | 6.0× |
| merge-random | 59.0 µs | 2.66 ms | 2.79 ms | 47× | 90.0 µs | 1.5× | 5.7× |
| merge-sorted | 47.3 µs | 2.41 ms | 2.54 ms | 54× | 78.8 µs | 1.7× | 6.6× |
| merge-reverse | 45.8 µs | 2.45 ms | 2.58 ms | 56× | 77.8 µs | 1.7× | 6.8× |
| merge-duplicates | 50.2 µs | 2.61 ms | 2.73 ms | 54× | 68.3 µs | 1.4× | 6.3× |
| quick-random | 59.8 µs | 2.89 ms | 3.03 ms | 51× | 69.3 µs | 1.2× | 10× |
| quick-sorted | 51.0 µs | 2.51 ms | 2.63 ms | 51× | 64.7 µs | 1.3× | 9.6× |
| quick-reverse | 51.2 µs | 2.50 ms | 2.61 ms | 51× | 64.0 µs | 1.3× | 9.3× |
| quick-duplicates | 20.3 µs | 1.34 ms | 1.46 ms | 72× | 38.2 µs | 1.9× | 19× |
| json-aggregate | 778.4 µs | 22.12 ms | 22.94 ms | 29× | 1.77 ms | 2.3× | 0.51× |
| allocation-low-retention | 16.52 ms | 329 ms | 356 ms | 22× | 18.05 ms | 1.1× | 0.82× |
| allocation-high-retention | 21.96 ms | 449 ms | 480 ms | 22× | 23.62 ms | 1.1× | 1.1× |
| calls-empty | 0.052 µs | 23.1 µs | 47.0 µs | 904× | 0.026 µs | 0.49× | 14× |
| calls | 0.137 µs | 33.3 µs | 63.9 µs | 466× | 0.116 µs | 0.85× | 7.8× |
| json-hello | 0.062 µs | 29.9 µs | 56.1 µs | 905× | 0.248 µs | 4.0× | 13× |
| generate-sort | 277.8 µs | 4.39 ms | 4.42 ms | 16× | 820.1 µs | 3.0× | 8.3× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.

Against `7b613a11f292`: **0 faster, 0 slower, 21 unchanged** (net of control drift, threshold 5%).
