// Whole-program profiling benchmark fixture.
//
// `baseline` and `stress` perform the same deterministic arithmetic. Baseline
// reaches each worker through a recursive call edge. Stress creates every
// worker future before awaiting any of them, so all workers are independently
// runnable and there is no nested spawn/await dependency chain.

class PackedProfilingResult {
    scenario: string,
    tasks: int,
    iterations: int,
    inner_rounds: int,
    kernel_calls: int,
    checksum: int,
}

function kernel(seed: int, iteration: int, inner_rounds: int) -> int throws never {
    let state = (seed + iteration * 97 + 1) % 2147483647;
    let round = 0;
    while (round < inner_rounds) {
        state = (state * 48271 + round + 1) % 2147483647;
        round += 1;
    }
    state
}

function worker(seed: int, iterations: int, inner_rounds: int) -> int throws never {
    let checksum = 0;
    let iteration = 0;
    while (iteration < iterations) {
        checksum = (checksum + kernel(seed, iteration, inner_rounds)) % 2147483647;
        iteration += 1;
    }
    checksum
}

function spawn_worker(
    seed: int,
    iterations: int,
    inner_rounds: int,
) -> baml.future.Future<int, never> throws never {
    spawn { worker(seed, iterations, inner_rounds) }
}

function baseline_chain(
    index: int,
    tasks: int,
    iterations: int,
    inner_rounds: int,
) -> int throws never {
    let current = worker(index + 1, iterations, inner_rounds);
    if (index + 1 < tasks) {
        (current + baseline_chain(index + 1, tasks, iterations, inner_rounds)) % 2147483647
    } else {
        current
    }
}

function main(
    scenario: string,
    tasks: int,
    iterations: int,
    inner_rounds: int,
) -> PackedProfilingResult throws never {
    let checksum = 0;
    if (scenario == "baseline") {
        checksum = baseline_chain(0, tasks, iterations, inner_rounds);
    } else {
        let futures: baml.future.Future<int, never>[] = [];
        let index = 0;
        while (index < tasks) {
            futures.push(spawn_worker(index + 1, iterations, inner_rounds));
            index += 1;
        }
        index = 0;
        while (index < tasks) {
            checksum = (checksum + (await futures[index])) % 2147483647;
            index += 1;
        }
    }
    PackedProfilingResult {
        scenario,
        tasks,
        iterations,
        inner_rounds,
        kernel_calls: tasks * iterations,
        checksum,
    }
}
