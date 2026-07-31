// obs-bench workload: agent-shaped concurrency (design §10.2 `agent/agent_like`).
//
// The multithreaded-rings workload every prior benchmark lacked, and the
// crashfuzz target. Shape (matches the research corpus's working-agent trace):
//   - 86 logical threads total: main + 5 pipeline stages x 17 spawned workers.
//   - call depth 14 on the worker path (main -> run_stage -> spawn closure ->
//     work -> l05 .. l14).
//   - sysops + awaits: every worker parks on `baml.sys.sleep` (the profiled
//     sysop window), so records migrate across VM rings at await points —
//     exercising the §5.2 causal defer and the §5.3 Suspend/Resume accounting.
//   - caught errors: ~1/5 of leaf calls throw; workers catch and continue, so
//     ends_err counters and error-status recent-ring slots are exercised
//     without failing the run.
//
// Deterministic: fixed thread/depth structure, error pattern keyed on loop
// indexes, no wall-clock dependence beyond the sleeps. `rounds` scales work
// per worker; `sleep_ms` scales park time (0 = never parks, for producer-rate
// legs; 1+ = real awaits, for migration/accounting legs).

function l14(x: int) -> int {
    if (x % 5 == 3) {
        throw "leaf_err";
    }
    x + 1
}

function l13(x: int) -> int {
    l14(x) + 1
}

function l12(x: int) -> int {
    l13(x) + 1
}

function l11(x: int) -> int {
    l12(x) + 1
}

function l10(x: int) -> int {
    l11(x) + 1
}

function l09(x: int) -> int {
    l10(x) + 1
}

function l08(x: int) -> int {
    l09(x) + 1
}

function l07(x: int) -> int {
    l08(x) + 1
}

function l06(x: int) -> int {
    l07(x) + 1
}

function l05(x: int) -> int {
    l06(x) + 1
}

// Worker body: each round parks once (when sleep_ms > 0), then descends
// l05..l14, catching the deterministic leaf errors.
function work(id: int, rounds: int, sleep_ms: int) -> int {
    let acc = 0;
    let r = 0;
    while (r < rounds) {
        if (sleep_ms > 0) {
            // `0n + sleep_ms` lifts the int knob to the bigint the Duration
            // API takes (mixed bigint/int operands are allowed).
            baml.sys.sleep(baml.time.Duration.from_milliseconds(0n + sleep_ms));
        }
        acc
            += (
                l05(id * 31 + r) catch (e) {
                    "leaf_err" => 0,
                    _ => 0
                }
            );
        r += 1;
    }
    acc
}

function run_stage(stage: int, rounds: int, sleep_ms: int) -> int {
    // `baml.sys.sleep` can throw `baml.errors.Io`, so the spawn futures carry
    // that error type; an Io throw would fail the run loudly, which is right
    // for a benchmark (sleeps failing means the leg is invalid).
    let futures: baml.future.Future<int, baml.errors.Io>[] = [];
    let w = 0;
    while (w < 17) {
        let id = stage * 100 + w;
        futures.push(spawn { work(id, rounds, sleep_ms) });
        w += 1;
    }
    let total = 0;
    for (let f in futures) {
        total += await f;
    }
    total
}

function main(rounds: int, sleep_ms: int) -> int {
    let total = 0;
    let stage = 0;
    while (stage < 5) {
        total += run_stage(stage, rounds, sleep_ms);
        stage += 1;
    }
    total
}
