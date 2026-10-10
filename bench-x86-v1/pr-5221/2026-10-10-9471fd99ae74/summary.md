## Native bench `9471fd99ae74` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 4.378 µs | 37.8 µs | 79.0 µs | 18× | 4.719 µs | 1.1× | 11× |
| array-empty | 0.073 µs | 28.8 µs | 55.6 µs | 759× | 0.006 µs | 0.08× | 11× |
| array-small | 0.164 µs | 88.7 µs | 124.1 µs | 756× | 0.090 µs | 0.55× | 11× |
| array-iterator | 5.274 µs | 3.75 ms | 4.36 ms | 826× | 2.725 µs | 0.52× | 20× |
| array-indexed | 5.315 µs | 444.7 µs | 468.5 µs | 88× | 5.225 µs | 0.98× | 2.1× |
| array-build | 25.3 µs | 6.11 ms | 6.74 ms | 267× | 40.3 µs | 1.6× | 5.9× |
| merge-random | 58.6 µs | 2.68 ms | 2.81 ms | 48× | 87.4 µs | 1.5× | 5.6× |
| merge-sorted | 47.1 µs | 2.42 ms | 2.56 ms | 54× | 76.9 µs | 1.6× | 6.5× |
| merge-reverse | 45.7 µs | 2.48 ms | 2.61 ms | 57× | 76.0 µs | 1.7× | 6.7× |
| merge-duplicates | 51.4 µs | 2.62 ms | 2.76 ms | 54× | 67.0 µs | 1.3× | 6.0× |
| quick-random | 61.5 µs | 2.88 ms | 3.00 ms | 49× | 68.1 µs | 1.1× | 9.8× |
| quick-sorted | 51.0 µs | 2.50 ms | 2.61 ms | 51× | 63.1 µs | 1.2× | 9.4× |
| quick-reverse | 50.2 µs | 2.50 ms | 2.61 ms | 52× | 63.5 µs | 1.3× | 9.2× |
| quick-duplicates | 20.3 µs | 1.35 ms | 1.48 ms | 73× | 37.0 µs | 1.8× | 18× |
| json-aggregate | 748.4 µs | 21.84 ms | 22.73 ms | 30× | 1.77 ms | 2.4× | 0.53× |
| allocation-low-retention | 16.44 ms | 344 ms | 369 ms | 22× | 17.87 ms | 1.1× | 0.80× |
| allocation-high-retention | 21.02 ms | 456 ms | 490 ms | 23× | 23.51 ms | 1.1× | 1.1× |
| calls-empty | 0.053 µs | 20.9 µs | 46.6 µs | 879× | 0.026 µs | 0.48× | 14× |
| calls | 0.134 µs | 32.7 µs | 63.6 µs | 475× | 0.115 µs | 0.86× | 7.6× |
| json-hello | 0.063 µs | 29.2 µs | 55.5 µs | 876× | 0.244 µs | 3.8× | 13× |
| generate-sort | 272.8 µs | 4.39 ms | 4.39 ms | 16× | 798.4 µs | 2.9× | 8.3× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.
