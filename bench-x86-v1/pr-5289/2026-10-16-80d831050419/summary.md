## Native bench `80d831050419` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 56.5 µs | 58.1 µs | 91.0 µs | 1.6× | 4.689 µs | 0.08× | 0.89× |
| array-empty | 23.8 µs | 31.3 µs | 58.7 µs | 2.5× | 0.007 µs | 0.00× | 0.04× |
| array-small | 24.8 µs | 89.6 µs | 127.9 µs | 5.2× | 0.095 µs | 0.00× | 0.08× |
| array-iterator | 121.9 µs | 3.73 ms | 4.55 ms | 37× | 2.689 µs | 0.02× | 0.88× |
| array-indexed | 115.7 µs | 433.4 µs | 463.3 µs | 4.0× | 5.294 µs | 0.05× | 0.11× |
| array-build | 553.4 µs | 5.94 ms | 6.68 ms | 12× | 49.3 µs | 0.09× | 0.28× |
| merge-random | 704.6 µs | 2.62 ms | 2.70 ms | 3.8× | 98.9 µs | 0.14× | 0.49× |
| merge-sorted | 590.0 µs | 2.35 ms | 2.48 ms | 4.2× | 80.8 µs | 0.14× | 0.58× |
| merge-reverse | 613.3 µs | 2.37 ms | 2.54 ms | 4.1× | 83.0 µs | 0.14× | 0.54× |
| merge-duplicates | 680.1 µs | 2.52 ms | 2.70 ms | 4.0× | 71.9 µs | 0.11× | 0.48× |
| quick-random | 851.6 µs | 2.74 ms | 2.96 ms | 3.5× | 81.5 µs | 0.10× | 0.75× |
| quick-sorted | 719.1 µs | 2.39 ms | 2.54 ms | 3.5× | 77.5 µs | 0.11× | 0.70× |
| quick-reverse | 727.1 µs | 2.44 ms | 2.55 ms | 3.5× | 79.2 µs | 0.11× | 0.66× |
| quick-duplicates | 403.3 µs | 1.33 ms | 1.44 ms | 3.6× | 46.0 µs | 0.11× | 1.0× |
| json-aggregate | 7.65 ms | 7.69 ms | 7.84 ms | 1.0× | 1.80 ms | 0.24× | 0.05× |
| allocation-low-retention | 87.85 ms | 322 ms | 349 ms | 4.0× | 19.43 ms | 0.22× | 0.19× |
| allocation-high-retention | 138 ms | 437 ms | 474 ms | 3.4× | 23.85 ms | 0.17× | 0.16× |
| calls-empty | 22.9 µs | 23.7 µs | 49.1 µs | 2.1× | 0.027 µs | 0.00× | 0.04× |
| calls | 25.6 µs | 34.1 µs | 66.7 µs | 2.6× | 0.122 µs | 0.00× | 0.05× |
| json-hello | 30.8 µs | 30.1 µs | 56.1 µs | 1.8× | 0.314 µs | 0.01× | 0.03× |
| generate-sort | 4.26 ms | 4.21 ms | 4.30 ms | 1.0× | 871.8 µs | 0.20× | 0.54× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.

Against `22ef413fd0a0`: **0 faster, 1 slower, 20 unchanged** (net of control drift, threshold 5%).
- allocation-high-retention: 84.70 ms → 138 ms (+62.7%, control -2.4%) **slower**
