## Native bench `e4d266ff8888` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 4.323 µs | 41.0 µs | 73.9 µs | 17× | 4.263 µs | 0.99× | 12× |
| array-empty | 0.085 µs | 29.5 µs | 57.2 µs | 675× | 0.006 µs | 0.07× | 9.6× |
| array-small | 0.177 µs | 90.8 µs | 130.1 µs | 735× | 0.093 µs | 0.52× | 10× |
| array-iterator | 5.430 µs | 3.79 ms | 4.43 ms | 816× | 2.730 µs | 0.50× | 20× |
| array-indexed | 5.284 µs | 450.0 µs | 483.7 µs | 92× | 5.298 µs | 1.0× | 2.2× |
| array-build | 30.2 µs | 6.21 ms | 6.75 ms | 223× | 40.9 µs | 1.4× | 5.1× |
| merge-random | 58.7 µs | 2.72 ms | 2.87 ms | 49× | 90.1 µs | 1.5× | 5.8× |
| merge-sorted | 46.0 µs | 2.46 ms | 2.58 ms | 56× | 78.3 µs | 1.7× | 6.7× |
| merge-reverse | 47.2 µs | 2.51 ms | 2.65 ms | 56× | 78.2 µs | 1.7× | 6.6× |
| merge-duplicates | 52.3 µs | 2.65 ms | 2.78 ms | 53× | 68.5 µs | 1.3× | 6.0× |
| quick-random | 58.3 µs | 2.96 ms | 3.09 ms | 53× | 70.2 µs | 1.2× | 11× |
| quick-sorted | 49.2 µs | 2.57 ms | 2.70 ms | 55× | 65.5 µs | 1.3× | 10.0× |
| quick-reverse | 49.5 µs | 2.56 ms | 2.68 ms | 54× | 64.6 µs | 1.3× | 9.6× |
| quick-duplicates | 19.8 µs | 1.36 ms | 1.50 ms | 76× | 38.1 µs | 1.9× | 19× |
| json-aggregate | 857.5 µs | 22.24 ms | 22.88 ms | 27× | 1.80 ms | 2.1× | 0.47× |
| allocation-low-retention | 16.86 ms | 331 ms | 355 ms | 21× | 18.08 ms | 1.1× | 0.81× |
| allocation-high-retention | 21.21 ms | 446 ms | 479 ms | 23× | 23.41 ms | 1.1× | 1.1× |
| calls-empty | 0.065 µs | 23.0 µs | 47.5 µs | 726× | 0.026 µs | 0.39× | 11× |
| calls | 0.160 µs | 33.3 µs | 64.6 µs | 404× | 0.116 µs | 0.72× | 6.6× |
| json-hello | 0.065 µs | 29.4 µs | 57.0 µs | 876× | 0.251 µs | 3.9× | 13× |
| generate-sort | 278.2 µs | 4.44 ms | 4.47 ms | 16× | 817.0 µs | 2.9× | 8.3× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.
