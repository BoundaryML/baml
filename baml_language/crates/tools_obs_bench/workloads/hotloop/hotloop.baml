// obs-bench workload: the accidental hot loop (design §10.2 `hotloop/`).
//
// Reproduces the measured pathology: a tight loop of trivial profiled calls
// whose calling contexts collapse to a 3-node CCT (main -> step -> add) while
// the legacy pipeline writes one disk event per call. `iters` iterations make
// 2 profiled calls each, so total calls = 2 * iters:
//
//   calls  500k -> iters   250000
//   calls    5M -> iters  2500000    (legacy anchor: 226,088,292 bytes, 45.2 B/call)
//   calls   15M -> iters  7500000
//   calls 36.2M -> iters 18100000    (the 1.69 GB / 3.8 s artifact's call count)
//
// The body is integer arithmetic only — no allocation, no sysops — so the
// producer hot path dominates and consumer/bytes measurements are pure
// pipeline cost. C3's hot-loop gate (<= 6 KB/s CCT bytes) and C1/C2 ratios
// run over this file. Keep the shape stable: benchmarks cite it by name.

function add(x: int, y: int) -> int {
    x + y
}

function step(acc: int, i: int) -> int {
    add(acc, i)
}

function main(iters: int) -> int {
    let acc = 0;
    let i = 0;
    while (i < iters) {
        acc = step(acc, i);
        i += 1;
    }
    acc
}
