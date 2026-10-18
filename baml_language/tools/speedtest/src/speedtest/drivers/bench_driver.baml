// speedtest driver, packed next to a workload's main.baml.
//
// One call verifies, `warmup - 1` more are untimed, then `jobs` calls are timed
// in-process, so the report excludes process start and engine load. BAML has no
// monotonic clock; on an idle machine the wall clock is adequate for runs of
// milliseconds and longer, and the parent records process wall time beside it.
class SpeedtestReport {
  result: string
  elapsed_ns: int
  jobs: int
}

function speedtest_driver(jobs: int, warmup: int) -> SpeedtestReport {
  let result = string.from(main());
  for (let i = 1; i < warmup; i += 1) {
    main();
  }
  let start = baml.time.Instant.now();
  for (let i = 0; i < jobs; i += 1) {
    main();
  }
  let elapsed = start.elapsed().to_nanoseconds();
  SpeedtestReport { result: result, elapsed_ns: int.parse(elapsed.to_string()), jobs: jobs }
}
