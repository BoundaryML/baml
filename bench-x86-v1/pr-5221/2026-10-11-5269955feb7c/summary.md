## Native bench `5269955feb7c` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 3.888 µs | 32.2 µs | 78.0 µs | 20× | 4.268 µs | 1.1× | 13× |
| array-empty | 0.087 µs | 29.0 µs | 56.1 µs | 646× | 0.006 µs | 0.07× | 9.2× |
| array-small | 0.175 µs | 89.9 µs | 125.9 µs | 720× | 0.094 µs | 0.54× | 10× |
| array-iterator | 5.279 µs | 3.80 ms | 4.43 ms | 839× | 2.747 µs | 0.52× | 20× |
| array-indexed | 5.322 µs | 447.4 µs | 474.2 µs | 89× | 5.226 µs | 0.98× | 2.2× |
| array-build | 29.8 µs | 6.18 ms | 6.89 ms | 231× | 40.4 µs | 1.4× | 5.1× |
| merge-random | 58.1 µs | 2.66 ms | 2.79 ms | 48× | 89.0 µs | 1.5× | 5.6× |
| merge-sorted | 45.0 µs | 2.40 ms | 2.54 ms | 56× | 77.7 µs | 1.7× | 6.8× |
| merge-reverse | 45.7 µs | 2.46 ms | 2.60 ms | 57× | 77.1 µs | 1.7× | 6.7× |
| merge-duplicates | 50.3 µs | 2.60 ms | 2.75 ms | 55× | 66.9 µs | 1.3× | 6.1× |
| quick-random | 57.9 µs | 2.91 ms | 3.04 ms | 52× | 68.2 µs | 1.2× | 10× |
| quick-sorted | 49.1 µs | 2.53 ms | 2.65 ms | 54× | 64.1 µs | 1.3× | 9.7× |
| quick-reverse | 48.9 µs | 2.52 ms | 2.64 ms | 54× | 63.6 µs | 1.3× | 9.4× |
| quick-duplicates | 19.3 µs | 1.34 ms | 1.47 ms | 76× | 38.0 µs | 2.0× | 14× |
| json-aggregate | 849.1 µs | 21.96 ms | 22.82 ms | 27× | 1.76 ms | 2.1× | 0.47× |
| allocation-low-retention | 17.44 ms | 329 ms | 352 ms | 20× | 17.92 ms | 1.0× | 0.76× |
| allocation-high-retention | 21.52 ms | 447 ms | 479 ms | 22× | 23.17 ms | 1.1× | 1.0× |
| calls-empty | 0.066 µs | 22.6 µs | 46.7 µs | 708× | 0.025 µs | 0.38× | 11× |
| calls | 0.152 µs | 33.2 µs | 63.6 µs | 419× | 0.116 µs | 0.76× | 6.8× |
| json-hello | 0.064 µs | 29.3 µs | 56.3 µs | 885× | 0.249 µs | 3.9× | 13× |
| generate-sort | 274.8 µs | 4.37 ms | 4.41 ms | 16× | 803.1 µs | 2.9× | 8.4× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.
