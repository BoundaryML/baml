## Native bench `7b613a11f292` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 4.113 µs | 44.6 µs | 79.8 µs | 19× | 8.691 µs | 2.1× | 12× |
| array-empty | 0.075 µs | 29.1 µs | 55.4 µs | 742× | 0.006 µs | 0.08× | 11× |
| array-small | 0.169 µs | 89.3 µs | 125.3 µs | 740× | 0.091 µs | 0.54× | 11× |
| array-iterator | 5.468 µs | 3.76 ms | 4.36 ms | 798× | 2.742 µs | 0.50× | 19× |
| array-indexed | 5.302 µs | 445.3 µs | 474.3 µs | 89× | 5.266 µs | 0.99× | 2.2× |
| array-build | 25.3 µs | 6.18 ms | 6.74 ms | 267× | 40.5 µs | 1.6× | 6.0× |
| merge-random | 58.7 µs | 2.65 ms | 2.79 ms | 48× | 88.8 µs | 1.5× | 5.6× |
| merge-sorted | 47.0 µs | 2.42 ms | 2.53 ms | 54× | 77.6 µs | 1.6× | 6.5× |
| merge-reverse | 45.6 µs | 2.46 ms | 2.60 ms | 57× | 77.5 µs | 1.7× | 6.7× |
| merge-duplicates | 50.8 µs | 2.61 ms | 2.75 ms | 54× | 68.3 µs | 1.3× | 6.1× |
| quick-random | 60.4 µs | 2.89 ms | 3.01 ms | 50× | 68.8 µs | 1.1× | 10× |
| quick-sorted | 51.8 µs | 2.51 ms | 2.63 ms | 51× | 64.4 µs | 1.2× | 9.3× |
| quick-reverse | 51.0 µs | 2.51 ms | 2.62 ms | 51× | 63.7 µs | 1.2× | 9.1× |
| quick-duplicates | 19.9 µs | 1.35 ms | 1.46 ms | 73× | 37.4 µs | 1.9× | 18× |
| json-aggregate | 752.2 µs | 21.93 ms | 22.60 ms | 30× | 1.77 ms | 2.4× | 0.53× |
| allocation-low-retention | 16.63 ms | 324 ms | 351 ms | 21× | 17.90 ms | 1.1× | 0.80× |
| allocation-high-retention | 22.67 ms | 443 ms | 476 ms | 21× | 23.40 ms | 1.0× | 0.96× |
| calls-empty | 0.052 µs | 22.8 µs | 46.7 µs | 896× | 0.025 µs | 0.48× | 14× |
| calls | 0.136 µs | 32.9 µs | 63.9 µs | 470× | 0.116 µs | 0.85× | 7.8× |
| json-hello | 0.063 µs | 29.5 µs | 55.9 µs | 886× | 0.248 µs | 3.9× | 13× |
| generate-sort | 276.9 µs | 4.37 ms | 4.43 ms | 16× | 812.0 µs | 2.9× | 8.3× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.
