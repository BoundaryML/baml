## Native bench `324030d93402` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 3.747 µs | 48.9 µs | 79.9 µs | 21× | 3.596 µs | 0.96× | 14× |
| array-empty | 0.078 µs | 29.6 µs | 56.5 µs | 725× | 0.007 µs | 0.09× | 11× |
| array-small | 0.237 µs | 90.4 µs | 125.7 µs | 529× | 0.117 µs | 0.49× | 8.3× |
| array-iterator | 8.342 µs | 3.78 ms | 4.50 ms | 539× | 2.946 µs | 0.35× | 14× |
| array-indexed | 8.094 µs | 468.1 µs | 498.7 µs | 62× | 5.388 µs | 0.67× | 1.4× |
| array-build | 27.7 µs | 6.21 ms | 6.76 ms | 244× | 43.2 µs | 1.6× | 5.6× |
| merge-random | 62.8 µs | 2.72 ms | 2.83 ms | 45× | 94.2 µs | 1.5× | 5.4× |
| merge-sorted | 53.5 µs | 2.41 ms | 2.56 ms | 48× | 80.9 µs | 1.5× | 5.8× |
| merge-reverse | 52.7 µs | 2.49 ms | 2.62 ms | 50× | 82.7 µs | 1.6× | 6.0× |
| merge-duplicates | 57.8 µs | 2.63 ms | 2.77 ms | 48× | 78.3 µs | 1.4× | 5.5× |
| quick-random | 64.6 µs | 2.91 ms | 3.04 ms | 47× | 74.3 µs | 1.1× | 9.6× |
| quick-sorted | 54.4 µs | 2.52 ms | 2.68 ms | 49× | 68.4 µs | 1.3× | 8.9× |
| quick-reverse | 53.6 µs | 2.52 ms | 2.63 ms | 49× | 71.1 µs | 1.3× | 8.9× |
| quick-duplicates | 23.8 µs | 1.35 ms | 1.48 ms | 62× | 41.3 µs | 1.7× | 16× |
| json-aggregate | 755.7 µs | 21.83 ms | 22.74 ms | 30× | 1.77 ms | 2.3× | 0.54× |
| allocation-low-retention | 16.50 ms | 332 ms | 355 ms | 22× | 17.85 ms | 1.1× | 0.84× |
| allocation-high-retention | 22.51 ms | 453 ms | 482 ms | 21× | 23.83 ms | 1.1× | 1.0× |
| calls-empty | 0.056 µs | 22.6 µs | 46.8 µs | 843× | 0.026 µs | 0.47× | 14× |
| calls | 0.147 µs | 33.5 µs | 64.0 µs | 435× | 0.130 µs | 0.88× | 7.2× |
| json-hello | 0.073 µs | 29.8 µs | 56.6 µs | 779× | 0.277 µs | 3.8× | 12× |
| generate-sort | 282.6 µs | 4.43 ms | 4.43 ms | 16× | 824.6 µs | 2.9× | 8.2× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.
