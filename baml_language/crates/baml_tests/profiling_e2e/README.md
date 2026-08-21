# Packed profiling end-to-end benchmark

This fixture measures a release-packed standalone BAML executable. The timed
process does not include `baml-cli`, Cargo, the test harness, or an interpreter
driver. Profiling-off and profiling-on commands differ only in `BAML_PROFILE`.

From `baml_language/`:

```sh
python3 crates/baml_tests/profiling_e2e/run.py
```

The runner builds these release artifacts before packing:

```sh
BAML_PROFILE=0 cargo build --release \
  -p baml_cli --bin baml-cli \
  -p baml_pack_host --bin baml-pack-host \
  -p baml_tests --example profiling_e2e_verify
BAML_PROFILE=0 BAML_CLI_ALLOW_DIRECT=1 target/release/baml-cli pack \
  --file crates/baml_tests/profiling_e2e/workload.baml main \
  --output target/profiling-e2e/profiling-e2e-packed
```

Defaults use three warmups and fifteen measured baseline runs, then two
warmups and ten measured saturation runs. Saturation spawns twice the logical
CPU count reported by Python. `--no-build` reuses the release-packed binary.
`--profile-mode off|on` can isolate one side after a fail-fast paired run, and
`--timeout-seconds` bounds a suspected stall. `--output` preserves separate
JSON records. The normal reproducible check remains the default paired run.

The runner verifies byte-identical output and reads every profiling-on durable
run with `profiling_e2e_verify`. A packed invocation produces exactly one
durable run, the workload boundary; the JSON argument/output helpers around it
are suppressed internal roots and publish nothing.
The verifier rejects incomplete runs, CCT shape/count
changes, evidence mismatches, any health/loss counter, or profiler state in an
off run. Stress call/context counts are exact; its await count has exact
semantic bounds because futures that have already completed do not suspend and
therefore correctly add no await interval. The runner also records
time-to-first-output separately from the remaining shutdown/consumer-flush
tail.

Hyperfine is the preferred whole-process timing tool. If it is absent, the
runner reports that fact and uses a direct `perf_counter_ns` repeated-process
fallback with alternating off/on order and identical captured output handling.
The initial 2026-08-19 record used that fallback because Hyperfine was absent.
After Hyperfine 1.20.0 was installed, final measurements used `--output=pipe`,
three/two warmups, and fifteen/ten measured runs. Exact commands, results, and
the earlier failed attempts are recorded in the PR description; the direct
runner remains the per-run output and durable-invariant checker.

Peak RSS is a separate diagnostic so it cannot perturb primary wall timing.
`rss_probe.py` launches exactly one packed child and reports that child's
`ru_maxrss` in normalized bytes. For example:

```sh
python3 crates/baml_tests/profiling_e2e/rss_probe.py \
  --cwd target/profiling-e2e/work --env BAML_PROFILE=1 -- \
  target/profiling-e2e/profiling-e2e-packed \
  --scenario baseline --tasks 1 --iterations 50000 --inner_rounds 640
```

On/off RSS probes must use fresh probe processes and cleaned profile stores.
The reported delta is whole-process incremental peak RSS, not an allocator-
exact profiler-only high-water mark.
