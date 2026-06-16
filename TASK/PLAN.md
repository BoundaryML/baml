# Implement Default Testing Runners

## Summary

Add the BEP default runners as BAML stdlib functions: `testing.Quorum`, `testing.Retry`, `testing.PassRate`, `testing.Sequential`, and `testing.FailFast`.

I reproduced the reported warning on canary: `catch_all (e) { _ => ... }` around a `testing.TestReportThunk` still emits `warning: unreachable arm`. I also found a larger current test plumbing issue: a failing `assert.is_true(false)` escapes as `baml.panics.UserPanic` instead of becoming `TestReport { outcome: "fail" }`. The plan fixes testing behavior by catching `baml.panics.Panic` explicitly in BAML, without suppressing the TIR warning globally.

## Key Changes

- Change `baml_language/crates/baml_builtins2/baml_std/testing/types.baml`:
  - Add `type ChildReportThunk = () -> ChildReport throws never`.
  - Add `class TestSetChild { name string, run ChildReportThunk }`.
  - Change `TestSetRunner` from wrapping an opaque `TestSetReportThunk` to receiving `TestSetChild[]` and returning `TestSetReport`.
  - Keep `TestSetReportThunk` only if needed for compatibility in snapshots, but stop using it for built-in testset runners.

- Change `baml_language/crates/baml_builtins2/baml_std/testing/registry.baml`:
  - Update `run_test` so the base run catches both typed throws and assertion/user panics:
    - First arm: `baml.panics.Panic => RunReport { outcome: "fail", ... }`
    - Fallback arm: `_ => RunReport { outcome: "fail", ... }`
  - Add BAML helpers to aggregate `ChildReport[]` into `TestSetReport`.
  - Add `TestRegistry.run_testset(name)` and/or `run_all()` that builds `TestSetChild` thunks for child tests and nested testsets, then applies the stored `TestSetRunner`.
  - Change `run_testset(children, runner)` to call the runner when present, otherwise use default parallel execution.

- Add `baml_language/crates/baml_builtins2/baml_std/testing/runners.baml`:
  - `Quorum(n, m) -> TestRunner`: run the base thunk `n` times, append all run records, pass if at least `m` reports pass.
  - `Retry(max_attempts) -> TestRunner`: stop on first passing report; if all fail, return an aggregate failed report with all attempts’ runs.
  - `PassRate(threshold) -> TestSetRunner`: run all children, then pass if `passed / total >= threshold`; empty sets count as pass.
  - `Sequential() -> TestSetRunner`: run child thunks one by one.
  - `FailFast() -> TestSetRunner`: run child thunks one by one and stop after the first non-pass report.
  - Validate runner constructor arguments with `baml.sys.panic`: `n > 0`, `0 <= m <= n`, `max_attempts > 0`, `0.0 <= threshold <= 1.0`.

- Change `baml_language/crates/baml_builtins2/src/lib.rs`:
  - Register the new `testing/runners.baml` builtin file after `types.baml` and `registry.baml` so the functions appear in `testing.*`.

- Change `baml_language/crates/baml_cli/src/test_command.rs`:
  - For unfiltered new-style tests, call the registry aggregate runner path so testset runners like `PassRate`, `Sequential`, and `FailFast` are honored.
  - Keep filtered leaf execution for partial runs; partial filtered runs should not apply parent testset aggregate runners because the suite is incomplete.
  - Extract both `TestReport` and `TestSetReport` outcomes when printing pass/fail summaries.

## Tests

- Update stdlib snapshots:
  - `__testing_std__` snapshots should show the new runner functions and changed `TestSetRunner` type.
  - `baml_cli__describe_command_tests__render_testing_package_listing.snap` should list `Quorum`, `Retry`, `PassRate`, `Sequential`, and `FailFast`.

- Update existing diagnostics:
  - Move `test_expr_with_runner` and the `testing.Quorum(5, 3)` case in `test_with_runner_ambiguity` from unresolved-name expectations to compile/pass expectations or adjust fixtures accordingly.

- Add engine tests in `baml_language/crates/bex_engine/tests/collect_tests.rs`:
  - `assert.is_true(false)` becomes a failed `TestReport`, not an unhandled panic.
  - `Quorum(5, 3)` passes with 3 successes and fails with fewer.
  - `Retry(3)` stops after first success.
  - `PassRate(0.7)` passes/fails based on child outcomes.
  - `FailFast()` does not execute children after the first failure.
  - `Sequential()` executes children in source order.

- Add CLI coverage:
  - `baml test --from <tmp project>` honors testset-level `PassRate`.
  - Filtered runs still execute matching leaf tests without applying parent aggregate runners.

## Assumptions

- Default runner implementations must be BAML; Rust changes are limited to exposing/executing the BAML runner model.
- The reproduced `catch_all` wildcard warning is not suppressed in TIR. The stdlib will explicitly match `baml.panics.Panic` where panic-catching behavior is intended.
- `FailFast` reports only executed children in `results`; `total` is the executed count because `TestSetReport` has no skipped field today.

