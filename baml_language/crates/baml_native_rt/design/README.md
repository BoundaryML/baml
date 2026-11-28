# Design notes: mechanisms and what they cost

Each document describes one part of the native backend as it is built: how it works, the code that does it, a BAML example, the arm64 that comes out, and the measured cost next to Go and, where it means something, Node.

| document | covers |
|---|---|
| `calls.md` | The two-word calling convention and pinned register, direct calls, interface dispatch through class objects and tag rows, function values and the layout check, thunks between layouts. |
| `natives.md` | Plain, `allocates` and `async` natives, the anchor stub, the compiled driver stub and the poll path, `scope.call_baml`, and the chain through `scope.io` to Tokio and back. |
| `memory.md` | The two-word value, tags, object layouts with their sizes, inline allocation and its slow path, field and index access under the busy bit, maps, string concatenation. |
| `gc.md` | The copying collector, the scan by tag, root finding, stack maps with word and value entries, what a checked build asserts, cost per collection, per live object, per byte and per frame. |
| `control.md` | Loop polls, the entry stack check, stack sizing, throw and catch, the cost of a handler that is not used. |
| `tasks.md` | The task record and switch frame, `spawn`, `await`, parking on timers, futures and I/O, the wake from the I/O thread. |
| `threads.md` | Which OS threads exist, per-worker state, what migration stress checks, and the list of everything that assumes one worker, with file and line. |
| `startup-and-size.md` | Instructions before the first line of user code, where they go, and binary sizes. |
| `bridges.md` | Calling compiled BAML from Python, Node and Rust: the library form, the exported C functions, handles, how a call starts and finishes, and why results are copied. |

## How the numbers were measured

Two rounds, on one machine (Apple M2 Max, arm64).

