## Native bench `e59d3348f492` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 3.852 µs | 48.8 µs | 74.5 µs | 19× | 4.469 µs | 1.2× | 13× |
| array-empty | 0.085 µs | 28.4 µs | 55.6 µs | 658× | 0.006 µs | 0.07× | 9.5× |
| array-small | 0.173 µs | 89.1 µs | 124.8 µs | 720× | 0.092 µs | 0.53× | 10× |
| array-iterator | 5.269 µs | 3.74 ms | 4.36 ms | 827× | 2.743 µs | 0.52× | 20× |
| array-indexed | 5.330 µs | 446.6 µs | 469.8 µs | 88× | 5.235 µs | 0.98× | 2.2× |
| array-build | 30.0 µs | 6.11 ms | 6.71 ms | 224× | 40.2 µs | 1.3× | 5.0× |
| merge-random | 57.6 µs | 2.68 ms | 2.82 ms | 49× | 88.9 µs | 1.5× | 5.8× |
| merge-sorted | 45.1 µs | 2.41 ms | 2.54 ms | 56× | 78.8 µs | 1.7× | 6.8× |
| merge-reverse | 46.6 µs | 2.46 ms | 2.60 ms | 56× | 77.7 µs | 1.7× | 6.6× |
| merge-duplicates | 50.4 µs | 2.61 ms | 2.75 ms | 55× | 67.4 µs | 1.3× | 6.1× |
| quick-random | 56.6 µs | 2.90 ms | 3.01 ms | 53× | 68.5 µs | 1.2× | 11× |
| quick-sorted | 48.7 µs | 2.52 ms | 2.65 ms | 54× | 65.1 µs | 1.3× | 9.8× |
| quick-reverse | 48.3 µs | 2.50 ms | 2.64 ms | 55× | 63.8 µs | 1.3× | 9.6× |
| quick-duplicates | 19.4 µs | 1.34 ms | 1.46 ms | 75× | 37.4 µs | 1.9× | 19× |
| json-aggregate | 848.1 µs | 21.98 ms | 22.66 ms | 27× | 1.76 ms | 2.1× | 0.47× |
| allocation-low-retention | 16.80 ms | 325 ms | 349 ms | 21× | 17.83 ms | 1.1× | 0.78× |
| allocation-high-retention | 21.67 ms | 440 ms | 471 ms | 22× | 23.18 ms | 1.1× | 1.0× |
| calls-empty | 0.065 µs | 22.6 µs | 46.8 µs | 720× | 0.025 µs | 0.39× | 11× |
| calls | 0.151 µs | 33.1 µs | 63.8 µs | 421× | 0.115 µs | 0.76× | 6.9× |
| json-hello | 0.064 µs | 29.0 µs | 55.5 µs | 870× | 0.249 µs | 3.9× | 13× |
| generate-sort | 273.9 µs | 4.39 ms | 4.45 ms | 16× | 814.6 µs | 3.0× | 8.4× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.
