## Native bench `afde63a65964` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 3.567 µs | 46.7 µs | 70.7 µs | 20× | 11.8 µs | 3.3× | 14× |
| array-empty | 0.088 µs | 29.2 µs | 56.2 µs | 642× | 0.007 µs | 0.08× | 9.2× |
| array-small | 0.150 µs | 90.0 µs | 126.4 µs | 842× | 0.094 µs | 0.62× | 12× |
| array-iterator | 3.998 µs | 3.79 ms | 4.41 ms | 1102× | 2.701 µs | 0.68× | 26× |
| array-indexed | 5.423 µs | 455.2 µs | 479.3 µs | 88× | 5.262 µs | 0.97× | 2.2× |
| array-build | 28.8 µs | 6.22 ms | 6.76 ms | 235× | 41.1 µs | 1.4× | 5.4× |
| merge-random | 57.5 µs | 2.70 ms | 2.87 ms | 50× | 88.1 µs | 1.5× | 5.7× |
| merge-sorted | 46.8 µs | 2.44 ms | 2.57 ms | 55× | 75.2 µs | 1.6× | 6.5× |
| merge-reverse | 47.2 µs | 2.50 ms | 2.62 ms | 55× | 76.4 µs | 1.6× | 6.6× |
| merge-duplicates | 51.7 µs | 2.68 ms | 2.78 ms | 54× | 68.0 µs | 1.3× | 6.0× |
| quick-random | 55.4 µs | 2.90 ms | 3.02 ms | 55× | 71.2 µs | 1.3× | 11× |
| quick-sorted | 47.9 µs | 2.53 ms | 2.66 ms | 56× | 63.1 µs | 1.3× | 10× |
| quick-reverse | 46.9 µs | 2.53 ms | 2.64 ms | 56× | 63.5 µs | 1.4× | 10× |
| quick-duplicates | 18.8 µs | 1.35 ms | 1.47 ms | 78× | 37.5 µs | 2.0× | 21× |
| json-aggregate | 888.3 µs | 22.10 ms | 23.03 ms | 26× | 1.79 ms | 2.0× | 0.45× |
| allocation-low-retention | 17.07 ms | 331 ms | 355 ms | 21× | 17.94 ms | 1.1× | 0.83× |
| allocation-high-retention | 21.58 ms | 449 ms | 484 ms | 22× | 23.63 ms | 1.1× | 1.1× |
| calls-empty | 0.069 µs | 22.8 µs | 46.6 µs | 677× | 0.026 µs | 0.38× | 10× |
| calls | 0.154 µs | 33.4 µs | 63.5 µs | 413× | 0.114 µs | 0.74× | 6.9× |
| json-hello | 0.063 µs | 28.8 µs | 55.5 µs | 875× | 0.248 µs | 3.9× | 13× |
| generate-sort | 282.7 µs | 4.42 ms | 4.42 ms | 16× | 815.3 µs | 2.9× | 8.2× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.
