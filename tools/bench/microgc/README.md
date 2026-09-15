# Pure-BAML micro-GC benchmarks

This harness runs allocation workloads inside a packed BAML program without an HTTP server or a Python/Node bridge. It samples the diagnostic `baml.sys.heap_stats()` hook and native macOS RSS so container payload allocation can be compared with scalar-only execution.

The workloads are generated deterministically from `workloads.py`:

| Workload | Allocation under test |
| --- | --- |
| `unused-scalars-3` | The original `let a = 0.05; let b = 0.07; let c = 2.17;` loop |
| `checksummed-scalars-3` | The same three bindings consumed by floating-point arithmetic |
| `float-array-1000` | A new 1,000-element float array on every iteration |
| `float-map-1000` | A new map populated by 1,000 hardcoded float insertions on every iteration |
| `hardcoded-scalars-10000` | 10,000 distinct unused float bindings directly inside every hot-loop iteration, with no selector, array, or map |

The generated projects live under ignored `.build/apps/`. Each result directory retains the exact generated `main.baml`, `baml.toml`, CLI identity and hashes, executable hash, raw samples, logs, configuration, and summary. Keeping the large literal workloads generated avoids checking in more than 11,000 repetitive BAML lines while preserving exact reproduction evidence.

## Diagnostic build

These benchmarks require a BAML CLI built with the read-only `baml.sys.heap_stats()` diagnostic used by the longevity investigation. The normal CLI will reject the generated sources if it does not expose `baml.sys.HeapStats` and `baml.sys.heap_stats()`.

The sampled hook must expose runtime slot counts, GC allocation pressure, weak permit registrations, and these macOS malloc statistics: `allocator_blocks_in_use`, `allocator_bytes_in_use`, `allocator_bytes_high_water`, and `allocator_bytes_reserved`.

## Generate and inspect source

```sh
cd tools/bench/microgc
python3 generate.py
python3 generate.py hardcoded-scalars-10000 float-map-1000
```

## Run

Stop unrelated memory-heavy workloads before comparing runs. The harness terminates the child after the requested duration or RSS safety limit and writes results only beneath `.build/` unless `--output` is supplied.

```sh
python3 run.py unused-scalars-3 --baml-cli /path/to/diagnostic/baml-cli --duration-seconds 30
python3 run.py float-array-1000 --baml-cli /path/to/diagnostic/baml-cli --duration-seconds 30
python3 run.py float-map-1000 --baml-cli /path/to/diagnostic/baml-cli --duration-seconds 30 --rss-limit-mib 2048
python3 run.py hardcoded-scalars-10000 --baml-cli /path/to/diagnostic/baml-cli --duration-seconds 30
```

`iterations` counts calls to the allocation body, not individual scalar bindings or container elements. The batch size differs by workload so diagnostic formatting does not dominate the hot loop.

## Plot

Plot one result in place or compare several result directories as columns:

```sh
uv run --with matplotlib python plot.py .build/results/<result>
uv run --with matplotlib python plot.py .build/results/<map-result> .build/results/<scalar-result> --output .build/map-vs-scalars.png
```

The chart separates RSS, malloc live bytes, malloc reserved bytes, runtime slots, GC allocation pressure, malloc blocks, retained BAML slot capacity, and weak permit registrations. Columns use independent y-axis scales because the map workload can consume orders of magnitude more memory than the scalar control.

## Verify the generators

```sh
python3 -m unittest test_workloads.py
```
