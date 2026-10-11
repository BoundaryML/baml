## Native bench `622fff115cbb` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 3.757 µs | 49.8 µs | 81.4 µs | 22× | 12.0 µs | 3.2× | 14× |
| array-empty | 0.073 µs | 29.3 µs | 56.3 µs | 770× | 0.006 µs | 0.08× | 11× |
| array-small | 0.177 µs | 91.0 µs | 126.8 µs | 718× | 0.093 µs | 0.53× | 11× |
| array-iterator | 5.588 µs | 3.81 ms | 4.46 ms | 798× | 2.815 µs | 0.50× | 19× |
| array-indexed | 5.850 µs | 446.4 µs | 475.0 µs | 81× | 5.238 µs | 0.90× | 2.0× |
| array-build | 25.6 µs | 6.31 ms | 6.88 ms | 269× | 41.4 µs | 1.6× | 5.9× |
| merge-random | 59.3 µs | 2.67 ms | 2.80 ms | 47× | 91.5 µs | 1.5× | 5.7× |
| merge-sorted | 47.6 µs | 2.42 ms | 2.53 ms | 53× | 80.7 µs | 1.7× | 6.6× |
| merge-reverse | 47.0 µs | 2.46 ms | 2.60 ms | 55× | 80.7 µs | 1.7× | 6.7× |
| merge-duplicates | 52.7 µs | 2.61 ms | 2.75 ms | 52× | 69.6 µs | 1.3× | 6.1× |
| quick-random | 61.4 µs | 2.90 ms | 3.01 ms | 49× | 70.9 µs | 1.2× | 9.9× |
| quick-sorted | 51.1 µs | 2.53 ms | 2.64 ms | 52× | 65.7 µs | 1.3× | 9.7× |
| quick-reverse | 50.8 µs | 2.50 ms | 2.62 ms | 51× | 65.6 µs | 1.3× | 9.5× |
| quick-duplicates | 20.8 µs | 1.35 ms | 1.48 ms | 71× | 38.5 µs | 1.9× | 18× |
| json-aggregate | 762.4 µs | 22.05 ms | 23.13 ms | 30× | 1.83 ms | 2.4× | 0.52× |
| allocation-low-retention | 16.57 ms | 331 ms | 358 ms | 22× | 18.18 ms | 1.1× | 0.88× |
| allocation-high-retention | 22.52 ms | 457 ms | 491 ms | 22× | 24.10 ms | 1.1× | 1.1× |
| calls-empty | 0.052 µs | 22.8 µs | 47.2 µs | 903× | 0.027 µs | 0.51× | 14× |
| calls | 0.138 µs | 33.3 µs | 64.4 µs | 467× | 0.116 µs | 0.84× | 8.3× |
| json-hello | 0.062 µs | 29.6 µs | 56.4 µs | 906× | 0.251 µs | 4.0× | 13× |
| generate-sort | 276.7 µs | 4.38 ms | 4.44 ms | 16× | 821.1 µs | 3.0× | 8.4× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.
