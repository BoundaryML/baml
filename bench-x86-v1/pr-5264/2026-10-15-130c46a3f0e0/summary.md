## Native bench `130c46a3f0e0` on bench-x86-v1

| scenario | native · off | VM · off | VM · medium | VM medium / native | Go | Go / native | Node / native |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 58.0 µs | 65.5 µs | 89.0 µs | 1.5× | 4.098 µs | 0.07× | 0.90× |
| array-empty | 22.9 µs | 29.8 µs | 56.5 µs | 2.5× | 0.007 µs | 0.00× | 0.04× |
| array-small | 24.0 µs | 89.1 µs | 127.5 µs | 5.3× | 0.097 µs | 0.00× | 0.08× |
| array-iterator | 114.6 µs | 3.72 ms | 4.41 ms | 39× | 2.872 µs | 0.03× | 0.94× |
| array-indexed | 114.6 µs | 427.9 µs | 457.6 µs | 4.0× | 5.350 µs | 0.05× | 0.10× |
| array-build | 553.2 µs | 6.04 ms | 6.69 ms | 12× | 45.0 µs | 0.08× | 0.28× |
| merge-random | 750.1 µs | 2.57 ms | 2.71 ms | 3.6× | 115.1 µs | 0.15× | 0.47× |
| merge-sorted | 624.4 µs | 2.34 ms | 2.45 ms | 3.9× | 85.4 µs | 0.14× | 0.51× |
| merge-reverse | 656.8 µs | 2.39 ms | 2.49 ms | 3.8× | 95.9 µs | 0.15× | 0.48× |
| merge-duplicates | 706.0 µs | 2.54 ms | 2.68 ms | 3.8× | 76.1 µs | 0.11× | 0.47× |
| quick-random | 882.3 µs | 2.77 ms | 2.90 ms | 3.3× | 76.8 µs | 0.09× | 0.71× |
| quick-sorted | 762.0 µs | 2.43 ms | 2.56 ms | 3.4× | 82.6 µs | 0.11× | 0.63× |
| quick-reverse | 758.2 µs | 2.42 ms | 2.51 ms | 3.3× | 79.5 µs | 0.10× | 0.63× |
| quick-duplicates | 421.1 µs | 1.31 ms | 1.44 ms | 3.4× | 42.6 µs | 0.10× | 0.93× |
| json-aggregate | 22.97 ms | 22.90 ms | 23.81 ms | 1.0× | 1.83 ms | 0.08× | 0.02× |
| allocation-low-retention | 81.82 ms | 324 ms | 348 ms | 4.3× | 17.94 ms | 0.22× | 0.18× |
| allocation-high-retention | 122 ms | 444 ms | 478 ms | 3.9× | 24.45 ms | 0.20× | 0.20× |
| calls-empty | 22.0 µs | 23.0 µs | 47.2 µs | 2.1× | 0.030 µs | 0.00× | 0.03× |
| calls | 24.8 µs | 33.9 µs | 64.1 µs | 2.6× | 0.145 µs | 0.01× | 0.04× |
| json-hello | 31.2 µs | 30.2 µs | 58.6 µs | 1.9× | 0.269 µs | 0.01× | 0.03× |
| generate-sort | 4.53 ms | 4.51 ms | 4.47 ms | 0.99× | 849.3 µs | 0.19× | 0.51× |

Per-job wall time, median of the trials; every BAML row names its telemetry level. Ratios above 1 mean native is faster.

Against `45d638e2d4db`: **16 faster, 0 slower, 5 unchanged** (net of control drift, threshold 5%).
- array-empty: 29.9 µs → 22.9 µs (-23.4%, control +6.0%) **faster**
- array-small: 87.6 µs → 24.0 µs (-72.6%, control -2.0%) **faster**
- array-iterator: 3.73 ms → 114.6 µs (-96.9%, control +19.5%) **faster**
- array-indexed: 429.4 µs → 114.6 µs (-73.3%, control +0.6%) **faster**
- array-build: 5.98 ms → 553.2 µs (-90.8%, control -1.6%) **faster**
- merge-random: 2.55 ms → 750.1 µs (-70.6%, control -7.5%) **faster**
- merge-sorted: 2.30 ms → 624.4 µs (-72.9%, control -5.6%) **faster**
- merge-reverse: 2.35 ms → 656.8 µs (-72.1%, control +3.2%) **faster**
- merge-duplicates: 2.50 ms → 706.0 µs (-71.7%, control +14.5%) **faster**
- quick-random: 2.76 ms → 882.3 µs (-68.0%, control -4.5%) **faster**
- quick-sorted: 2.39 ms → 762.0 µs (-68.1%, control +10.2%) **faster**
- quick-reverse: 2.37 ms → 758.2 µs (-68.0%, control -0.7%) **faster**
- quick-duplicates: 1.30 ms → 421.1 µs (-67.7%, control +2.7%) **faster**
- allocation-low-retention: 329 ms → 81.82 ms (-75.1%, control +0.2%) **faster**
- allocation-high-retention: 441 ms → 122 ms (-72.4%, control +8.2%) **faster**
- calls: 38.2 µs → 24.8 µs (-35.1%, control +10.4%) **faster**
