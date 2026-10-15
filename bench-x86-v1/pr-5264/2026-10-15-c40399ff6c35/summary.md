## Native bench `c40399ff6c35` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 69.6 µs | 70.4 µs | 101.2 µs | 1.5× | 13.9 µs | 0.20× | 0.75× |
| array-empty | 31.1 µs | 31.2 µs | 59.9 µs | 1.9× | 0.009 µs | 0.00× | 0.03× |
| array-small | 93.9 µs | 94.5 µs | 133.7 µs | 1.4× | 0.120 µs | 0.00× | 0.02× |
| array-iterator | 3.96 ms | 3.91 ms | 4.77 ms | 1.2× | 2.918 µs | 0.00× | 0.03× |
| array-indexed | 449.0 µs | 449.8 µs | 473.3 µs | 1.1× | 6.808 µs | 0.02× | 0.03× |
| array-build | 6.45 ms | 6.33 ms | 7.20 ms | 1.1× | 54.7 µs | 0.01× | 0.02× |
| merge-random | 2.66 ms | 2.67 ms | 2.79 ms | 1.0× | 120.8 µs | 0.05× | 0.13× |
| merge-sorted | 2.39 ms | 2.44 ms | 2.56 ms | 1.1× | 83.4 µs | 0.03× | 0.13× |
| merge-reverse | 2.48 ms | 2.47 ms | 2.59 ms | 1.0× | 102.5 µs | 0.04× | 0.13× |
| merge-duplicates | 2.60 ms | 2.61 ms | 2.74 ms | 1.1× | 86.8 µs | 0.03× | 0.13× |
| quick-random | 2.89 ms | 2.92 ms | 3.05 ms | 1.1× | 80.3 µs | 0.03× | 0.22× |
| quick-sorted | 2.52 ms | 2.51 ms | 2.64 ms | 1.0× | 84.3 µs | 0.03× | 0.21× |
| quick-reverse | 2.51 ms | 2.60 ms | 2.62 ms | 1.0× | 88.5 µs | 0.04× | 0.19× |
| quick-duplicates | 1.37 ms | 1.37 ms | 1.50 ms | 1.1× | 49.3 µs | 0.04× | 0.29× |
| json-aggregate | 24.36 ms | 24.19 ms | 24.98 ms | 1.0× | 2.08 ms | 0.09× | 0.02× |
| allocation-low-retention | 347 ms | 346 ms | 373 ms | 1.1× | 19.01 ms | 0.05× | 0.04× |
| allocation-high-retention | 473 ms | 471 ms | 510 ms | 1.1× | 24.83 ms | 0.05× | 0.05× |
| calls-empty | 24.0 µs | 24.1 µs | 49.1 µs | 2.0× | 0.031 µs | 0.00× | 0.03× |
| calls | 40.0 µs | 35.2 µs | 67.4 µs | 1.7× | 0.141 µs | 0.00× | 0.03× |
| json-hello | 30.9 µs | 31.0 µs | 59.5 µs | 1.9× | 0.325 µs | 0.01× | 0.03× |
| generate-sort | 4.41 ms | 4.38 ms | 4.43 ms | 1.0× | 1.05 ms | 0.24× | 0.55× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.
