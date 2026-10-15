## Native bench `031a5adcf02a` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 58.8 µs | 63.8 µs | 87.3 µs | 1.5× | 4.534 µs | 0.08× | 0.86× |
| array-empty | 29.7 µs | 29.4 µs | 56.2 µs | 1.9× | 0.007 µs | 0.00× | 0.03× |
| array-small | 90.8 µs | 89.4 µs | 128.1 µs | 1.4× | 0.094 µs | 0.00× | 0.02× |
| array-iterator | 3.72 ms | 3.69 ms | 4.45 ms | 1.2× | 3.135 µs | 0.00× | 0.03× |
| array-indexed | 434.9 µs | 431.8 µs | 453.5 µs | 1.0× | 5.261 µs | 0.01× | 0.03× |
| array-build | 6.04 ms | 5.99 ms | 6.71 ms | 1.1× | 44.4 µs | 0.01× | 0.03× |
| merge-random | 2.55 ms | 2.56 ms | 2.69 ms | 1.1× | 90.2 µs | 0.04× | 0.13× |
| merge-sorted | 2.31 ms | 2.31 ms | 2.42 ms | 1.0× | 80.5 µs | 0.03× | 0.13× |
| merge-reverse | 2.35 ms | 2.37 ms | 2.48 ms | 1.1× | 93.8 µs | 0.04× | 0.13× |
| merge-duplicates | 2.51 ms | 2.48 ms | 2.63 ms | 1.0× | 70.7 µs | 0.03× | 0.12× |
| quick-random | 2.75 ms | 2.73 ms | 2.86 ms | 1.0× | 86.2 µs | 0.03× | 0.22× |
| quick-sorted | 2.39 ms | 2.39 ms | 2.52 ms | 1.1× | 80.3 µs | 0.03× | 0.20× |
| quick-reverse | 2.38 ms | 2.37 ms | 2.50 ms | 1.1× | 73.9 µs | 0.03× | 0.20× |
| quick-duplicates | 1.31 ms | 1.30 ms | 1.43 ms | 1.1× | 39.0 µs | 0.03× | 0.14× |
| json-aggregate | 22.80 ms | 22.69 ms | 23.54 ms | 1.0× | 1.84 ms | 0.08× | 0.02× |
| allocation-low-retention | 328 ms | 327 ms | 351 ms | 1.1× | 17.84 ms | 0.05× | 0.04× |
| allocation-high-retention | 447 ms | 450 ms | 483 ms | 1.1× | 24.26 ms | 0.05× | 0.05× |
| calls-empty | 23.0 µs | 23.5 µs | 47.6 µs | 2.1× | 0.028 µs | 0.00× | 0.03× |
| calls | 38.1 µs | 33.3 µs | 64.9 µs | 1.7× | 0.136 µs | 0.00× | 0.03× |
| json-hello | 29.8 µs | 30.0 µs | 57.0 µs | 1.9× | 0.254 µs | 0.01× | 0.03× |
| generate-sort | 4.26 ms | 4.27 ms | 4.28 ms | 1.0× | 849.1 µs | 0.20× | 0.54× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.
