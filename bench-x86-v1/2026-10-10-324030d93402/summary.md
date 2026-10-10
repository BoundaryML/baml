## Native bench `324030d93402` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 3.812 µs | 53.2 µs | 77.1 µs | 20× | 4.073 µs | 1.1× | 14× |
| array-empty | 0.096 µs | 29.2 µs | 57.2 µs | 598× | 0.008 µs | 0.08× | 9.2× |
| array-small | 0.262 µs | 90.7 µs | 128.2 µs | 489× | 0.128 µs | 0.49× | 7.7× |
| array-iterator | 9.594 µs | 3.84 ms | 4.48 ms | 467× | 4.342 µs | 0.45× | 12× |
| array-indexed | 9.302 µs | 467.5 µs | 498.4 µs | 54× | 6.804 µs | 0.73× | 1.3× |
| array-build | 30.8 µs | 6.22 ms | 6.83 ms | 222× | 45.6 µs | 1.5× | 5.4× |
| merge-random | 74.5 µs | 2.71 ms | 2.82 ms | 38× | 103.0 µs | 1.4× | 4.7× |
| merge-sorted | 66.7 µs | 2.45 ms | 2.58 ms | 39× | 88.9 µs | 1.3× | 5.0× |
| merge-reverse | 61.8 µs | 2.50 ms | 2.64 ms | 43× | 93.5 µs | 1.5× | 5.4× |
| merge-duplicates | 68.3 µs | 2.67 ms | 2.77 ms | 41× | 78.3 µs | 1.1× | 5.0× |
| quick-random | 71.2 µs | 2.97 ms | 3.09 ms | 43× | 80.7 µs | 1.1× | 9.2× |
| quick-sorted | 59.5 µs | 2.56 ms | 2.67 ms | 45× | 75.2 µs | 1.3× | 8.6× |
| quick-reverse | 61.3 µs | 2.52 ms | 2.65 ms | 43× | 77.9 µs | 1.3× | 8.1× |
| quick-duplicates | 26.2 µs | 1.36 ms | 1.49 ms | 57× | 47.6 µs | 1.8× | 17× |
| json-aggregate | 772.7 µs | 22.28 ms | 23.28 ms | 30× | 1.84 ms | 2.4× | 0.54× |
| allocation-low-retention | 16.63 ms | 332 ms | 355 ms | 21× | 18.28 ms | 1.1× | 0.84× |
| allocation-high-retention | 24.49 ms | 450 ms | 481 ms | 20× | 24.37 ms | 1.00× | 0.97× |
| calls-empty | 0.072 µs | 22.7 µs | 47.5 µs | 662× | 0.038 µs | 0.53× | 11× |
| calls | 0.205 µs | 33.6 µs | 64.6 µs | 316× | 0.168 µs | 0.82× | 5.4× |
| json-hello | 0.087 µs | 29.5 µs | 56.8 µs | 657× | 0.353 µs | 4.1× | 9.8× |
| generate-sort | 285.8 µs | 4.45 ms | 4.48 ms | 16× | 871.6 µs | 3.1× | 8.3× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.
