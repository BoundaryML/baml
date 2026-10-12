## Native bench `e79ad755e977` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 4.298 µs | 51.9 µs | 81.4 µs | 19× | 3.918 µs | 0.91× | 12× |
| array-empty | 0.085 µs | 29.6 µs | 56.6 µs | 668× | 0.006 µs | 0.07× | 9.8× |
| array-small | 0.182 µs | 91.4 µs | 127.8 µs | 703× | 0.096 µs | 0.53× | 10× |
| array-iterator | 5.298 µs | 3.83 ms | 4.46 ms | 841× | 2.659 µs | 0.50× | 21× |
| array-indexed | 5.302 µs | 454.2 µs | 481.2 µs | 91× | 5.289 µs | 1.00× | 2.3× |
| array-build | 30.3 µs | 6.20 ms | 6.86 ms | 227× | 41.4 µs | 1.4× | 5.1× |
| merge-random | 59.9 µs | 2.71 ms | 2.87 ms | 48× | 90.5 µs | 1.5× | 5.6× |
| merge-sorted | 45.5 µs | 2.45 ms | 2.56 ms | 56× | 80.7 µs | 1.8× | 6.9× |
| merge-reverse | 47.7 µs | 2.52 ms | 2.64 ms | 55× | 79.6 µs | 1.7× | 6.6× |
| merge-duplicates | 51.1 µs | 2.64 ms | 2.79 ms | 55× | 68.1 µs | 1.3× | 6.3× |
| quick-random | 57.2 µs | 2.93 ms | 3.07 ms | 54× | 72.4 µs | 1.3× | 11× |
| quick-sorted | 49.2 µs | 2.56 ms | 2.68 ms | 55× | 67.0 µs | 1.4× | 9.9× |
| quick-reverse | 49.0 µs | 2.56 ms | 2.65 ms | 54× | 66.1 µs | 1.4× | 10× |
| quick-duplicates | 19.4 µs | 1.35 ms | 1.48 ms | 77× | 38.9 µs | 2.0× | 15× |
| json-aggregate | 866.2 µs | 22.29 ms | 23.23 ms | 27× | 1.81 ms | 2.1× | 0.47× |
| allocation-low-retention | 17.05 ms | 329 ms | 354 ms | 21× | 18.13 ms | 1.1× | 0.82× |
| allocation-high-retention | 22.53 ms | 447 ms | 482 ms | 21× | 24.71 ms | 1.1× | 1.00× |
| calls-empty | 0.066 µs | 22.8 µs | 48.2 µs | 734× | 0.025 µs | 0.38× | 12× |
| calls | 0.153 µs | 33.5 µs | 64.6 µs | 423× | 0.116 µs | 0.76× | 7.1× |
| json-hello | 0.065 µs | 29.6 µs | 56.8 µs | 878× | 0.248 µs | 3.8× | 13× |
| generate-sort | 281.7 µs | 4.41 ms | 4.47 ms | 16× | 819.9 µs | 2.9× | 8.3× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.
