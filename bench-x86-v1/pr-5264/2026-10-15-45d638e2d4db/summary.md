## Native bench `45d638e2d4db` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 57.6 µs | 62.8 µs | 88.2 µs | 1.5× | 4.213 µs | 0.07× | 0.88× |
| array-empty | 29.9 µs | 30.0 µs | 56.7 µs | 1.9× | 0.007 µs | 0.00× | 0.03× |
| array-small | 87.6 µs | 89.1 µs | 125.5 µs | 1.4× | 0.101 µs | 0.00× | 0.02× |
| array-iterator | 3.73 ms | 3.75 ms | 4.43 ms | 1.2× | 2.812 µs | 0.00× | 0.03× |
| array-indexed | 429.4 µs | 430.8 µs | 459.7 µs | 1.1× | 6.485 µs | 0.02× | 0.03× |
| array-build | 5.98 ms | 5.90 ms | 6.60 ms | 1.1× | 46.3 µs | 0.01× | 0.03× |
| merge-random | 2.55 ms | 2.55 ms | 2.66 ms | 1.0× | 103.8 µs | 0.04× | 0.13× |
| merge-sorted | 2.30 ms | 2.32 ms | 2.43 ms | 1.1× | 92.0 µs | 0.04× | 0.13× |
| merge-reverse | 2.35 ms | 2.37 ms | 2.48 ms | 1.1× | 93.2 µs | 0.04× | 0.13× |
| merge-duplicates | 2.50 ms | 2.48 ms | 2.61 ms | 1.0× | 85.1 µs | 0.03× | 0.13× |
| quick-random | 2.76 ms | 2.73 ms | 2.87 ms | 1.0× | 86.3 µs | 0.03× | 0.23× |
| quick-sorted | 2.39 ms | 2.43 ms | 2.51 ms | 1.1× | 82.6 µs | 0.03× | 0.21× |
| quick-reverse | 2.37 ms | 2.39 ms | 2.49 ms | 1.1× | 75.3 µs | 0.03× | 0.20× |
| quick-duplicates | 1.30 ms | 1.29 ms | 1.41 ms | 1.1× | 46.2 µs | 0.04× | 0.31× |
| json-aggregate | 22.65 ms | 22.57 ms | 23.61 ms | 1.0× | 1.78 ms | 0.08× | 0.02× |
| allocation-low-retention | 329 ms | 330 ms | 351 ms | 1.1× | 18.24 ms | 0.06× | 0.05× |
| allocation-high-retention | 441 ms | 444 ms | 476 ms | 1.1× | 23.76 ms | 0.05× | 0.05× |
| calls-empty | 23.2 µs | 23.2 µs | 47.6 µs | 2.1× | 0.025 µs | 0.00× | 0.03× |
| calls | 38.2 µs | 33.7 µs | 65.8 µs | 1.7× | 0.121 µs | 0.00× | 0.03× |
| json-hello | 29.7 µs | 29.9 µs | 56.5 µs | 1.9× | 0.257 µs | 0.01× | 0.03× |
| generate-sort | 4.23 ms | 4.21 ms | 4.25 ms | 1.0× | 868.3 µs | 0.21× | 0.55× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.
