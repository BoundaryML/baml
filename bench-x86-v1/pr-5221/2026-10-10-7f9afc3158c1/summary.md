## Native bench `7f9afc3158c1` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 3.887 µs | 46.4 µs | 79.2 µs | 20× | 8.361 µs | 2.2× | 13× |
| array-empty | 0.074 µs | 28.8 µs | 56.7 µs | 772× | 0.006 µs | 0.08× | 11× |
| array-small | 0.166 µs | 89.4 µs | 125.9 µs | 759× | 0.094 µs | 0.56× | 11× |
| array-iterator | 5.322 µs | 3.74 ms | 4.39 ms | 824× | 2.750 µs | 0.52× | 20× |
| array-indexed | 5.306 µs | 446.0 µs | 471.5 µs | 89× | 5.217 µs | 0.98× | 2.2× |
| array-build | 25.5 µs | 6.17 ms | 6.74 ms | 264× | 40.3 µs | 1.6× | 5.9× |
| merge-random | 58.6 µs | 2.68 ms | 2.78 ms | 47× | 89.1 µs | 1.5× | 5.6× |
| merge-sorted | 47.1 µs | 2.40 ms | 2.54 ms | 54× | 79.0 µs | 1.7× | 6.6× |
| merge-reverse | 45.9 µs | 2.47 ms | 2.58 ms | 56× | 77.3 µs | 1.7× | 6.7× |
| merge-duplicates | 50.5 µs | 2.60 ms | 2.75 ms | 54× | 68.0 µs | 1.3× | 6.2× |
| quick-random | 60.0 µs | 2.88 ms | 3.00 ms | 50× | 69.5 µs | 1.2× | 10× |
| quick-sorted | 51.4 µs | 2.50 ms | 2.61 ms | 51× | 65.3 µs | 1.3× | 9.3× |
| quick-reverse | 51.2 µs | 2.49 ms | 2.62 ms | 51× | 64.2 µs | 1.3× | 9.2× |
| quick-duplicates | 19.9 µs | 1.34 ms | 1.45 ms | 73× | 37.6 µs | 1.9× | 19× |
| json-aggregate | 757.0 µs | 22.03 ms | 22.72 ms | 30× | 1.77 ms | 2.3× | 0.52× |
| allocation-low-retention | 16.54 ms | 324 ms | 349 ms | 21× | 17.83 ms | 1.1× | 0.82× |
| allocation-high-retention | 22.46 ms | 443 ms | 475 ms | 21× | 23.29 ms | 1.0× | 1.0× |
| calls-empty | 0.053 µs | 22.7 µs | 46.8 µs | 886× | 0.025 µs | 0.48× | 14× |
| calls | 0.136 µs | 32.8 µs | 63.5 µs | 466× | 0.118 µs | 0.87× | 7.8× |
| json-hello | 0.065 µs | 29.5 µs | 55.8 µs | 860× | 0.260 µs | 4.0× | 13× |
| generate-sort | 276.1 µs | 4.38 ms | 4.40 ms | 16× | 815.6 µs | 3.0× | 8.3× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.
