## Native bench `213c70cc6889` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 53.5 µs | 59.0 µs | 86.5 µs | 1.6× | 4.248 µs | 0.08× | 0.93× |
| array-empty | 22.9 µs | 30.2 µs | 57.5 µs | 2.5× | 0.007 µs | 0.00× | 0.04× |
| array-small | 24.4 µs | 88.8 µs | 126.7 µs | 5.2× | 0.100 µs | 0.00× | 0.08× |
| array-iterator | 122.9 µs | 3.62 ms | 4.40 ms | 36× | 3.218 µs | 0.03× | 0.86× |
| array-indexed | 116.4 µs | 420.1 µs | 446.7 µs | 3.8× | 5.962 µs | 0.05× | 0.10× |
| array-build | 559.2 µs | 5.78 ms | 6.57 ms | 12× | 43.9 µs | 0.08× | 0.27× |
| json-aggregate | 7.59 ms | 7.62 ms | 7.74 ms | 1.0× | 1.77 ms | 0.23× | 0.05× |
| allocation-low-retention | 82.25 ms | 326 ms | 384 ms | 4.7× | 21.66 ms | 0.26× | 0.21× |
| allocation-high-retention | 138 ms | 447 ms | 481 ms | 3.5× | 24.03 ms | 0.17× | 0.17× |
| calls-empty | 22.7 µs | 23.9 µs | 47.8 µs | 2.1× | 0.029 µs | 0.00× | 0.03× |
| calls | 25.6 µs | 34.0 µs | 65.1 µs | 2.5× | 0.118 µs | 0.00× | 0.04× |
| json-hello | 30.3 µs | 29.8 µs | 55.6 µs | 1.8× | 0.257 µs | 0.01× | 0.03× |
| generate-sort | 4.21 ms | 4.18 ms | 4.25 ms | 1.0× | 878.0 µs | 0.21× | 0.55× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.

Against `22ef413fd0a0`: **0 faster, 1 slower, 12 unchanged** (net of control drift, threshold 5%).
- allocation-high-retention: 84.70 ms → 138 ms (+63.2%, control -0.1%) **slower**
