## Native bench `82c45449e225` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 3.413 µs | 28.9 µs | 74.3 µs | 22× | 4.098 µs | 1.2× | 14× |
| array-empty | 0.085 µs | 28.2 µs | 54.1 µs | 635× | 0.006 µs | 0.07× | 9.3× |
| array-small | 0.172 µs | 89.0 µs | 124.9 µs | 728× | 0.092 µs | 0.53× | 10× |
| array-iterator | 5.275 µs | 3.82 ms | 4.42 ms | 837× | 2.700 µs | 0.51× | 20× |
| array-indexed | 5.276 µs | 443.1 µs | 477.5 µs | 90× | 5.223 µs | 0.99× | 2.2× |
| array-build | 29.9 µs | 6.17 ms | 6.74 ms | 226× | 39.1 µs | 1.3× | 5.0× |
| merge-random | 58.8 µs | 2.65 ms | 2.77 ms | 47× | 86.5 µs | 1.5× | 5.5× |
| merge-sorted | 44.9 µs | 2.39 ms | 2.52 ms | 56× | 74.6 µs | 1.7× | 6.7× |
| merge-reverse | 46.3 µs | 2.45 ms | 2.58 ms | 56× | 74.6 µs | 1.6× | 6.6× |
| merge-duplicates | 50.5 µs | 2.60 ms | 2.72 ms | 54× | 66.0 µs | 1.3× | 6.0× |
| quick-random | 57.2 µs | 2.87 ms | 3.01 ms | 53× | 67.4 µs | 1.2× | 10× |
| quick-sorted | 48.9 µs | 2.49 ms | 2.62 ms | 54× | 63.1 µs | 1.3× | 9.6× |
| quick-reverse | 48.0 µs | 2.49 ms | 2.61 ms | 54× | 62.5 µs | 1.3× | 9.5× |
| quick-duplicates | 19.3 µs | 1.33 ms | 1.45 ms | 75× | 36.5 µs | 1.9× | 9.1× |
| json-aggregate | 841.1 µs | 21.78 ms | 22.45 ms | 27× | 1.75 ms | 2.1× | 0.47× |
| allocation-low-retention | 16.64 ms | 327 ms | 352 ms | 21× | 17.60 ms | 1.1× | 0.78× |
| allocation-high-retention | 20.00 ms | 445 ms | 476 ms | 24× | 22.75 ms | 1.1× | 1.1× |
| calls-empty | 0.065 µs | 20.5 µs | 45.7 µs | 699× | 0.025 µs | 0.38× | 11× |
| calls | 0.151 µs | 32.4 µs | 62.2 µs | 413× | 0.116 µs | 0.77× | 6.9× |
| json-hello | 0.065 µs | 27.7 µs | 54.1 µs | 837× | 0.246 µs | 3.8× | 12× |
| generate-sort | 271.5 µs | 4.36 ms | 4.36 ms | 16× | 802.3 µs | 3.0× | 8.4× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.
