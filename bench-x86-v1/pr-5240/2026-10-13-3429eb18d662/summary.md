## Native bench `3429eb18d662` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 4.674 µs | 45.8 µs | 79.0 µs | 17× | 9.438 µs | 2.0× | 11× |
| array-empty | 0.085 µs | 29.0 µs | 56.1 µs | 662× | 0.006 µs | 0.07× | 9.5× |
| array-small | 0.150 µs | 89.7 µs | 125.1 µs | 832× | 0.093 µs | 0.62× | 12× |
| array-iterator | 4.013 µs | 3.78 ms | 4.44 ms | 1105× | 2.667 µs | 0.66× | 26× |
| array-indexed | 5.292 µs | 454.1 µs | 474.4 µs | 90× | 5.244 µs | 0.99× | 2.2× |
| array-build | 28.8 µs | 6.19 ms | 6.80 ms | 236× | 43.4 µs | 1.5× | 5.3× |
| merge-random | 57.7 µs | 2.70 ms | 2.83 ms | 49× | 88.3 µs | 1.5× | 5.7× |
| merge-sorted | 45.5 µs | 2.43 ms | 2.54 ms | 56× | 76.0 µs | 1.7× | 6.7× |
| merge-reverse | 45.8 µs | 2.48 ms | 2.60 ms | 57× | 77.0 µs | 1.7× | 6.8× |
| merge-duplicates | 50.1 µs | 2.62 ms | 2.76 ms | 55× | 67.7 µs | 1.4× | 6.1× |
| quick-random | 56.5 µs | 2.90 ms | 3.01 ms | 53× | 69.0 µs | 1.2× | 11× |
| quick-sorted | 49.1 µs | 2.52 ms | 2.66 ms | 54× | 64.4 µs | 1.3× | 9.8× |
| quick-reverse | 49.0 µs | 2.51 ms | 2.61 ms | 53× | 63.8 µs | 1.3× | 9.4× |
| quick-duplicates | 18.7 µs | 1.34 ms | 1.46 ms | 78× | 36.9 µs | 2.0× | 9.6× |
| json-aggregate | 856.7 µs | 22.16 ms | 23.16 ms | 27× | 1.79 ms | 2.1× | 0.47× |
| allocation-low-retention | 17.34 ms | 330 ms | 355 ms | 20× | 18.22 ms | 1.1× | 0.79× |
| allocation-high-retention | 21.71 ms | 445 ms | 482 ms | 22× | 24.00 ms | 1.1× | 1.1× |
| calls-empty | 0.068 µs | 22.3 µs | 46.6 µs | 682× | 0.025 µs | 0.37× | 11× |
| calls | 0.156 µs | 32.5 µs | 63.1 µs | 406× | 0.116 µs | 0.74× | 7.1× |
| json-hello | 0.066 µs | 29.0 µs | 55.1 µs | 838× | 0.247 µs | 3.8× | 12× |
| generate-sort | 277.8 µs | 4.45 ms | 4.39 ms | 16× | 814.6 µs | 2.9× | 8.3× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.
