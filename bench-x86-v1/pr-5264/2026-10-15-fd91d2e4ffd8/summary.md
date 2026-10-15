## Native bench `fd91d2e4ffd8` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 74.0 µs | 82.8 µs | 115.5 µs | 1.6× | 12.8 µs | 0.17× | 0.94× |
| array-empty | 30.4 µs | 31.8 µs | 62.2 µs | 2.0× | 0.007 µs | 0.00× | 0.03× |
| array-small | 93.7 µs | 93.8 µs | 135.8 µs | 1.4× | 0.093 µs | 0.00× | 0.02× |
| array-iterator | 3.98 ms | 3.89 ms | 4.82 ms | 1.2× | 2.759 µs | 0.00× | 0.03× |
| array-indexed | 464.9 µs | 445.2 µs | 481.9 µs | 1.0× | 6.062 µs | 0.01× | 0.03× |
| array-build | 6.27 ms | 6.24 ms | 7.24 ms | 1.2× | 54.3 µs | 0.01× | 0.03× |
| merge-random | 2.62 ms | 2.71 ms | 2.89 ms | 1.1× | 97.1 µs | 0.04× | 0.15× |
| merge-sorted | 2.45 ms | 2.34 ms | 2.55 ms | 1.0× | 82.2 µs | 0.03× | 0.14× |
| merge-reverse | 2.48 ms | 2.46 ms | 2.59 ms | 1.0× | 85.6 µs | 0.03× | 0.14× |
| merge-duplicates | 2.54 ms | 2.61 ms | 2.85 ms | 1.1× | 71.3 µs | 0.03× | 0.14× |
| quick-random | 2.81 ms | 2.79 ms | 2.96 ms | 1.1× | 74.7 µs | 0.03× | 0.24× |
| quick-sorted | 2.56 ms | 2.48 ms | 2.65 ms | 1.0× | 80.1 µs | 0.03× | 0.21× |
| quick-reverse | 2.51 ms | 2.45 ms | 2.61 ms | 1.0× | 67.9 µs | 0.03× | 0.24× |
| quick-duplicates | 1.34 ms | 1.41 ms | 1.53 ms | 1.1× | 46.8 µs | 0.03× | 0.32× |
| json-aggregate | 24.09 ms | 23.76 ms | 25.57 ms | 1.1× | 2.02 ms | 0.08× | 0.02× |
| allocation-low-retention | 342 ms | 339 ms | 372 ms | 1.1× | 19.25 ms | 0.06× | 0.05× |
| allocation-high-retention | 476 ms | 477 ms | 512 ms | 1.1× | 27.30 ms | 0.06× | 0.05× |
| calls-empty | 23.7 µs | 23.7 µs | 49.8 µs | 2.1× | 0.031 µs | 0.00× | 0.04× |
| calls | 39.8 µs | 35.3 µs | 67.2 µs | 1.7× | 0.138 µs | 0.00× | 0.03× |
| json-hello | 30.8 µs | 31.3 µs | 58.9 µs | 1.9× | 0.262 µs | 0.01× | 0.03× |
| generate-sort | 4.52 ms | 4.42 ms | 4.77 ms | 1.1× | 900.1 µs | 0.20× | 0.53× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.
