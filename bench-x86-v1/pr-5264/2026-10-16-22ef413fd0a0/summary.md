## Native bench `22ef413fd0a0` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 63.5 µs | 65.8 µs | 88.7 µs | 1.4× | 4.122 µs | 0.06× | 0.81× |
| array-empty | 24.1 µs | 32.1 µs | 60.6 µs | 2.5× | 0.008 µs | 0.00× | 0.04× |
| array-small | 24.6 µs | 90.3 µs | 127.1 µs | 5.2× | 0.097 µs | 0.00× | 0.08× |
| array-iterator | 128.1 µs | 4.02 ms | 4.73 ms | 37× | 3.485 µs | 0.03× | 0.88× |
| array-indexed | 126.9 µs | 457.6 µs | 489.5 µs | 3.9× | 6.255 µs | 0.05× | 0.10× |
| array-build | 560.1 µs | 6.02 ms | 6.54 ms | 12× | 47.5 µs | 0.08× | 0.27× |
| merge-random | 704.9 µs | 2.56 ms | 2.67 ms | 3.8× | 108.9 µs | 0.15× | 0.49× |
| merge-sorted | 601.8 µs | 2.31 ms | 2.45 ms | 4.1× | 80.5 µs | 0.13× | 0.53× |
| merge-reverse | 609.3 µs | 2.39 ms | 2.49 ms | 4.1× | 97.5 µs | 0.16× | 0.51× |
| merge-duplicates | 684.1 µs | 2.50 ms | 2.65 ms | 3.9× | 77.6 µs | 0.11× | 0.48× |
| quick-random | 844.8 µs | 2.74 ms | 2.85 ms | 3.4× | 88.2 µs | 0.10× | 0.74× |
| quick-sorted | 714.6 µs | 2.38 ms | 2.51 ms | 3.5× | 81.9 µs | 0.11× | 0.72× |
| quick-reverse | 720.7 µs | 2.36 ms | 2.49 ms | 3.4× | 67.8 µs | 0.09× | 0.69× |
| quick-duplicates | 408.7 µs | 1.30 ms | 1.42 ms | 3.5× | 47.2 µs | 0.12× | 0.94× |
| json-aggregate | 7.67 ms | 7.72 ms | 7.80 ms | 1.0× | 1.80 ms | 0.23× | 0.05× |
| allocation-low-retention | 83.22 ms | 350 ms | 374 ms | 4.5× | 18.83 ms | 0.23× | 0.18× |
| allocation-high-retention | 84.70 ms | 451 ms | 481 ms | 5.7× | 23.67 ms | 0.28× | 0.27× |
| calls-empty | 24.0 µs | 24.7 µs | 50.4 µs | 2.1× | 0.035 µs | 0.00× | 0.03× |
| calls | 26.7 µs | 36.0 µs | 70.0 µs | 2.6× | 0.152 µs | 0.01× | 0.04× |
| json-hello | 30.3 µs | 30.0 µs | 56.6 µs | 1.9× | 0.299 µs | 0.01× | 0.03× |
| generate-sort | 4.23 ms | 4.30 ms | 4.37 ms | 1.0× | 864.8 µs | 0.20× | 0.55× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.
