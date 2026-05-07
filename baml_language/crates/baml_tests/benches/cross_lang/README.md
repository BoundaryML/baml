# Suite A2 — cross-language walltime benchmarks

This directory holds the corpus + harness for **Suite A2** of the BAML
benchmark suite. A2 measures the same workloads as `runtime_benchmark.rs`
(Suite A1, on CodSpeed), but as **subprocess walltime** so we can
compare against equivalent Python and Go programs apples-to-apples.

## Layout

```
cross_lang/
├── README.md                       (this file)
├── go.mod                          (module bamlbench/crosslang)
├── harness/
│   ├── run.sh                      hyperfine orchestrator
│   └── drift_check.py              .baml ↔ runtime_benchmark.rs sync check
├── workloads/
│   ├── baml/<16 .baml files>
│   ├── python/<16 .py files>
│   └── go/<15 .go files in package crosslang>     (fib_20_e2e shares Fib20)
└── cli/
    ├── baml/main.rs                wired as [[bin]] cross_lang_baml
    ├── python/run.py               python3 run.py <workload-file>
    └── go/main.go                  cross_lang_go <workload-name>
```

## What's measured

For each workload × language cell, hyperfine times the entire subprocess
from spawn to exit:

- **BAML**: `cross_lang_baml <path>` — compiles bytecode, builds engine,
  runs `main()`. Compile + execute, in-process Tokio runtime.
- **Python**: `python3 cli/python/run.py <path>` — interpreter startup +
  module import + `main()`.
- **Go**: `cross_lang_go <name>` — Go runtime startup + dispatch +
  function call.

This is **not** the same thing as Suite A1's CodSpeed numbers. A1 measures
execution-only inside the bench process (compile is amortized across
samples). A2 measures process startup + compile + execute. Don't put A1
and A2 on the same chart.

## Workloads

16 total. Names mirror `runtime_benchmark.rs` 1:1.

```
Pure VM (11):
    fib_20, loop_500k, string_concat_5k, array_push_50k, array_iter_10k,
    class_create_50k, field_access_50k, call_chain_100_x_5k,
    nested_loop, mixed_ops, closure_call_50k

E2E (5):
    hello_world, arithmetic, fib_20_e2e, class_and_loop, one_hundred_functions
```

The 3 BAML-specific compile-pipeline benches from A1
(`startup_empty_expression`, `compile_to_engine`, `engine_init_cost`) are
**deliberately excluded** — no fair Python/Go analog.

## Drift check

`runtime_benchmark.rs`'s `vm_<name>` and `e2e_<name>` benches contain the
authoritative BAML source as inline `r#"..."#` strings (or as helper
output for `call_chain_100_x_5k` and `one_hundred_functions`).
`drift_check.py` extracts those strings and asserts they match
`workloads/baml/<name>.baml` byte-for-byte. Run it before merging any
change to either side:

```sh
python3 cross_lang/harness/drift_check.py
```

## Running locally

```sh
# Quick sanity (1 warmup, 3 runs, just a few workloads):
cross_lang/harness/run.sh --warmup 1 --runs 3 --workloads fib_20,arithmetic
```

To post results to a running benchmark-results service:

```sh
BENCH_SERVICE_URL=http://localhost:8080 \
BENCH_SERVICE_TOKEN=devtoken \
BENCH_RUN_ID=<id-from-POST-/benchmark-runs> \
cross_lang/harness/run.sh --warmup 3 --runs 10
```

## Adding a workload

1. Add it to `runtime_benchmark.rs` first (don't bypass A1).
2. Add `workloads/baml/<name>.baml` matching the inline string.
3. Add `workloads/python/<name>.py` defining `main()`.
4. Add `workloads/go/<name>.go` in `package crosslang` exporting one fn,
   then wire it into `cli/go/main.go`'s dispatch map.
5. Add `<name>` to `DEFAULT_WORKLOADS` in `harness/run.sh`.
6. Add the mapping to `INLINE_WORKLOADS` (or `SYNTH_WORKLOADS`) in
   `harness/drift_check.py`.
7. `python3 cross_lang/harness/drift_check.py` should pass.
