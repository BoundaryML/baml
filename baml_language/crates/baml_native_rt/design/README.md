# Design notes: mechanisms and what they cost

Each document describes one part of the native backend as it is built:
how it works, the code that does it, a BAML example, the arm64 that comes
out, and the measured cost next to Go and, where it means something, Node.

| document | covers |
|---|---|
| `calls.md` | The calling convention and pinned register, direct calls, interface dispatch through the method table, function values. |
| `natives.md` | Plain, `allocates` and `async` natives, the anchor stub, the compiled driver stub and the poll path, `scope.call_baml`, and the chain through `scope.io` to Tokio and back. |
| `memory.md` | Object headers and layouts with their sizes, inline allocation and its slow path, field and index access, maps, string concatenation. |
| `gc.md` | The copying collector, root finding, stack maps and their decode, cost per collection, per live object, per byte and per frame. |
| `control.md` | Loop polls, the entry stack check, stack sizing, throw and catch, the cost of a handler that is not used. |
| `tasks.md` | The task record and switch frame, `spawn`, `await`, parking on timers, futures and I/O, the wake from the I/O thread. |
| `threads.md` | Which OS threads exist, per-worker state, what migration stress checks, and the list of everything that assumes one worker, with file and line. |
| `startup-and-size.md` | Instructions before the first line of user code, where they go, and binary sizes. |

## How the numbers were measured

Machine: Apple M2 Max, macOS 26.7.1, arm64. Commit `39d3ad4c25`, branch
`claude/baml-native-backend`. Go 1.26.0 (`go build`, default flags).
Node v22.23.4. The compiler is the `native_pack` example built with
`--profile fasttest`; the runtime archive it links is always built
`opt-level=3`, `codegen-units=1`, `panic=abort`, whatever the compiler's
profile (`baml_compiler2_native/build.rs`).

Three methods. Each number says which it came from.

**Counted.** A probe is a small BAML program built to an executable at
two iteration counts. Instructions are `instructions retired` from
`/usr/bin/time -l`, the fewest of 7 runs. Cost per operation is the
difference of the two counts over the difference in iterations. The
documents give both raw counts. Where a probe needs a loop around the
thing measured, a second probe with the same loop and a trivial body is
subtracted, and both are shown. Each loop was disassembled to check that
the operation is still there: MIR inlining, scalar replacement and
devirtualization delete naive probes (a small callee is inlined, an
object that does not escape is never allocated, a call on a known class
is direct).

**Traced.** For a breakdown by function, one iteration is single-stepped
under `lldb` from a breakpoint and the instructions are tallied by
symbol. These are user-mode instructions of one thread. They agree with
the counted totals where both exist (spawn: 464 traced, 466 counted).
Under the debugger `malloc` and `free` take slower paths, so allocator
totals from a trace are too high; they are reported separately and
marked.

**Read from the source.** A few stub sizes are counted from the assembly
in the source and say so.

Things to know when reading the numbers:

- `instructions retired` covers every thread of the process and the
  kernel. A system call that does nothing costs about 1,090 on this
  machine (`getppid` in a loop). So anything that crosses a thread or
  maps memory is dominated by the kernel, and Go's totals include its
  background threads. Where it matters a `GOMAXPROCS=1` figure is given
  too.
- Go functions are marked `//go:noinline` where the BAML probe defeats
  inlining. Go's numbers are for its own idiom of the same operation,
  shown as source next to each.
- Node numbers include its JIT tiers and its other threads, and vary
  with warm-up. They are approximate and given only for calls,
  allocation, closures, awaits and startup.
- Instructions are not time. Wall time is given where a context switch
  is involved.

## Reproducing

The probes, the Go and Node sources, the scripts and every raw count are
kept outside the repository, in the session scratch directory
`scratchpad/costmap/`:

| file | what |
|---|---|
| `probes/*.baml` | the BAML probes, with `__N__` and `__M__` for the counts |
| `mk.sh <probe> <N> [M]` | substitute and build with `native_pack --emit-asm` |
| `cnt.sh <exe> [args]` | fewest `instructions retired` of 7 runs |
| `run.sh <probe> <N lo> <N hi> [M]` | build both, count both, print the per-iteration cost |
| `dis.sh <exe> <symbol regex>` | disassemble one function of the linked probe |
| `trace.py`, `tr.sh` | the `lldb` single-step tally; `sum.py`, `seq.py` summarize a trace |
| `rus.py` | read the process's instruction counter at breakpoints (startup) |
| `go/bench/main.go`, `node/bench.js` | the equivalents, one mode per probe |
| `raw/counts.tsv` | every counted measurement: label, N lo, N hi, both counts, per operation |
| `raw/trace.*.txt` | every trace |

To rebuild the compiler entry point without touching the test corpus:

```sh
cd baml_language
CARGO_BUILD_JOBS=4 CARGO_PROFILE_FASTTEST_LTO=off \
  cargo build --profile fasttest -p baml_compiler2_native --example native_pack
BAML_TELEMETRY=off target/fasttest/examples/native_pack probe.baml main out --emit-asm
```

## Where native stands against Go

Per operation, instructions, from the documents:

| | native | Go |
|---|---|---|
| direct call and return | 13 | 16 |
| interface dispatch (sequence only) | 5 | 3 |
| allocate a 16-byte object | 10 | 146 |
| map lookup, string key | 37 to 39 | 138 to 155 |
| map store to an existing string key | 268 to 289 | 170 |
| string concatenation, 23 bytes | 196 | 383 |
| collection, per live object | 122 | 376 |
| collection, fixed | 255 | 1,122,109 |
| throw caught one frame up | 250 | 3,073 (`panic`), 181 (error return) |
| each further frame of a throw | 48 | 285 (`panic`), 14 (error return) |
| `spawn` and `await` | 466 | 4,645 |
| hand work to the I/O thread and get it back | 39,852 | 3,711 (goroutine and channels) |
| `==` on two small class instances | 3,268 | a few compares (not measured) |
| start a program that returns 42 | 14.51 M | 17.93 M |

## Not measured

- A cgo call, as the Go side of a native call: cgo did not build in this
  environment (the SDK path the toolchain wants is missing).
- An `await` that parks on an unsettled future, in isolation: with one
  worker and child-first spawn it only happens behind another park.
- The allocation slow path stub alone: its 59 instructions are counted
  from the source.
- The `baml-io` thread's share of an I/O round trip in user mode: only
  the whole-process total and the worker's trace exist.
- A truly fresh task stack: the probe reuses stacks whose tasks have
  already ended.
- Startup under a single-step trace from `main`: `gettimeofday` spins
  under the debugger. The breakdown uses the process counter at
  breakpoints instead, plus a trace from the stack-map decode onward.
- Go's object sizes in `memory.md` are from the Go runtime's definitions,
  not measured.
- x86-64: the stubs exist in the source and have never been built or
  run.
