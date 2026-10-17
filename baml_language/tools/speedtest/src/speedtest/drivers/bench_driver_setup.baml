// speedtest driver for workloads that define `setup()` and `run(state)`.
//
// `setup()` runs once, untimed (for example building a large live graph); one
// `run(state)` call verifies, `warmup - 1` more are untimed, then `jobs` calls
// are timed in-process. See bench_driver.baml for the clock caveat.
class SpeedtestReport {
  result: string
  elapsed_ns: int
  jobs: int
}

function speedtest_driver(jobs: int, warmup: int) -> SpeedtestReport {
  let state = setup();
  let result = string.from(run(state));
  for (let i = 1; i < warmup; i += 1) {
    run(state);
  }
  let start = baml.time.Instant.now();
  for (let i = 0; i < jobs; i += 1) {
    run(state);
  }
  let elapsed = start.elapsed().to_nanoseconds();
  SpeedtestReport { result: result, elapsed_ns: int.parse(elapsed.to_string()), jobs: jobs }
}
