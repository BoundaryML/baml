// obs-bench workload: fixed-wall call-rate sweep (design §10.2 `hotloop/bench_rate`).
//
// C4 sublinearity: CCT bytes/s must stay flat (±25%) across a ~100x call-rate
// sweep at constant wall time. The rate knob is `work`: unprofiled integer
// spinning *inlined in the leaf body* between profiled calls — the call count
// per iteration is constant (one `step` call), only the time per call grows,
// so the profiled-call rate falls as `work` rises. `work = 0` is the max-rate
// end; the harness picks `iters` per leg from a calibration pass (obs-bench
// `calibrate`) to hold wall time fixed across the sweep.
//
// Same runaway-path shape as hotloop.baml (main -> step) so byte comparisons
// isolate rate, not shape.

function step(acc: int, i: int, work: int) -> int {
    let spun = 0;
    let w = 0;
    while (w < work) {
        spun = (spun + w * 31) % 1000003;
        w += 1;
    }
    acc + spun + i
}

function main(iters: int, work: int) -> int {
    let acc = 0;
    let i = 0;
    while (i < iters) {
        acc = step(acc, i, work);
        i += 1;
    }
    acc
}
