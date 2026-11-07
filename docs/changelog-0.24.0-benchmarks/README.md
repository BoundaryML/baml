# BAML 0.24.0 benchmark evidence

This directory preserves the measurements quoted in [the release performance audit](../changelog-0.24.0-performance.md). The original measurements ran on 2026-11-05 using published 0.23.0 and 0.24.0 macOS arm64 compiler/CFFI artifacts and matching Python wheels. Their commits, artifact hashes and sizes, hardware, Python version, build profile, and evidence-file hashes are recorded in [environment.json](environment.json). These are bytecode interpreter and bridge measurements, not native Rust backend measurements or provider latency measurements.

[benchmark-results.json](benchmark-results.json) contains all five interleaved pairs for strings, enum comparisons, n-body, and Python call overhead. [array-results.json](array-results.json) contains the separate five-pair array followup. [summary.json](summary.json) stores unrounded medians, ranges, changes, and checksums calculated from those observations. Original results are evidence and must not be overwritten by fresh measurements.

The three scripts in [original](original/) are byte-for-byte copies of the scripts that ran: [bench.py](original/bench.py), [run_benchmarks.py](original/run_benchmarks.py), and [bench_array.py](original/bench_array.py). The original suite orchestrator contains historical absolute temporary paths. The array orchestrator was an inline shell-tool Python invocation; there was no additional original runner file. Its behavior was five alternating release pairs, invoking the preserved per-process array script, with the same environment as the suite. The new [replay.py](replay.py) supplies that orchestration portably. It was written after measurement and did not produce the preserved observations.

The exact BAML inputs are [main.baml](inputs/baml_src/main.baml), [bench.baml](inputs/baml_src/bench.baml), and [array.baml](inputs/baml_src/array.baml). The initial suite compiled `main.baml` and `bench.baml`; `array.baml` was added and clients regenerated for the later array measurement. The shared benchmark source hash recorded in the audit is the SHA-256 of `bench.baml`. The auxiliary `main.baml` was also present during measurement and is preserved rather than silently omitted. [023.toml](inputs/config/023.toml) and [024.toml](inputs/config/024.toml) preserve the actual generator configuration. They explicitly expose `BAML_TELEMETRY` to the generated SDK so `BAML_TELEMETRY=off` disables recording. They contain no credentials.

## Validate without running benchmarks

Run from any directory:

```bash
python3 /path/to/baml/docs/changelog-0.24.0-benchmarks/replay.py validate
```

This checks every preserved script/input/result hash, verifies matching kernel results across releases and trials, recalculates the complete summary, compares it exactly with `summary.json`, and prints rounded values matching the audit. It does not load BAML, install packages, or rerun timings.

## Reproduce with isolated environments

Use Python 3.12.0 for the closest comparison. Create a separate virtual environment for each version. Install `baml-bridge==0.23.0` and `pydantic>=2` in the lower environment; the lower bridge did not declare its Pydantic dependency. Install `baml-bridge==0.24.0` in the upper environment; it installs Pydantic itself. The measured environments resolved Pydantic 2.14.3, pydantic-core 2.50.4, protobuf 7.36.2, typing-extensions 4.16.0, typing-inspection 0.4.5, and annotated-types 0.8.0. Pin these versions too when reproducing the historical environment. See `environment.json` for the native artifact fingerprints.

Download the `baml-language-0.23.0-aarch64-apple-darwin.tar.gz` and `baml-language-0.24.0-aarch64-apple-darwin.tar.gz` assets from their corresponding GitHub release tags. Each contains `bin/baml-cli`. Download each release's `libbaml_cffi-aarch64-apple-darwin.dylib` too. Keep compiler, generated SDK, wheel, and CFFI library versions aligned. These artifacts are available under `https://github.com/BoundaryML/baml/releases/tag/baml-language-<version>`. This replay does not depend on the moving canary manifest.

Create four project directories: lower-suite, upper-suite, lower-array, and upper-array. In each, place the appropriate preserved TOML file at `baml.toml` and create `baml_src`. Suite projects receive only `main.baml` and `bench.baml`; array projects receive all three preserved BAML files. Generate each with its matching published compiler. For example:

```bash
BOUNDARY_API_KEY=local BAML_TELEMETRY=off /path/to/0.23.0/bin/baml-cli generate --project /path/to/lower-suite
BOUNDARY_API_KEY=local BAML_TELEMETRY=off /path/to/0.24.0/bin/baml-cli generate --project /path/to/upper-suite
BOUNDARY_API_KEY=local BAML_TELEMETRY=off /path/to/0.23.0/bin/baml-cli generate --project /path/to/lower-array
BOUNDARY_API_KEY=local BAML_TELEMETRY=off /path/to/0.24.0/bin/baml-cli generate --project /path/to/upper-array
```

The upper config also generates Rust, TypeScript, and Go clients, as it did originally. Only its Python output is measured. Do not add `--embed-telemetry`. The benchmark uses pure computation, makes no provider calls, and needs no secret credentials.

Run the portable orchestrator with explicit paths. SDK arguments name the Python import root containing `baml_sdk`, normally the project's `python` directory. The output directory must not already exist:

```bash
python3 /path/to/baml/docs/changelog-0.24.0-benchmarks/replay.py replay \
  --lower-python /path/to/lower-venv/bin/python \
  --upper-python /path/to/upper-venv/bin/python \
  --lower-sdk /path/to/lower-suite/python \
  --upper-sdk /path/to/upper-suite/python \
  --lower-array-sdk /path/to/lower-array/python \
  --upper-array-sdk /path/to/upper-array/python \
  --lower-library /path/to/0.23.0/libbaml_cffi-aarch64-apple-darwin.dylib \
  --upper-library /path/to/0.24.0/libbaml_cffi-aarch64-apple-darwin.dylib \
  --output-dir /path/to/new-results \
  --trials 5
```

The portable runner invokes the unchanged original per-process scripts, alternates release order, sets `BAML_TELEMETRY=off` and `BOUNDARY_API_KEY=local`, gives each release an isolated `BAML_HOME`, checks cross-release results, and writes fresh raw JSON and a summary into the new output directory. Supplying separate suite and array SDKs preserves the two historical source inventories. If array SDK flags are omitted, replay uses the supplied suite SDKs for both measurements, so those SDKs must include the array function; that convenience mode has the final combined source inventory rather than the initial suite inventory.

Fresh timings will vary with hardware, Python, scheduling, and machine load. The original machine was not CPU-pinned and also ran changelog preparation. Preserve the full ranges and control rows: the sync no-op median was slower, and sequential async ranges overlapped. Do not replace these observations with a claim that every application or Python call gets faster.