- At `39d3ad4c25`, macOS 26.7.1, before values were two words: every number in these documents at first, with Go 1.26.0 (`go build`, default flags) and Node v22.23.4. Every Go and Node number, every single-step trace, the I/O chain and the startup breakdown are still from this round.
- At `881e4c1dfc`, macOS 27.2.1, after: the counted native numbers were taken again with probes written again from the descriptions here (the first round's probes and scripts were lost). The documents give both and say "before" for the first round. Go and Node were not run again. A C program that returns 42 retired 11,073,643 instructions in the first round and 11,670,648 in the second, so the system's own share of a process moved by 0.6 M between them: only differences within a round are exact, and anything dominated by the kernel or the dynamic linker is not comparable across rounds to better than that.

Branch `claude/baml-native-backend`. The compiler is the `native_pack` example built with `--profile fasttest`; the runtime archive it links is always built `opt-level=3`, `codegen-units=1`, `panic=abort`, whatever the compiler's profile (`baml_compiler2_native/build.rs`).

Three methods. Each number says which it came from.

**Counted.** A probe is a small BAML program built to an executable at two iteration counts. Instructions are `instructions retired` from `/usr/bin/time -l`, the fewest of 7 runs. Cost per operation is the difference of the two counts over the difference in iterations. The documents give both raw counts. Where a probe needs a loop around the thing measured, a second probe with the same loop and a trivial body is subtracted, and both are shown. Each loop was disassembled to check that the operation is still there: MIR inlining, scalar replacement and devirtualization delete naive probes (a small callee is inlined, an object that does not escape is never allocated, a call on a known class is direct).

**Traced** (first round only). For a breakdown by function, one iteration is single-stepped under `lldb` from a breakpoint and the instructions are tallied by symbol. These are user-mode instructions of one thread. They agree with the counted totals where both exist (spawn: 464 traced, 466 counted). Under the debugger `malloc` and `free` take slower paths, so allocator totals from a trace are too high; they are reported separately and marked.

**Read from the source.** A few stub sizes are counted from the assembly in the source and say so.

Things to know when reading the numbers:

- `instructions retired` covers every thread of the process and the kernel. A system call that does nothing costs about 1,090 on this machine (`getppid` in a loop). So anything that crosses a thread or maps memory is dominated by the kernel, and Go's totals include its background threads. Where it matters a `GOMAXPROCS=1` figure is given too.
- Go functions are marked `//go:noinline` where the BAML probe defeats inlining. Go's numbers are for its own idiom of the same operation, shown as source next to each.
- Node numbers include its JIT tiers and its other threads, and vary with warm-up. They are approximate and given only for calls, allocation, closures, awaits and startup.
- Instructions are not time. Wall time is given where a context switch is involved.

## Reproducing

The second round's probes, scripts and raw counts are kept outside the repository, in the session scratch directory `scratchpad/uv-impl/costmap/`:

| file | what |
|---|---|
| `probes/*.baml` | the BAML probes, with `__N__` and `__M__` for the counts |
| `mk.sh <probe> <N> [M]` | substitute and build with `native_pack --emit-asm` |
| `cnt.sh <exe> [args]` | fewest `instructions retired` of 7 runs |
| `run.sh <probe> <N lo> <N hi> [M]` | build both, count both, print the per-iteration cost |
| `all.sh` | every counted measurement |
| `dis.sh <exe> <symbol regex>` | disassemble the matching functions of the linked probe |
| `raw/counts.tsv` | every counted measurement: label, N lo, N hi, both counts, per operation |
| `listings/` | a program whose listing shows the thunks and the task shim |

The first round's Go and Node sources, `lldb` trace scripts and traces are gone with its scratch directory.

To rebuild the compiler entry point without touching the test corpus:

```sh
cd baml_language
CARGO_BUILD_JOBS=4 CARGO_PROFILE_FASTTEST_LTO=off \
  cargo build --profile fasttest -p baml_compiler2_native --example native_pack
BAML_TELEMETRY=off target/fasttest/examples/native_pack probe.baml main out --emit-asm
```

## Where native stands against Go

Per operation, instructions, from the documents. Native is the second round with the first in brackets; Go is the first round.

| | native (before) | Go |
|---|---|---|
| direct call and return, three `int` arguments | 17 (13) | 16 |
| interface dispatch (sequence only) | 6 for an instance, 8 for any other receiver (5) | 3 |
| a call through a function value (check, load the code, call) | 7 where no other layout can arrive, 5 otherwise (2, no check) | 2 |
| allocate `P { x: int }` | 12, 32 bytes (10, 16 bytes) | 146 |
| load of a field whose type fixes its tag; of a union field | 2; 7 (1; 1) | 1 |
| store to an `int` field; to any other field | 2; 9 (1; 1) | 1, or 6 with the write barrier's test |
| map lookup, string key | 33 to 35 (37 to 39) | 138 to 155 |
| map store to an existing string key | 284 to 305 (268 to 289) | 170 |
| string concatenation, 23 bytes, whole loop | 232 (222) | 393 |
| collection, per live object | 177 (122) | 376 |
| collection, per element of a large `int[]` | 17.9 (2.96) | 0 |
| collection, fixed | 222 (255) | 1,122,109 |
| throw caught one frame up | 314 (250) | 3,073 (`panic`), 181 (error return) |
| each further frame of a throw | 59 (48) | 285 (`panic`), 14 (error return) |
| `spawn` and `await` | 500 (466) | 4,645 |
| hand work to the I/O thread and get it back | not measured again (39,852) | 3,711 (goroutine and channels) |
| `==` on two small class instances | 4,532 (3,268) | a few compares (not measured) |
| start a program that returns 42, over a C program that does | 4.11 M (3.44 M) | 6.85 M |

What two-word values and the busy bit cost, in one line each: a call passes a tag constant per argument and result; every slot is twice the bytes, so objects are, and the space fills twice as fast; an access to a slot is an address computation and an atomic instruction where it was one load or store, and a store of a pointer or a read of a union is a seven-instruction locked sequence; the collector tests a tag per slot, which a large array of scalars pays in full. What they bought: no boxes, optional scalars in one slot anywhere, one representation in registers, frames, the heap and natives, generic code compiled once, and no type table.

## Not measured

- A cgo call, as the Go side of a native call: cgo did not build in this environment (the SDK path the toolchain wants is missing).
- An `await` that parks on an unsettled future, in isolation: with one worker and child-first spawn it only happens behind another park.
- The allocation slow path stub alone: its 59 instructions are counted from the source.
- In the second round: any single-step trace, so no number there is broken down by function; Go and Node; the I/O round trip, the timer and new-stack probes of `tasks.md`, the catch-region probe of `control.md`, the stack-map decode; a frame holding a value entry under collection; contention on a slot's busy bit.
- The `baml-io` thread's share of an I/O round trip in user mode: only the whole-process total and the worker's trace exist.
- A truly fresh task stack: the probe reuses stacks whose tasks have already ended.
- Startup under a single-step trace from `main`: `gettimeofday` spins under the debugger. The breakdown uses the process counter at breakpoints instead, plus a trace from the stack-map decode onward.
- Go's object sizes in `memory.md` are from the Go runtime's definitions, not measured.
- x86-64: the stubs exist in the source and have never been built or run.
