## Native bench `531c9bc1d47d` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 56.7 µs | 63.0 µs | 91.7 µs | 1.6× | 4.103 µs | 0.07× | 0.93× |
| array-empty | 23.1 µs | 30.4 µs | 57.1 µs | 2.5× | 0.007 µs | 0.00× | 0.04× |
| array-small | 24.4 µs | 88.9 µs | 128.2 µs | 5.3× | 0.097 µs | 0.00× | 0.08× |
| array-iterator | 122.6 µs | 3.73 ms | 4.39 ms | 36× | 2.776 µs | 0.02× | 0.87× |
| array-indexed | 116.9 µs | 438.2 µs | 463.0 µs | 4.0× | 6.543 µs | 0.06× | 0.10× |
| array-build | 549.3 µs | 5.93 ms | 6.81 ms | 12× | 44.3 µs | 0.08× | 0.28× |
| merge-random | 706.1 µs | 2.56 ms | 2.70 ms | 3.8× | 110.8 µs | 0.16× | 0.47× |
| merge-sorted | 597.5 µs | 2.31 ms | 2.43 ms | 4.1× | 81.7 µs | 0.14× | 0.52× |
| merge-reverse | 618.5 µs | 2.37 ms | 2.50 ms | 4.0× | 98.6 µs | 0.16× | 0.50× |
| merge-duplicates | 673.7 µs | 2.51 ms | 2.62 ms | 3.9× | 70.8 µs | 0.11× | 0.46× |
| quick-random | 860.6 µs | 2.74 ms | 2.84 ms | 3.3× | 86.7 µs | 0.10× | 0.71× |
| quick-sorted | 724.6 µs | 2.41 ms | 2.50 ms | 3.4× | 80.3 µs | 0.11× | 0.67× |
| quick-reverse | 721.5 µs | 2.38 ms | 2.49 ms | 3.4× | 80.3 µs | 0.11× | 0.65× |
| quick-duplicates | 409.3 µs | 1.31 ms | 1.41 ms | 3.4× | 45.8 µs | 0.11× | 0.93× |
| json-aggregate | 7.81 ms | 7.86 ms | 7.94 ms | 1.0× | 1.79 ms | 0.23× | 0.05× |
| allocation-low-retention | 85.60 ms | 325 ms | 349 ms | 4.1× | 19.01 ms | 0.22× | 0.18× |
| allocation-high-retention | 141 ms | 436 ms | 480 ms | 3.4× | 23.64 ms | 0.17× | 0.16× |
| calls-empty | 22.5 µs | 23.6 µs | 47.5 µs | 2.1× | 0.030 µs | 0.00× | 0.03× |
| calls | 25.3 µs | 33.8 µs | 64.4 µs | 2.5× | 0.144 µs | 0.01× | 0.04× |
| json-hello | 31.0 µs | 29.9 µs | 56.2 µs | 1.8× | 0.249 µs | 0.01× | 0.03× |
| generate-sort | 4.19 ms | 4.28 ms | 4.25 ms | 1.0× | 870.3 µs | 0.21× | 0.55× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.

Against `22ef413fd0a0`: **0 faster, 2 slower, 19 unchanged** (net of control drift, threshold 5%).
- array-indexed: 126.9 µs → 116.9 µs (-7.9%, control -20.6%) **slower**
- allocation-high-retention: 84.70 ms → 141 ms (+65.9%, control -1.8%) **slower**
