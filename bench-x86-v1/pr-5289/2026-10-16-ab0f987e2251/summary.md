## Native bench `ab0f987e2251` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 57.9 µs | 64.9 µs | 100.2 µs | 1.7× | 4.529 µs | 0.08× | 0.94× |
| array-empty | 22.9 µs | 30.4 µs | 57.5 µs | 2.5× | 0.007 µs | 0.00× | 0.04× |
| array-small | 24.7 µs | 88.6 µs | 125.3 µs | 5.1× | 0.097 µs | 0.00× | 0.08× |
| array-iterator | 124.1 µs | 3.62 ms | 4.38 ms | 35× | 3.132 µs | 0.03× | 0.87× |
| array-indexed | 119.0 µs | 439.3 µs | 470.8 µs | 4.0× | 5.412 µs | 0.05× | 0.10× |
| array-build | 561.1 µs | 5.94 ms | 6.62 ms | 12× | 49.5 µs | 0.09× | 0.27× |
| merge-random | 721.6 µs | 2.59 ms | 2.69 ms | 3.7× | 103.2 µs | 0.14× | 0.46× |
| merge-sorted | 606.3 µs | 2.32 ms | 2.45 ms | 4.0× | 81.0 µs | 0.13× | 0.51× |
| merge-reverse | 627.6 µs | 2.39 ms | 2.51 ms | 4.0× | 94.5 µs | 0.15× | 0.50× |
| merge-duplicates | 687.8 µs | 2.52 ms | 2.67 ms | 3.9× | 78.0 µs | 0.11× | 0.46× |
| quick-random | 843.2 µs | 2.77 ms | 2.89 ms | 3.4× | 79.4 µs | 0.09× | 0.72× |
| quick-sorted | 716.3 µs | 2.41 ms | 2.54 ms | 3.5× | 81.1 µs | 0.11× | 0.68× |
| quick-reverse | 707.6 µs | 2.40 ms | 2.51 ms | 3.5× | 73.6 µs | 0.10× | 0.67× |
| quick-duplicates | 409.3 µs | 1.30 ms | 1.44 ms | 3.5× | 41.2 µs | 0.10× | 0.95× |
| json-aggregate | 7.75 ms | 7.66 ms | 7.80 ms | 1.0× | 1.80 ms | 0.23× | 0.05× |
| allocation-low-retention | 82.79 ms | 325 ms | 353 ms | 4.3× | 17.94 ms | 0.22× | 0.18× |
| allocation-high-retention | 140 ms | 450 ms | 490 ms | 3.5× | 23.72 ms | 0.17× | 0.16× |
| calls-empty | 22.5 µs | 23.7 µs | 47.7 µs | 2.1× | 0.031 µs | 0.00× | 0.04× |
| calls | 25.4 µs | 34.1 µs | 66.0 µs | 2.6× | 0.142 µs | 0.01× | 0.05× |
| json-hello | 30.4 µs | 30.4 µs | 56.5 µs | 1.9× | 0.309 µs | 0.01× | 0.03× |
| generate-sort | 4.33 ms | 4.24 ms | 4.28 ms | 0.99× | 904.3 µs | 0.21× | 0.54× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.

Against `22ef413fd0a0`: **0 faster, 1 slower, 20 unchanged** (net of control drift, threshold 5%).
- allocation-high-retention: 84.70 ms → 140 ms (+65.4%, control -0.4%) **slower**
